use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::Subscription;

pub fn publish_subscribed(env: &Env, user: &Address, sub: &Subscription) {
    env.events().publish(
        (Symbol::new(env, "subscribed"), user.clone()),
        (sub.merchant.clone(), sub.amount, sub.interval),
    );
}

pub fn publish_charged(env: &Env, user: &Address, sub: &Subscription, charged_at: u64) {
    env.events().publish(
        (Symbol::new(env, "charged"), user.clone()),
        (sub.merchant.clone(), sub.amount, charged_at),
    );
}

pub fn publish_pay_per_use(env: &Env, user: &Address, merchant: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "pay_per_use"), user.clone()),
        (merchant.clone(), amount),
    );
}

pub fn publish_cancelled(env: &Env, user: &Address) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), user.clone()), ());
}

pub fn publish_upgraded(env: &Env, new_wasm_hash: &BytesN<32>) {
    env.events()
        .publish((Symbol::new(env, "upgraded"),), new_wasm_hash.clone());
}

pub fn publish_contract_paused(env: &Env) {
    env.events()
        .publish((Symbol::new(env, "contract_paused"),), ());
}

pub fn publish_contract_unpaused(env: &Env) {
    env.events()
        .publish((Symbol::new(env, "contract_unpaused"),), ());
}

pub fn publish_daily_limit_set(env: &Env, user: &Address, limit: i128) {
    env.events()
        .publish((Symbol::new(env, "daily_limit_set"), user.clone()), limit);
}

pub fn publish_daily_limit_removed(env: &Env, user: &Address) {
    env.events()
        .publish((Symbol::new(env, "daily_limit_removed"), user.clone()), ());
}

pub fn publish_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_transferred"),),
        (old_admin.clone(), new_admin.clone()),
    );
}
