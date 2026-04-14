#![no_std]
use soroban_sdk::{contract, contractimpl, panic_with_error, vec, Address, BytesN, Env, IntoVal, Symbol};

mod error;
mod events;
mod storage;
mod test;

pub use error::ContractError;
pub use storage::VaultConfig;
use storage::extend_instance_ttl;

// --- Auth helpers (private) ---

fn require_fee_manager_or_vault_admin(env: &Env, caller: &Address, vault: &Address) {
    let fee_manager = storage::get_fee_manager(env);
    if *caller == fee_manager {
        caller.require_auth();
        return;
    }
    let config = storage::get_vault_config(env, vault)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::VaultNotRegistered));
    if *caller == config.admin {
        caller.require_auth();
        return;
    }
    panic_with_error!(env, ContractError::Unauthorized);
}

fn require_vault_admin(env: &Env, vault: &Address) -> VaultConfig {
    let config = storage::get_vault_config(env, vault)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::VaultNotRegistered));
    config.admin.require_auth();
    config
}

// --- Fee-bounds validation ---

fn validate_fee_bounds(env: &Env, min_fee_bps: u32, max_fee_bps: u32) {
    if min_fee_bps > max_fee_bps || max_fee_bps > 10_000 {
        panic_with_error!(env, ContractError::InvalidFeeBounds);
    }
}

// --- Cross-contract call helpers ---

fn call_vault(env: &Env, vault: &Address, fn_name: &str, args: soroban_sdk::Vec<soroban_sdk::Val>) {
    env.invoke_contract::<soroban_sdk::Val>(vault, &Symbol::new(env, fn_name), args);
}

#[contract]
pub struct VaultRolesManager;

#[contractimpl]
impl VaultRolesManager {
    pub fn __constructor(env: Env, admin: Address, fee_manager: Address) {
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_fee_manager(&env, &fee_manager);
    }

    // --- Read-only ---

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    pub fn get_fee_manager(env: Env) -> Address {
        storage::get_fee_manager(&env)
    }

    pub fn get_vault_config(env: Env, vault: Address) -> VaultConfig {
        storage::get_vault_config(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::VaultNotRegistered))
    }

    // --- Global admin ---

    pub fn set_fee_manager(env: Env, new_fee_manager: Address) {
        extend_instance_ttl(&env);
        let admin = storage::get_admin(&env);
        admin.require_auth();

        let old = storage::get_fee_manager(&env);
        storage::set_fee_manager(&env, &new_fee_manager);

        events::FeeManagerUpdated {
            old,
            new_addr: new_fee_manager,
        }
        .publish(&env);
    }

    // --- Registration ---

    pub fn register_vault(
        env: Env,
        admin: Address,
        vault: Address,
        config: VaultConfig,
    ) {
        extend_instance_ttl(&env);
        admin.require_auth();

        if storage::has_vault_config(&env, &vault) {
            panic_with_error!(&env, ContractError::VaultAlreadyRegistered);
        }

        validate_fee_bounds(&env, config.min_fee_bps, config.max_fee_bps);

        let proxy = env.current_contract_address();

        // Hand manager role to the proxy so the proxy can call vault functions.
        call_vault(
            &env,
            &vault,
            "set_manager",
            vec![&env, proxy.into_val(&env)],
        );

        storage::set_vault_config(&env, &vault, &config);

        events::VaultRegistered {
            vault,
            admin: config.admin,
            target_apy_bps: config.target_apy_bps,
        }
        .publish(&env);
    }

    pub fn unregister_vault(env: Env, vault: Address) {
        extend_instance_ttl(&env);
        let config = require_vault_admin(&env, &vault);

        // Return manager role to the vault admin.
        call_vault(
            &env,
            &vault,
            "set_manager",
            vec![&env, config.admin.clone().into_val(&env)],
        );

        storage::remove_vault_config(&env, &vault);

        events::VaultUnregistered {
            vault,
            admin: config.admin,
        }
        .publish(&env);
    }

    // --- Fee management ---

    pub fn lock_fees(
        env: Env,
        caller: Address,
        vault: Address,
        new_fee_bps: Option<u32>,
    ) {
        extend_instance_ttl(&env);
        require_fee_manager_or_vault_admin(&env, &caller, &vault);

        let config = storage::get_vault_config(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::VaultNotRegistered));

        if let Some(fee) = new_fee_bps {
            if fee < config.min_fee_bps || fee > config.max_fee_bps {
                panic_with_error!(&env, ContractError::FeeOutOfBounds);
            }
        }

        call_vault(
            &env,
            &vault,
            "lock_fees",
            vec![&env, new_fee_bps.into_val(&env)],
        );

        if let Some(fee_bps) = new_fee_bps {
            events::FeesLocked {
                vault,
                fee_bps,
            }
            .publish(&env);
        }
    }

    pub fn distribute_fees(env: Env, caller: Address, vault: Address) {
        extend_instance_ttl(&env);
        require_fee_manager_or_vault_admin(&env, &caller, &vault);

        let proxy = env.current_contract_address();
        call_vault(
            &env,
            &vault,
            "distribute_fees",
            vec![&env, proxy.into_val(&env)],
        );

        events::FeesDistributed { vault }.publish(&env);
    }

    pub fn release_fees(env: Env, vault: Address, strategy: Address, amount: i128) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        call_vault(
            &env,
            &vault,
            "release_fees",
            vec![&env, strategy.into_val(&env), amount.into_val(&env)],
        );
    }

    // --- Config updates ---

    pub fn set_target_apy(env: Env, vault: Address, target_apy_bps: u32) {
        extend_instance_ttl(&env);
        let mut config = require_vault_admin(&env, &vault);

        config.target_apy_bps = target_apy_bps;
        storage::set_vault_config(&env, &vault, &config);

        events::ConfigUpdated {
            vault,
            target_apy_bps: config.target_apy_bps,
            max_fee_bps: config.max_fee_bps,
            min_fee_bps: config.min_fee_bps,
        }
        .publish(&env);
    }

    pub fn set_fee_bounds(
        env: Env,
        vault: Address,
        min_fee_bps: u32,
        max_fee_bps: u32,
    ) {
        extend_instance_ttl(&env);
        let mut config = require_vault_admin(&env, &vault);

        validate_fee_bounds(&env, min_fee_bps, max_fee_bps);

        config.min_fee_bps = min_fee_bps;
        config.max_fee_bps = max_fee_bps;
        storage::set_vault_config(&env, &vault, &config);

        events::ConfigUpdated {
            vault,
            target_apy_bps: config.target_apy_bps,
            max_fee_bps: config.max_fee_bps,
            min_fee_bps: config.min_fee_bps,
        }
        .publish(&env);
    }

    // --- Passthrough functions ---

    pub fn upgrade_vault(env: Env, vault: Address, new_wasm_hash: BytesN<32>) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        call_vault(&env, &vault, "upgrade", vec![&env, new_wasm_hash.into_val(&env)]);
    }

    /// Sets the vault's manager. If the new manager is not this proxy,
    /// the vault config is removed since the proxy no longer controls the vault.
    pub fn set_vault_manager(env: Env, vault: Address, new_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        call_vault(&env, &vault, "set_manager", vec![&env, new_manager.clone().into_val(&env)]);
        if new_manager != env.current_contract_address() {
            storage::remove_vault_config(&env, &vault);
        }
    }

    pub fn set_vault_fee_receiver(env: Env, vault: Address, new_fee_receiver: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        // Vault's set_fee_receiver requires caller to be Manager or VaultFeeReceiver.
        // The proxy holds the Manager role, so we pass the proxy address.
        let proxy = env.current_contract_address();
        call_vault(&env, &vault, "set_fee_receiver", vec![&env, proxy.into_val(&env), new_fee_receiver.into_val(&env)]);
    }

    pub fn set_vault_emergency_manager(env: Env, vault: Address, emergency_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        call_vault(&env, &vault, "set_emergency_manager", vec![&env, emergency_manager.into_val(&env)]);
    }

    pub fn set_vault_rebalance_manager(env: Env, vault: Address, rebalance_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        call_vault(&env, &vault, "set_rebalance_manager", vec![&env, rebalance_manager.into_val(&env)]);
    }

    pub fn rescue_vault(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        let proxy = env.current_contract_address();
        call_vault(&env, &vault, "rescue", vec![&env, strategy.into_val(&env), proxy.into_val(&env)]);
    }

    pub fn pause_vault_strategy(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        let proxy = env.current_contract_address();
        call_vault(&env, &vault, "pause_strategy", vec![&env, strategy.into_val(&env), proxy.into_val(&env)]);
    }

    pub fn unpause_vault_strategy(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);
        let proxy = env.current_contract_address();
        call_vault(&env, &vault, "unpause_strategy", vec![&env, strategy.into_val(&env), proxy.into_val(&env)]);
    }
}
