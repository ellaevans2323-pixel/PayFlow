use soroban_sdk::{BytesN, Env};

use crate::{admin, events};

pub fn upgrade(env: &Env, new_wasm_hash: BytesN<32>) {
    #[cfg(not(test))]
    env.deployer()
        .update_current_contract_wasm(new_wasm_hash.clone());

    events::publish_upgraded(env, &new_wasm_hash);
}
