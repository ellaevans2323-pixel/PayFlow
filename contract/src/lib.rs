#![no_std]

mod admin;
mod batch;
#[cfg(test)]
mod bench;
mod charge_exec;
mod errors;
mod events;
mod fee;
mod grace;
mod merchant_stats;
mod migration;
mod referral;
mod spending_limit;
mod storage;
mod subscription_count;
mod subscription_history;
mod subscription_metadata;
mod test;
mod trial;
mod upgrade;
mod validation;
mod whitelist;

use crate::errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, BytesN, Env, String, Symbol, Vec,
};

pub use batch::ChargeResult;

// ─────────────────────────────────────────────────────────────
// Storage keys
// ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Subscription(Address),
    Token,
    // Admin
    Admin,
    // Grace period
    GracePeriod,
    // Merchant whitelist
    MerchantWhitelist(Address),
    WhitelistEnabled,
    // Protocol fee
    FeeCollector,
    FeeBps,
    // Feature: subscription count
    ActiveCount,
    // Feature: merchant revenue stats
    MerchantRevenue(Address),
    // Per-day merchant revenue buckets (keyed by Unix day)
    MerchantRevenueDay(Address, u64),
    // Feature: daily spending limits (temporary storage)
    DailyLimit(Address),
    DailySpent(Address),
    // Feature: referral tracking
    Referral(Address),
    // Feature: state migration
    SchemaVersion,
    // Feature: subscription metadata labels
    SubscriptionMeta(Address),
    // Feature: charge history
    ChargeHistory(Address),
    // Feature: emergency contract pause
    ContractPaused,
    // Feature: per-merchant subscriber count
    MerchantSubCount(Address),
    // Pending admin for two-step transfer
    PendingAdmin,
}

// ─────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────

pub const SUBSCRIPTION_TTL_LEDGERS: u32 = 6307200; // ~1 year (assuming 5s blocks)
pub const MAX_AMOUNT: i128 = 100_000_000_000;
pub const MAX_SUBSCRIPTION_AMOUNT: i128 = 1_000_000_0000000;

// ─────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Subscription {
    pub merchant: Address,
    pub amount: i128,
    pub interval: u64,
    pub last_charged: u64,
    pub active: bool,
    pub paused: bool,              // true if paused, false otherwise
    pub token: Address,            // SAC token used for this subscription
    pub referrer: Option<Address>, // optional referral address
    pub label: Symbol,             // user-assigned label for this subscription
    pub trial_duration: u64,       // optional trial duration in seconds
}

// ─────────────────────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────────────────────

#[contract]
pub struct FlowPay;

#[contractimpl]
impl FlowPay {
    pub fn initialize(env: Env, token: Address, admin: Address) {
        if env.storage().instance().has(&DataKey::Token) {
            env.panic_with_error(ContractError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Token, &token);
        admin::initialize_admin(&env, &admin);
    }

    /// Creates or replaces a recurring subscription for `user`.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber address. Must authorize the call.
    /// - `merchant`: Recipient that receives recurring and pay-per-use transfers.
    /// - `amount`: Amount transferred per billing period. Must be greater than zero.
    /// - `interval`: Billing cadence in seconds. Must be greater than zero.
    /// - `token`: Stellar Asset Contract used for this subscription.
    /// - `trial_period`: Optional seconds to delay the first charge.
    /// - `referrer`: Optional referrer stored for the subscriber.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// Requires authorization from `user`.
    ///
    /// # Errors
    ///
    /// Panics if the contract is paused, the merchant whitelist rejects `merchant`,
    /// `amount` or `interval` is zero, or the contract allowance is below `amount`.
    ///
    /// # Side Effects
    ///
    /// Stores the subscription, refreshes its TTL, updates active subscription
    /// count and referral storage, and emits `subscribed`.
    pub fn subscribe(
        env: Env,
        user: Address,
        merchant: Address,
        amount: i128,
        interval: u64,
        token: Address,
        trial_period: Option<u64>,
        referrer: Option<Address>,
    ) {
        ensure_contract_not_paused(&env);
        user.require_auth();

        if whitelist::is_whitelist_enabled(&env) {
            if !whitelist::is_whitelisted(&env, &merchant) {
                env.panic_with_error(ContractError::MerchantNotWhitelisted);
            }
        }

        if amount <= 0 {
            env.panic_with_error(ContractError::AmountMustBePositive);
        }
        if interval == 0 {
            env.panic_with_error(ContractError::IntervalMustBePositive);
        }

        use soroban_sdk::xdr::ToXdr;
        if token.clone().to_xdr(&env).get(7) == Some(0) {
            env.panic_with_error(ContractError::InvalidTokenAddress);
        }

        validation::check_allowance(&env, &user, &token, amount);

        let now = env.ledger().timestamp();
        let trial_duration = trial_period.unwrap_or(0);
        let last_charged = now + trial_duration;

        let sub = Subscription {
            merchant,
            amount,
            interval,
            last_charged,
            active: true,
            paused: false,
            token,
            referrer: referrer.clone(),
            label: Symbol::new(&env, "default"),
            trial_duration,
        };

        let existing_sub: Option<Subscription> = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(user.clone()));

        let should_increment = existing_sub
            .as_ref()
            .map(|existing| !existing.active)
            .unwrap_or(true);

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(user.clone()), &sub);

        extend_subscription_ttl(&env, &user);

        if should_increment {
            subscription_count::increment(&env);
        }
        referral::store_referral(&env, &user, &referrer);
        merchant_stats::increment_subscriber_count(&env, &sub.merchant);
        events::publish_subscribed(&env, &user, &sub);
    }

    /// Charges the next due recurring payment for `user`.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber whose active subscription should be charged.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// No subscriber signature is required. The contract spends through the
    /// previously granted token allowance.
    ///
    /// # Errors
    ///
    /// Panics if the contract is paused, no subscription exists, the subscription
    /// is inactive or paused, the interval has not elapsed, the grace period has
    /// elapsed, or token transfer authorization/allowance is insufficient.
    ///
    /// # Side Effects
    ///
    /// Transfers `amount` from `user` to the merchant, records merchant revenue
    /// and charge history, refreshes subscription TTL, updates `last_charged`,
    /// and emits `charged`.
    pub fn charge(env: Env, user: Address) {
        ensure_contract_not_paused(&env);
        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        if !sub.active {
            env.panic_with_error(ContractError::SubscriptionNotActive);
        }
        if sub.paused {
            env.panic_with_error(ContractError::SubscriptionPaused);
        }

        let now = env.ledger().timestamp();

        if now < sub.last_charged + sub.interval {
            env.panic_with_error(ContractError::IntervalNotElapsed);
        }

        let grace_period = grace::get_grace_period(&env);
        if grace_period > 0 && now > sub.last_charged + sub.interval + grace_period {
            env.panic_with_error(ContractError::GracePeriodElapsed);
        }

        charge_exec::execute_charge(&env, &user, &key, &mut sub, now);
    }

    pub fn extend_subscription_ttl(env: Env, user: Address) {
        extend_subscription_ttl(&env, &user);
    }

    /// Executes an immediate pay-per-use charge for an active subscription.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber address. Must authorize the call.
    /// - `amount`: One-time amount to transfer. Must be greater than zero.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// Requires authorization from `user`.
    ///
    /// # Errors
    ///
    /// Panics if the contract is paused, `amount` is zero, no subscription
    /// exists, the subscription is inactive or paused, the daily spending limit
    /// would be exceeded, or token transfer authorization/allowance is insufficient.
    ///
    /// # Side Effects
    ///
    /// Transfers `amount` to the subscription merchant, updates merchant revenue
    /// and daily spend tracking, and emits `pay_per_use`.
    pub fn pay_per_use(env: Env, user: Address, amount: i128) {
        ensure_contract_not_paused(&env);
        user.require_auth();

        if amount <= 0 {
            env.panic_with_error(ContractError::AmountMustBePositive);
        }
        if amount > MAX_AMOUNT {
            env.panic_with_error(ContractError::AmountExceedsMaximum);
        }

        let key = DataKey::Subscription(user.clone());

        let sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        if !sub.active {
            env.panic_with_error(ContractError::SubscriptionNotActive);
        }
        if sub.paused {
            env.panic_with_error(ContractError::SubscriptionPaused);
        }

        spending_limit::enforce_limit(&env, &user, amount);

        let token = token::Client::new(&env, &sub.token);

        let mut merchant_amount = amount;
        if let Some((collector, bps)) = fee::get_fee(&env) {
            let fee_amount = (amount * (bps as i128)) / 10_000;
            if fee_amount > 0 {
                token.transfer_from(
                    &env.current_contract_address(),
                    &user,
                    &collector,
                    &fee_amount,
                );
                merchant_amount = amount - fee_amount;
            }
        }

        token.transfer_from(
            &env.current_contract_address(),
            &user,
            &sub.merchant,
            &merchant_amount,
        );

        merchant_stats::increment_revenue_with_daily(&env, &sub.merchant, merchant_amount);
        spending_limit::record_spend(&env, &user, amount);

        events::publish_pay_per_use(&env, &user, &sub.merchant, amount);
    }

    /// Cancels `user`'s active subscription.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber address. Must authorize the call.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// Requires authorization from `user`.
    ///
    /// # Errors
    ///
    /// Panics if no subscription exists for `user`.
    ///
    /// # Side Effects
    ///
    /// Marks the subscription inactive, decrements active subscription count, and
    /// emits `cancelled`.
    pub fn cancel(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        sub.active = false;

        env.storage().persistent().set(&key, &sub);

        subscription_count::decrement(&env);
        merchant_stats::decrement_subscriber_count(&env, &sub.merchant);
        events::publish_cancelled(&env, &user);
    }

    /// Pauses `user`'s subscription without cancelling it.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber address. Must authorize the call.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// Requires authorization from `user`.
    ///
    /// # Errors
    ///
    /// Panics if no subscription exists or the subscription is inactive.
    ///
    /// # Side Effects
    ///
    /// Sets the subscription `paused` flag and emits `paused`.
    pub fn pause(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        if !sub.active {
            env.panic_with_error(ContractError::SubscriptionNotActive);
        }

        sub.paused = true;

        env.storage().persistent().set(&key, &sub);

        events::publish_paused(&env, &user);
    }

    /// Resumes `user`'s paused subscription.
    ///
    /// # Parameters
    ///
    /// - `user`: Subscriber address. Must authorize the call.
    ///
    /// # Returns
    ///
    /// Returns nothing.
    ///
    /// # Auth
    ///
    /// Requires authorization from `user`.
    ///
    /// # Errors
    ///
    /// Panics if no subscription exists or the subscription is inactive.
    ///
    /// # Side Effects
    ///
    /// Clears the subscription `paused` flag and emits `resumed`.
    pub fn resume(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        if !sub.active {
            env.panic_with_error(ContractError::SubscriptionNotActive);
        }

        sub.paused = false;

        env.storage().persistent().set(&key, &sub);

        events::publish_resumed(&env, &user);
    }

    /// Pauses all user-facing payment operations for the contract.
    pub fn pause_contract(env: Env) {
        admin::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::ContractPaused, &true);
        events::publish_contract_paused(&env);
    }

    /// Unpauses user-facing payment operations for the contract.
    pub fn unpause_contract(env: Env) {
        admin::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::ContractPaused, &false);
        events::publish_contract_unpaused(&env);
    }

    /// Proposes a new admin (step 1 of two-step transfer).
    /// The proposed address must call `accept_admin()` to complete the transfer.
    ///
    /// # Auth
    ///
    /// Requires authorization from the current admin.
    pub fn transfer_admin(env: Env, new_admin: Address) {
        admin::transfer_admin(&env, &new_admin);
    }

    /// Accepts a pending admin transfer (step 2 of two-step transfer).
    /// Emits `admin_transferred` and replaces the active admin.
    ///
    /// # Auth
    ///
    /// Requires authorization from the pending (new) admin.
    pub fn accept_admin(env: Env) {
        admin::accept_admin(&env);
    }

    /// Returns whether the contract is currently paused.
    pub fn is_contract_paused(env: Env) -> bool {
        is_contract_paused(&env)
    }

    /// Returns the current admin address, or `None` if no admin has been set.
    pub fn get_admin(env: Env) -> Option<Address> {
        storage::get_admin_optional(&env)
    }

    /// Returns the default token address set during `initialize()`, or `None` if not initialized.
    pub fn get_token(env: Env) -> Option<Address> {
        storage::get_token(&env)
    }

    /// Upgrades the current contract WASM to `new_wasm_hash`.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        admin::require_admin(&env);
        upgrade::upgrade(&env, new_wasm_hash);
    }

    pub fn get_subscription(env: Env, user: Address) -> Option<Subscription> {
        env.storage().persistent().get(&DataKey::Subscription(user))
    }

    /// Returns the Unix timestamp of the next scheduled charge for a user.
    ///
    /// Returns `None` if:
    /// - No subscription exists for the user
    /// - The subscription is inactive (cancelled)
    ///
    /// Returns `Some(last_charged + interval)` if the subscription is active.
    pub fn next_charge_at(env: Env, user: Address) -> Option<u64> {
        let sub = storage::get_subscription(&env, &user)?;
        if !sub.active {
            None
        } else {
            Some(sub.last_charged + sub.interval)
        }
    }

    /// Returns the trial end timestamp if the user is in a trial period.
    pub fn get_trial_end(env: Env, user: Address) -> Option<u64> {
        trial::get_trial_end(env, user)
    }

    /// Sets the contract-wide grace period for charges.
    /// Only the contract admin can call this.
    pub fn set_grace_period(env: Env, seconds: u64) {
        admin::require_admin(&env);
        grace::set_grace_period(&env, seconds);
        events::publish_grace_period_updated(&env, seconds);
    }

    /// Returns the current grace period in seconds. Returns 0 if not set.
    pub fn get_grace_period(env: Env) -> u64 {
        grace::get_grace_period(&env)
    }

    /// Adds a merchant to the whitelist.
    pub fn add_merchant(env: Env, merchant: Address) {
        admin::require_admin(&env);
        whitelist::add_merchant(&env, &merchant);
    }

    /// Removes a merchant from the whitelist.
    pub fn remove_merchant(env: Env, merchant: Address) {
        admin::require_admin(&env);
        whitelist::remove_merchant(&env, &merchant);
    }

    /// Enables or disables the merchant whitelist.
    pub fn set_whitelist_enabled(env: Env, enabled: bool) {
        admin::require_admin(&env);
        whitelist::set_whitelist_enabled(&env, enabled);
    }

    /// Returns whether the merchant whitelist is currently enabled.
    pub fn is_whitelist_enabled(env: Env) -> bool {
        whitelist::is_whitelist_enabled(&env)
    }

    /// Returns whether a merchant is whitelisted.
    pub fn is_merchant_whitelisted(env: Env, merchant: Address) -> bool {
        whitelist::is_whitelisted(&env, &merchant)
    }

    /// Returns the current protocol fee settings, or `None` if unset.
    pub fn get_fee(env: Env) -> Option<(Address, u32)> {
        fee::get_fee_collector(&env).map(|collector| (collector, fee::get_fee_bps(&env)))
    }

    /// Sets the protocol fee collection settings.
    /// Only the contract admin can call this.
    pub fn set_fee(env: Env, collector: Address, bps: u32) {
        admin::require_admin(&env);
        fee::set_fee(&env, collector.clone(), bps);
        events::publish_fee_updated(&env, &collector, bps);
    }

    // ─────────────────────────────────────────────────────────────
    // Batch charge
    // ─────────────────────────────────────────────────────────────

    /// Charges multiple subscribers in a single transaction.
    ///
    /// Each user is processed independently — individual failures (inactive,
    /// paused, interval not elapsed, etc.) are recorded as a `ChargeResult`
    /// variant and do **not** abort the batch.
    pub fn batch_charge(env: Env, users: Vec<Address>) -> Vec<ChargeResult> {
        ensure_contract_not_paused(&env);
        batch::batch_charge(&env, users)
    }

    // ─────────────────────────────────────────────────────────────
    // Subscription count
    // ─────────────────────────────────────────────────────────────

    /// Returns the current number of active subscriptions.
    pub fn get_active_count(env: Env) -> u64 {
        subscription_count::get_active_count(&env)
    }

    // ─────────────────────────────────────────────────────────────
    // Merchant revenue
    // ─────────────────────────────────────────────────────────────

    /// Returns the total amount charged to a merchant's subscribers
    /// (sum of all successful `charge()` and `pay_per_use()` calls).
    pub fn get_merchant_revenue(env: Env, merchant: Address) -> i128 {
        merchant_stats::get_merchant_revenue(&env, &merchant)
    }

    /// Returns per-day revenue for the given merchant for the last `days` days.
    /// Oldest -> newest.
    pub fn get_merchant_revenue_history(env: Env, merchant: Address, days: u32) -> Vec<i128> {
        merchant_stats::get_merchant_revenue_history(&env, &merchant, days)
    }

    /// Returns the number of active subscribers for a given merchant.
    pub fn get_merchant_subscriber_count(env: Env, merchant: Address) -> u64 {
        merchant_stats::get_merchant_subscriber_count(&env, &merchant)
    }

    /// Resets a merchant's cumulative revenue counter to zero.
    /// Only the contract admin can call this.
    pub fn reset_merchant_revenue(env: Env, merchant: Address) {
        admin::require_admin(&env);
        merchant_stats::reset_merchant_revenue(&env, &merchant);
    }

    // ─────────────────────────────────────────────────────────────
    // Daily spending limits
    // ─────────────────────────────────────────────────────────────

    /// Sets a daily spending cap for `pay_per_use()` for the calling user.
    /// Stored in temporary storage; resets automatically after ~1 day.
    pub fn set_daily_limit(env: Env, user: Address, limit: i128) {
        user.require_auth();
        if limit <= 0 {
            env.panic_with_error(ContractError::AmountMustBePositive);
        }
        spending_limit::set_daily_limit(&env, &user, limit);
        events::publish_daily_limit_set(&env, &user, limit);
    }

    /// Removes the caller's daily spending cap for `pay_per_use()`.
    pub fn remove_daily_limit(env: Env, user: Address) {
        user.require_auth();
        spending_limit::remove_daily_limit(&env, &user);
        events::publish_daily_limit_removed(&env, &user);
    }

    /// Returns the current daily spending limit for the caller, or `None` if unset.
    pub fn get_daily_limit(env: Env, user: Address) -> Option<i128> {
        spending_limit::get_daily_limit(&env, &user)
    }

    /// Returns the amount spent so far today via `pay_per_use()` for the caller.
    pub fn get_daily_spent(env: Env, user: Address) -> i128 {
        spending_limit::get_daily_spent(&env, &user)
    }

    // ─────────────────────────────────────────────
    // Referral tracking
    // ─────────────────────────────────────────────────────────────

    /// Returns the referrer address for a given subscriber, or `None`.
    pub fn get_referrer(env: Env, user: Address) -> Option<Address> {
        referral::get_referrer(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // State migration
    // ─────────────────────────────────────────────────────────────

    /// Migrates contract storage to the latest schema version.
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn migrate(env: Env, users: Vec<Address>) {
        migration::migrate(&env, users);
    }

    /// Returns the current storage schema version.
    pub fn get_schema_version(env: Env) -> u32 {
        migration::get_schema_version(&env)
    }

    // ─────────────────────────────────────────────────────────────
    // Subscription metadata
    // ─────────────────────────────────────────────────────────────

    /// Attaches a short label (e.g. plan name) to the caller's subscription.
    pub fn set_metadata(env: Env, user: Address, label: String) {
        user.require_auth();
        if let Err(err) = subscription_metadata::set_metadata(&env, &user, label) {
            env.panic_with_error(err);
        }
    }

    /// Returns the metadata label for a subscriber, or `None` if not set.
    pub fn get_metadata(env: Env, user: Address) -> Option<String> {
        subscription_metadata::get_metadata(&env, &user)
    }

    /// Clears the metadata label for the caller's subscription.
    pub fn clear_metadata(env: Env, user: Address) {
        user.require_auth();
        subscription_metadata::clear_metadata(&env, &user);
    }

    // ─────────────────────────────────────────────────────────────
    // Charge history
    // ─────────────────────────────────────────────────────────────

    /// Returns the last (up to 12) charge timestamps for a subscriber,
    /// ordered oldest → newest.
    pub fn get_charge_history(env: Env, user: Address) -> Vec<u64> {
        subscription_history::get_charge_history(&env, &user)
    }

    /// Clears the charge history for a subscriber.
    pub fn clear_charge_history(env: Env, user: Address) {
        user.require_auth();
        subscription_history::clear_charge_history(&env, &user);
    }

    /// Returns a paginated slice of charge timestamps for a subscriber.
    /// limit is capped at 12.
    pub fn get_charge_history_page(env: Env, user: Address, offset: u32, limit: u32) -> Vec<u64> {
        subscription_history::get_charge_history_page(&env, &user, offset, limit)
    }
}

fn extend_subscription_ttl(env: &Env, user: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Subscription(user.clone()),
        SUBSCRIPTION_TTL_LEDGERS,
        SUBSCRIPTION_TTL_LEDGERS,
    );
}

fn is_contract_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::ContractPaused)
        .unwrap_or(false)
}

fn ensure_contract_not_paused(env: &Env) {
    if is_contract_paused(env) {
        env.panic_with_error(ContractError::ContractPaused);
    }
}
