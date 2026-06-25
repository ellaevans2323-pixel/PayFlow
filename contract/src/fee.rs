use soroban_sdk::{token, Address, Env};

use crate::{errors::ContractError, DataKey, Subscription};

/// Retrieves the fee collector address from instance storage.
pub fn get_fee_collector(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::FeeCollector)
}

/// Retrieves the fee in basis points (bps) from instance storage.
/// 1 bps = 0.01%
pub fn get_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0)
}

/// Returns fee settings when both collector and non-zero bps are configured.
pub fn get_fee(env: &Env) -> Option<(Address, u32)> {
    let collector = get_fee_collector(env)?;
    let bps = get_fee_bps(env);
    if bps == 0 {
        None
    } else {
        Some((collector, bps))
    }
}

/// Sets the fee collector and basis points.
pub fn set_fee(env: &Env, collector: Address, bps: u32) {
    if bps > 10_000 {
        env.panic_with_error(ContractError::InvalidFeeBps);
    }
    env.storage()
        .instance()
        .set(&DataKey::FeeCollector, &collector);
    env.storage().instance().set(&DataKey::FeeBps, &bps);
}

/// Computes the protocol fee for `amount` using configured bps (0 when unset).
pub fn calculate_fee_amount(amount: i128, bps: u32) -> i128 {
    if bps == 0 || amount <= 0 {
        return 0;
    }
    amount * (bps as i128) / 10_000
}

/// Transfers subscription charge amounts (fee to collector, net to merchant).
/// Returns the fee amount deducted from the gross subscription amount.
pub fn transfer_subscription_charge(env: &Env, user: &Address, sub: &Subscription) -> i128 {
    let bps = get_fee_bps(env);
    let fee_amount = match get_fee_collector(env) {
        Some(collector) if bps > 0 => {
            let fee = calculate_fee_amount(sub.amount, bps);
            if fee > 0 {
                let token_client = token::Client::new(env, &sub.token);
                token_client.transfer_from(&env.current_contract_address(), user, &collector, &fee);
            }
            fee
        }
        _ => 0,
    };
    let net = sub.amount - fee_amount;

    let token_client = token::Client::new(env, &sub.token);
    token_client.transfer_from(&env.current_contract_address(), user, &sub.merchant, &net);

    fee_amount
}
