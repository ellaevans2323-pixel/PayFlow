use soroban_sdk::{Address, Env};
use crate::storage::{get_admin, set_admin};
use crate::events;

pub fn require_admin(env: &Env) {
    let admin = get_admin(env);
    admin.require_auth();
}

pub fn transfer_admin(env: &Env, new_admin: &Address) {
    let old_admin = get_admin(env);
    old_admin.require_auth();
    
    set_admin(env, new_admin);
    events::publish_admin_transferred(env, &old_admin, new_admin);
}
