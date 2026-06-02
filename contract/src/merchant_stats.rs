use soroban_sdk::{Address, Env, Vec};

use crate::DataKey;

/// Returns the total revenue accumulated for a merchant.
pub fn get_merchant_revenue(env: &Env, merchant: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::MerchantRevenue(merchant.clone()))
        .unwrap_or(0i128)
}

/// Adds `amount` to the merchant's running revenue total.
pub fn increment_revenue(env: &Env, merchant: &Address, amount: i128) {
    let current = get_merchant_revenue(env, merchant);
    env.storage().persistent().set(
        &DataKey::MerchantRevenue(merchant.clone()),
        &(current + amount),
    );
}

/// Returns the per-day revenue for the last `days` days (oldest -> newest).
pub fn get_merchant_revenue_history(env: &Env, merchant: &Address, days: u32) -> Vec<i128> {
    let mut out: Vec<i128> = Vec::new(env);
    if days == 0 {
        return out;
    }

    let now = env.ledger().timestamp();
    let today = now / 86400;

    // Iterate from oldest -> newest
    for i in 0..days {
        let day = if today >= (days - 1 - i) as u64 {
            today - (days - 1 - i) as u64
        } else {
            0u64
        };

        let key = DataKey::MerchantRevenueDay(merchant.clone(), day);
        let v: i128 = env.storage().persistent().get(&key).unwrap_or(0i128);
        out.push_back(v);
    }

    out
}

/// Adds `amount` to today's per-merchant revenue bucket, in addition to the cumulative total.
pub fn increment_revenue_with_daily(env: &Env, merchant: &Address, amount: i128) {
    // update cumulative
    increment_revenue(env, merchant, amount);

    let now = env.ledger().timestamp();
    let today = now / 86400;

    let key = DataKey::MerchantRevenueDay(merchant.clone(), today);
    let current_day: i128 = env.storage().persistent().get(&key).unwrap_or(0i128);
    env.storage()
        .persistent()
        .set(&key, &(current_day + amount));
}

/// Returns the number of active subscribers for a merchant.
pub fn get_merchant_subscriber_count(env: &Env, merchant: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::MerchantSubCount(merchant.clone()))
        .unwrap_or(0u64)
}

/// Increments the per-merchant subscriber count by 1.
pub fn increment_subscriber_count(env: &Env, merchant: &Address) {
    let count = get_merchant_subscriber_count(env, merchant);
    env.storage()
        .persistent()
        .set(&DataKey::MerchantSubCount(merchant.clone()), &(count + 1));
}

/// Decrements the per-merchant subscriber count by 1 (floor 0).
pub fn decrement_subscriber_count(env: &Env, merchant: &Address) {
    let count = get_merchant_subscriber_count(env, merchant);
    if count > 0 {
        env.storage()
            .persistent()
            .set(&DataKey::MerchantSubCount(merchant.clone()), &(count - 1));
    }
/// Resets a merchant's cumulative revenue counter to zero.
pub fn reset_merchant_revenue(env: &Env, merchant: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::MerchantRevenue(merchant.clone()), &0i128);
}
