#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env};

mod error;
mod events;
mod storage;
mod test;

pub use error::ContractError;
use storage::extend_instance_ttl;

#[contract]
pub struct VaultRolesManager;

#[contractimpl]
impl VaultRolesManager {
    pub fn __constructor(env: Env, admin: Address, fee_manager: Address) {
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_fee_manager(&env, &fee_manager);
    }
}
