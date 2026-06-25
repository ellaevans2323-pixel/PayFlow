#![no_std]

mod admin;
mod batch;
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
mod validation;
mod whitelist;

use crate::errors::ContractError;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

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
    // Feature: global volume cap
    GlobalVolumeWindow,
    // Feature: contract pause
    ContractPaused,
}

// ─────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────

pub const SUBSCRIPTION_TTL_LEDGERS: u32 = 6307200; // ~1 year (assuming 5s blocks)
pub const GLOBAL_MAX_VOLUME_PER_HOUR: i128 = 50_000_000_000_000; // 50 trillion stroops
pub const HOUR_IN_SECONDS: u64 = 3600;

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
    pub paused: bool,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalVolumeWindow {
    pub current_window_start: u64,
    pub accumulated_volume: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolStats {
    pub active_count: u64,
    pub fee_bps: u32,
    pub fee_collector: Option<Address>,
    pub grace_period: u64,
    pub whitelist_enabled: bool,
    pub schema_version: u32,
    pub contract_paused: bool,
}

// ─────────────────────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────────────────────

#[contract]
pub struct FlowPay;

#[contractimpl]
impl FlowPay {
    pub fn initialize(env: Env, token: Address) {
        if env.storage().instance().has(&DataKey::Token) {
            panic!("already initialized");
        }

        env.storage().instance().set(&DataKey::Token, &token);
    }

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
        subscribe_inner(&env, user, merchant, amount, interval, token, trial_period, referrer);
    }

    pub fn subscribe_with_metadata(
        env: Env,
        user: Address,
        merchant: Address,
        amount: i128,
        interval: u64,
        token: Address,
        trial_period: Option<u64>,
        referrer: Option<Address>,
        label: String,
    ) {
        // Validate label length before any storage writes
        if label.len() > 64 {
            env.panic_with_error(ContractError::MetadataLabelTooLong);
        }
        // require_auth called inside subscribe_inner
        subscribe_inner(&env, user.clone(), merchant, amount, interval, token, trial_period, referrer);
        subscription_metadata::set_metadata(&env, &user, label);
    }

    pub fn charge(env: Env, user: Address) {
        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        assert!(sub.active, "subscription is not active");
        assert!(!sub.paused, "subscription is paused");

        let now = env.ledger().timestamp();

        if now < sub.last_charged + sub.interval {
            env.panic_with_error(ContractError::IntervalNotElapsed);
        }

        let grace_period = grace::get_grace_period(&env);
        if grace_period > 0 && now > sub.last_charged + sub.interval + grace_period {
            env.panic_with_error(ContractError::GracePeriodElapsed);
        }

        let token = token::Client::new(&env, &sub.token);

        token.transfer_from(
            &env.current_contract_address(),
            &user,
            &sub.merchant,
            &sub.amount,
        );

        check_and_update_global_volume(&env, sub.amount);
        merchant_stats::increment_revenue(&env, &sub.merchant, sub.amount);

        sub.last_charged = now;

        env.storage().persistent().set(&key, &sub);
        extend_subscription_ttl(&env, &user);

        subscription_history::record_charge(&env, &user, now);
        events::publish_charged(&env, &user, &sub, now);
    }

    pub fn extend_subscription_ttl(env: Env, user: Address) {
        extend_subscription_ttl(&env, &user);
    }

    pub fn pay_per_use(env: Env, user: Address, amount: i128) {
        user.require_auth();

        assert!(amount > 0, "amount must be positive");

        let key = DataKey::Subscription(user.clone());

        let sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");
        assert!(!sub.paused, "subscription is paused");

        spending_limit::enforce_limit(&env, &user, amount);

        let token = token::Client::new(&env, &sub.token);

        token.transfer_from(
            &env.current_contract_address(),
            &user,
            &sub.merchant,
            &amount,
        );

        check_and_update_global_volume(&env, amount);
        merchant_stats::increment_revenue(&env, &sub.merchant, amount);
        spending_limit::record_spend(&env, &user, amount);

        events::publish_pay_per_use(&env, &user, &sub.merchant, amount);
    }

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
        events::publish_cancelled(&env, &user);
    }

    pub fn pause(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");

        sub.paused = true;

        env.storage().persistent().set(&key, &sub);

        env.events()
            .publish((Symbol::new(&env, "paused"), user), ());
    }

    pub fn resume(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");

        sub.paused = false;

        env.storage().persistent().set(&key, &sub);

        env.events()
            .publish((Symbol::new(&env, "resumed"), user), ());
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

    /// Sets the protocol fee collection settings.
    /// Only the contract admin can call this.
    pub fn set_fee(env: Env, collector: Address, bps: u32) {
        admin::require_admin(&env);
        fee::set_fee(&env, collector, bps);
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

    // ─────────────────────────────────────────────────────────────
    // Daily spending limits
    // ─────────────────────────────────────────────────────────────

    /// Sets a daily spending cap for `pay_per_use()` for the calling user.
    /// Stored in temporary storage; resets automatically after ~1 day.
    pub fn set_daily_limit(env: Env, user: Address, limit: i128) {
        user.require_auth();
        assert!(limit > 0, "limit must be positive");
        spending_limit::set_daily_limit(&env, &user, limit);
    }

    // ─────────────────────────────────────────────────────────────
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
    pub fn migrate(env: Env) {
        migration::migrate(&env);
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
        subscription_metadata::set_metadata(&env, &user, label);
    }

    /// Returns the metadata label for a subscriber, or `None` if not set.
    pub fn get_metadata(env: Env, user: Address) -> Option<String> {
        subscription_metadata::get_metadata(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // Charge history
    // ─────────────────────────────────────────────────────────────

    /// Returns the last (up to 12) charge timestamps for a subscriber,
    /// ordered oldest → newest.
    pub fn get_charge_history(env: Env, user: Address) -> Vec<u64> {
        subscription_history::get_charge_history(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // Protocol stats
    // ─────────────────────────────────────────────────────────────

    /// Returns a snapshot of all protocol-level state in a single call.
    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        ProtocolStats {
            active_count: subscription_count::get_active_count(&env),
            fee_bps: fee::get_fee_bps(&env),
            fee_collector: fee::get_fee_collector(&env),
            grace_period: grace::get_grace_period(&env),
            whitelist_enabled: whitelist::is_whitelist_enabled(&env),
            schema_version: migration::get_schema_version(&env),
            contract_paused: env
                .storage()
                .instance()
                .get(&DataKey::ContractPaused)
                .unwrap_or(false),
        }
            /// Returns a snapshot of all protocol-level state in a single call.
            /// Useful for frontends and off-chain monitoring to get a complete view of the protocol.
            /// 
            /// # Returns
            /// A `ProtocolStats` struct containing:
            /// - `active_count`: Number of active subscriptions
            /// - `fee_bps`: Current protocol fee in basis points
            /// - `fee_collector`: Address receiving protocol fees
            /// - `grace_period`: Grace period in seconds for charging
            /// - `whitelist_enabled`: Whether merchant whitelist is enforced
            /// - `schema_version`: Current contract schema version
            /// - `contract_paused`: Whether the contract is globally paused

    // ─────────────────────────────────────────────────────────────
    // Contract pause
    // ─────────────────────────────────────────────────────────────

    /// Pauses the contract globally. Only the admin can call this.
    pub fn pause_contract(env: Env) {
        admin::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::ContractPaused, &true);
    }
        /// Pauses the contract globally. Only the admin can call this.
        /// When paused, all charging operations (charge, batch_charge, pay_per_use) will fail.
        /// Subscriptions remain intact and can be resumed by unpausing the contract.
        /// Useful for emergency stops during security incidents or maintenance.
    /// Unpauses the contract globally. Only the admin can call this.
    pub fn unpause_contract(env: Env) {
        admin::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::ContractPaused, &false);
    }
        /// Unpauses the contract globally. Only the admin can call this.
        /// Resumes normal operation of all charging functions.
        /// No subscription state is affected; operations resume as normal.

fn extend_subscription_ttl(env: &Env, user: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Subscription(user.clone()),
        SUBSCRIPTION_TTL_LEDGERS,
        SUBSCRIPTION_TTL_LEDGERS,
    );
}

fn subscribe_inner(
    env: &Env,
    user: Address,
    merchant: Address,
    amount: i128,
    interval: u64,
    token: Address,
    trial_period: Option<u64>,
    referrer: Option<Address>,
) {
    user.require_auth();

    if whitelist::is_whitelist_enabled(env) {
        if !whitelist::is_whitelisted(env, &merchant) {
            env.panic_with_error(ContractError::MerchantNotWhitelisted);
        }
    }

    // Prevent new subscriptions when contract is paused
    let paused = env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::ContractPaused)
        .unwrap_or(false);
    if paused {
        env.panic_with_error(ContractError::ContractPausedError);
    }

    assert!(amount > 0, "amount must be positive");
    assert!(interval > 0, "interval must be positive");

    let token_client = token::Client::new(env, &token);
    let allowance = token_client.allowance(&user, &env.current_contract_address());
    assert!(allowance >= amount, "insufficient allowance");

    let now = env.ledger().timestamp();
    let last_charged = match trial_period {
        Some(period) => now + period,
        None => now,
    };

    let sub = Subscription {
        merchant,
        amount,
        interval,
        last_charged,
        active: true,
        paused: false,
        token,
            /// Creates a new subscription with optional metadata label in a single atomic transaction.
            /// This combines subscribe + set_subscription_label to reduce transaction costs.
            /// 
            /// # Arguments
            /// * `user` - The subscriber's address (must authorize the transaction)
            /// * `merchant` - The recipient of payments
            /// * `amount` - Payment amount per interval
            /// * `interval` - Time between charges in seconds
            /// * `token` - Token contract address
            /// * `trial_period` - Optional grace period before first charge
            /// * `referrer` - Optional referrer address for rewards
            /// * `label` - User-defined label (max 64 bytes)
            ///
            /// # Panics
            /// - If label exceeds 64 bytes
            /// - If whitelist is enabled and merchant not whitelisted
            /// - If insufficient token allowance
            pub fn subscribe_with_metadata(
                env: Env,
                user: Address,
                merchant: Address,
                amount: i128,
                interval: u64,
                token: Address,
                trial_period: Option<u64>,
                referrer: Option<Address>,
                label: String,
            ) {
                // Validate label length before any storage writes
                if label.len() > 64 {
                    env.panic_with_error(ContractError::MetadataLabelTooLong);
                }
                // require_auth called inside subscribe_inner
                subscribe_inner(&env, user.clone(), merchant, amount, interval, token, trial_period, referrer);
                subscription_metadata::set_metadata(&env, &user, label);
            }
        .unwrap_or(GlobalVolumeWindow {
            current_window_start: now,
            accumulated_volume: 0,
        });

        // Check if contract is paused
        let paused = env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::ContractPaused)
            .unwrap_or(false);
        if paused {
            env.panic_with_error(ContractError::ContractPausedError);
        }

    if now >= window.current_window_start + HOUR_IN_SECONDS {
        window.current_window_start = now;
        window.accumulated_volume = 0;
    }

    let new_volume = window
        .accumulated_volume
        .checked_add(amount)
        .unwrap_or_else(|| env.panic_with_error(ContractError::GlobalVolumeExceeded));

    if new_volume > GLOBAL_MAX_VOLUME_PER_HOUR {
        env.panic_with_error(ContractError::GlobalVolumeExceeded);
    }

    window.accumulated_volume = new_volume;
    env.storage()
        .instance()
        .set(&DataKey::GlobalVolumeWindow, &window);
}
