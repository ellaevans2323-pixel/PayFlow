use soroban_sdk::{Address, Env};

use crate::{errors::ContractError, events, DataKey, SUBSCRIPTION_TTL_LEDGERS};

/// Returns the referrer for a given subscriber, if one was recorded.
pub fn get_referrer(env: &Env, user: &Address) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Referral(user.clone()))
}

/// Stores the referrer for a subscriber. Clears any prior referrer when `None`.
pub fn store_referral(env: &Env, user: &Address, referrer: &Option<Address>) {
    let key = DataKey::Referral(user.clone());
    if let Some(ref r) = referrer {
        if r == user {
            env.panic_with_error(ContractError::SelfReferral);
        }
        env.storage().persistent().set(&key, r);
        env.storage().persistent().extend_ttl(
            &key,
            SUBSCRIPTION_TTL_LEDGERS,
            SUBSCRIPTION_TTL_LEDGERS,
        );

        events::publish_referred(env, user, r);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// Removes the referral entry for a subscriber, if one exists.
/// Safe to call even when no referral was ever recorded.
pub fn remove_referral(env: &Env, user: &Address) {
    let key = DataKey::Referral(user.clone());
    env.storage().persistent().remove(&key);
}
