#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Env, Symbol,
};

use crate::{VaultRolesManager, VaultRolesManagerClient, VaultConfig};

// ---------------------------------------------------------------------------
// Mock vault
// ---------------------------------------------------------------------------

#[contract]
pub struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn __constructor(env: Env, manager: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "manager"), &manager);
    }

    pub fn get_manager(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "manager"))
            .unwrap()
    }

    pub fn set_manager(env: Env, new_manager: Address) {
        let current: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "manager"))
            .unwrap();
        current.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "manager"), &new_manager);
    }

    pub fn lock_fees(_env: Env, _new_fee_bps: Option<u32>) {}
    pub fn distribute_fees(_env: Env, _caller: Address) {}
    pub fn release_fees(_env: Env, _strategy: Address, _amount: i128) {}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (VaultRolesManagerClient<'_>, Address, Address) {
    let admin = Address::generate(env);
    let fee_manager = Address::generate(env);
    let contract_id = env.register(
        VaultRolesManager,
        (&admin, &fee_manager),
    );
    let client = VaultRolesManagerClient::new(env, &contract_id);
    (client, admin, fee_manager)
}

fn setup_with_vault(
    env: &Env,
) -> (VaultRolesManagerClient<'_>, Address, Address, Address, Address) {
    let (client, admin, fee_manager) = setup(env);

    let vault_admin = Address::generate(env);
    let vault_id = env.register(MockVault, (&client.address,));
    let config = VaultConfig {
        admin: vault_admin.clone(),
        target_apy_bps: 500,
        min_fee_bps: 10,
        max_fee_bps: 200,
    };
    client.register_vault(&vault_admin, &vault_id, &config);
    (client, admin, fee_manager, vault_admin, vault_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_constructor_sets_admin_and_fee_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, fee_manager) = setup(&env);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_fee_manager(), fee_manager);
}

#[test]
fn test_set_fee_manager_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager) = setup(&env);
    let new_fm = Address::generate(&env);
    client.set_fee_manager(&new_fm);
    assert_eq!(client.get_fee_manager(), new_fm);
}

#[test]
fn test_register_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager) = setup(&env);

    let vault_admin = Address::generate(&env);
    // Register mock vault with proxy as initial manager so set_manager auth passes.
    let vault_id = env.register(MockVault, (&client.address,));
    let config = VaultConfig {
        admin: vault_admin.clone(),
        target_apy_bps: 500,
        min_fee_bps: 10,
        max_fee_bps: 200,
    };
    client.register_vault(&vault_admin, &vault_id, &config);

    let stored = client.get_vault_config(&vault_id);
    assert_eq!(stored.admin, vault_admin);
    assert_eq!(stored.target_apy_bps, 500);

    // The vault's manager should now be the proxy (MockVault tracks it).
    let mock_client = MockVaultClient::new(&env, &vault_id);
    assert_eq!(mock_client.get_manager(), client.address);
}

#[test]
fn test_unregister_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, vault_admin, vault_id) =
        setup_with_vault(&env);

    client.unregister_vault(&vault_id);

    // Manager should be returned to vault_admin.
    let mock_client = MockVaultClient::new(&env, &vault_id);
    assert_eq!(mock_client.get_manager(), vault_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #110)")]
fn test_register_vault_already_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, vault_admin, vault_id) =
        setup_with_vault(&env);

    let config = VaultConfig {
        admin: vault_admin.clone(),
        target_apy_bps: 500,
        min_fee_bps: 10,
        max_fee_bps: 200,
    };
    // Second registration should panic.
    client.register_vault(&vault_admin, &vault_id, &config);
}

#[test]
#[should_panic(expected = "Error(Contract, #121)")]
fn test_register_vault_invalid_fee_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager) = setup(&env);

    let vault_admin = Address::generate(&env);
    let vault_id = env.register(MockVault, (&client.address,));
    let bad_config = VaultConfig {
        admin: vault_admin.clone(),
        target_apy_bps: 500,
        min_fee_bps: 300,  // min > max → invalid
        max_fee_bps: 100,
    };
    client.register_vault(&vault_admin, &vault_id, &bad_config);
}

#[test]
fn test_lock_fees_by_fee_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    // fee within [10, 200]
    client.lock_fees(&fee_manager, &vault_id, &Some(100u32));
}

#[test]
fn test_lock_fees_by_vault_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, vault_admin, vault_id) =
        setup_with_vault(&env);
    client.lock_fees(&vault_admin, &vault_id, &Some(50u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #120)")]
fn test_lock_fees_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    // 201 > max_fee_bps (200)
    client.lock_fees(&fee_manager, &vault_id, &Some(201u32));
}

#[test]
fn test_lock_fees_none_passes_through() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    // None should skip bounds check and not panic.
    client.lock_fees(&fee_manager, &vault_id, &None);
}

#[test]
fn test_distribute_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    client.distribute_fees(&fee_manager, &vault_id);
}

#[test]
fn test_release_fees_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    let strategy = Address::generate(&env);
    client.release_fees(&vault_id, &strategy, &1000i128);
}

#[test]
fn test_set_target_apy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);

    client.set_target_apy(&vault_id, &800u32);

    let config = client.get_vault_config(&vault_id);
    assert_eq!(config.target_apy_bps, 800);
    // Other fields unchanged.
    assert_eq!(config.min_fee_bps, 10);
    assert_eq!(config.max_fee_bps, 200);
}

#[test]
fn test_set_fee_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);

    client.set_fee_bounds(&vault_id, &5u32, &150u32);

    let config = client.get_vault_config(&vault_id);
    assert_eq!(config.min_fee_bps, 5);
    assert_eq!(config.max_fee_bps, 150);
    assert_eq!(config.target_apy_bps, 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #121)")]
fn test_set_fee_bounds_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _fee_manager, _vault_admin, vault_id) =
        setup_with_vault(&env);
    // max > 10_000 → invalid
    client.set_fee_bounds(&vault_id, &0u32, &10_001u32);
}
