#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env};

mod error;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use error::ContractError;
pub use storage::Campaign;

#[contract]
pub struct BoostTreasury;

#[contractimpl]
impl BoostTreasury {
    pub fn __constructor(env: Env, admin: Address, manager: Address) {
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_manager(&env, &manager);
    }

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    pub fn get_manager(env: Env) -> Address {
        storage::get_manager(&env)
    }
}
