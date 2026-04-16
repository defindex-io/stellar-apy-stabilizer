#![no_std]

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, vec, Address, Env, Symbol, Vec,
};

mod error;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use error::ContractError;
pub use storage::Campaign;
use storage::{extend_instance_ttl, VaultAssetStrategySet};

// --- Auth helpers (private) ---

fn require_admin(env: &Env) {
    let admin = storage::get_admin(env);
    admin.require_auth();
}

#[allow(dead_code)]
fn require_manager(env: &Env) {
    let manager = storage::get_manager(env);
    manager.require_auth();
}

fn require_active_campaign(env: &Env, vault: &Address) -> Campaign {
    let campaign = storage::get_campaign(env, vault)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::CampaignNotRegistered));
    if !campaign.active {
        panic_with_error!(env, ContractError::CampaignInactive);
    }
    campaign
}

fn require_positive_amount(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, ContractError::InvalidAmount);
    }
}

#[contract]
pub struct BoostTreasury;

#[contractimpl]
impl BoostTreasury {
    pub fn __constructor(env: Env, admin: Address, manager: Address) {
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_manager(&env, &manager);
    }

    // --- Read-only ---

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    pub fn get_manager(env: Env) -> Address {
        storage::get_manager(&env)
    }

    pub fn get_campaign(env: Env, vault: Address) -> Campaign {
        storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered))
    }

    // --- Admin-only ---

    pub fn set_manager(env: Env, new_manager: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let old = storage::get_manager(&env);
        storage::set_manager(&env, &new_manager);

        events::ManagerUpdated {
            old,
            new_addr: new_manager,
        }
        .publish(&env);
    }

    pub fn register_campaign(env: Env, vault: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        if storage::has_campaign(&env, &vault) {
            panic_with_error!(&env, ContractError::CampaignAlreadyRegistered);
        }

        // Call vault.get_assets() to discover the single asset
        let assets: Vec<VaultAssetStrategySet> = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_assets"),
            vec![&env],
        );
        if assets.len() != 1 {
            panic_with_error!(&env, ContractError::MultiAssetVaultNotSupported);
        }
        let asset = assets.get_unchecked(0).address;

        let campaign = Campaign {
            active: true,
            asset: asset.clone(),
            total_deposited: 0,
            total_boosted: 0,
            total_withdrawn: 0,
        };
        storage::set_campaign(&env, &vault, &campaign);

        events::CampaignRegistered {
            vault,
            asset,
        }
        .publish(&env);
    }

    pub fn update_campaign(env: Env, vault: Address, active: bool) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let mut campaign = storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered));

        campaign.active = active;
        storage::set_campaign(&env, &vault, &campaign);

        events::CampaignUpdated { vault, active }.publish(&env);
    }

    pub fn unregister_campaign(env: Env, vault: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let campaign = storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered));

        if campaign.available() != 0 {
            panic_with_error!(&env, ContractError::CampaignHasBalance);
        }

        storage::remove_campaign(&env, &vault);

        events::CampaignUnregistered { vault }.publish(&env);
    }

    // --- Anyone (authenticated) ---

    pub fn deposit(env: Env, caller: Address, vault: Address, amount: i128) {
        extend_instance_ttl(&env);
        caller.require_auth();
        require_positive_amount(&env, amount);

        let mut campaign = require_active_campaign(&env, &vault);

        token::Client::new(&env, &campaign.asset).transfer(
            &caller,
            &env.current_contract_address(),
            &amount,
        );

        campaign.total_deposited += amount;
        storage::set_campaign(&env, &vault, &campaign);

        events::Deposited {
            vault,
            depositor: caller,
            amount,
        }
        .publish(&env);
    }
}
