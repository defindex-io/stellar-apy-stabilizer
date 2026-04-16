#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, BytesN, Env, Symbol,
};

use crate::{FeeProxy, FeeProxyClient, VaultConfig};

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

    pub fn upgrade(_env: Env, _new_wasm_hash: BytesN<32>) {}
    pub fn set_fee_receiver(_env: Env, _caller: Address, _new_fee_receiver: Address) {}
    pub fn set_emergency_manager(_env: Env, _emergency_manager: Address) {}
    pub fn set_rebalance_manager(_env: Env, _new_rebalance_manager: Address) {}
    pub fn rescue(_env: Env, _strategy_address: Address, _caller: Address) {}
    pub fn pause_strategy(_env: Env, _strategy_address: Address, _caller: Address) {}
    pub fn unpause_strategy(_env: Env, _strategy_address: Address, _caller: Address) {}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (FeeProxyClient<'_>, Address, Address) {
    let admin = Address::generate(env);
    let fee_manager = Address::generate(env);
    let contract_id = env.register(
        FeeProxy,
        (&admin, &fee_manager),
    );
    let client = FeeProxyClient::new(env, &contract_id);
    (client, admin, fee_manager)
}

fn setup_with_vault(
    env: &Env,
) -> (FeeProxyClient<'_>, Address, Address, Address, Address) {
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

// ---------------------------------------------------------------------------
// Passthrough function tests (Task 6)
// ---------------------------------------------------------------------------

#[test]
fn test_upgrade_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let fake_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_vault(&vault_id, &fake_hash);
}

#[test]
fn test_set_vault_manager_removes_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let new_manager = Address::generate(&env);
    client.set_vault_manager(&vault_id, &new_manager);
    let mock_client = MockVaultClient::new(&env, &vault_id);
    assert_eq!(mock_client.get_manager(), new_manager);
}

#[test]
fn test_set_vault_fee_receiver() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let receiver = Address::generate(&env);
    client.set_vault_fee_receiver(&vault_id, &receiver);
}

#[test]
fn test_set_vault_emergency_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let em = Address::generate(&env);
    client.set_vault_emergency_manager(&vault_id, &em);
}

#[test]
fn test_set_vault_rebalance_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let rm = Address::generate(&env);
    client.set_vault_rebalance_manager(&vault_id, &rm);
}

#[test]
fn test_rescue_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let strategy = Address::generate(&env);
    client.rescue_vault(&vault_id, &strategy);
}

#[test]
fn test_pause_unpause_vault_strategy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    let strategy = Address::generate(&env);
    client.pause_vault_strategy(&vault_id, &strategy);
    client.unpause_vault_strategy(&vault_id, &strategy);
}

// ---------------------------------------------------------------------------
// Authorization edge case tests (Task 7)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_lock_fees_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, vault_id) = setup_with_vault(&env);
    // Random address is neither fee_manager nor vault admin
    let attacker = Address::generate(&env);
    client.lock_fees(&attacker, &vault_id, &Some(100u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_unregister_nonexistent_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let random_vault = Address::generate(&env);
    client.unregister_vault(&random_vault);
}

#[test]
fn test_vault_isolation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let partner_a = Address::generate(&env);
    let partner_b = Address::generate(&env);
    let vault_a = env.register(MockVault, (&client.address,));
    let vault_b = env.register(MockVault, (&client.address,));

    let config_a = VaultConfig {
        admin: partner_a.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    let config_b = VaultConfig {
        admin: partner_b.clone(),
        target_apy_bps: 600,
        max_fee_bps: 8000,
        min_fee_bps: 100,
    };
    client.register_vault(&partner_a, &vault_a, &config_a);
    client.register_vault(&partner_b, &vault_b, &config_b);

    let stored_a = client.get_vault_config(&vault_a);
    let stored_b = client.get_vault_config(&vault_b);
    assert_eq!(stored_a.target_apy_bps, 400);
    assert_eq!(stored_b.target_apy_bps, 600);
    assert_eq!(stored_a.admin, partner_a);
    assert_eq!(stored_b.admin, partner_b);
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_lock_fees_nonexistent_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, fee_manager) = setup(&env);
    let random_vault = Address::generate(&env);
    client.lock_fees(&fee_manager, &random_vault, &Some(100u32));
}

// ---------------------------------------------------------------------------
// Integration tests using real vault WASM
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration_tests {
    use super::*;
    use soroban_sdk::{Map, String};

    mod defindex_vault {
        soroban_sdk::contractimport!(file = "../../external-contracts/defindex_vault.optimized.wasm");
    }

    fn setup_real_vault(env: &Env, manager: &Address) -> Address {
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let token_address = token_contract.address();

        let assets = soroban_sdk::vec![
            env,
            defindex_vault::AssetStrategySet {
                address: token_address,
                strategies: soroban_sdk::vec![env],
            }
        ];

        let emergency_mgr = Address::generate(env);
        let fee_receiver = Address::generate(env);
        let rebalance_mgr = Address::generate(env);

        let mut roles: Map<u32, Address> = Map::new(env);
        roles.set(0u32, emergency_mgr);
        roles.set(1u32, fee_receiver);
        roles.set(2u32, manager.clone());
        roles.set(3u32, rebalance_mgr);

        let vault_fee: u32 = 100;
        let protocol_receiver = Address::generate(env);
        let protocol_rate: u32 = 2000;
        let router = Address::generate(env);

        let mut name_symbol: Map<String, String> = Map::new(env);
        name_symbol.set(
            String::from_str(env, "name"),
            String::from_str(env, "TestVault"),
        );
        name_symbol.set(
            String::from_str(env, "symbol"),
            String::from_str(env, "TV"),
        );

        env.register(
            defindex_vault::WASM,
            (assets, roles, vault_fee, protocol_receiver, protocol_rate, router, name_symbol, false),
        )
    }

    #[test]
    fn test_integration_register_vault() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _fee_manager) = setup(&env);

        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };

        client.register_vault(&partner, &vault_id, &config);

        let vault_client = defindex_vault::Client::new(&env, &vault_id);
        assert_eq!(vault_client.get_manager(), client.address);

        let stored = client.get_vault_config(&vault_id);
        assert_eq!(stored.admin, partner);
        assert_eq!(stored.target_apy_bps, 400);
    }

    #[test]
    fn test_integration_unregister_vault() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _fee_manager) = setup(&env);
        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };
        client.register_vault(&partner, &vault_id, &config);

        client.unregister_vault(&vault_id);

        let vault_client = defindex_vault::Client::new(&env, &vault_id);
        assert_eq!(vault_client.get_manager(), partner);
    }

    #[test]
    fn test_integration_lock_fees() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, fee_manager) = setup(&env);
        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };
        client.register_vault(&partner, &vault_id, &config);

        // Lock fees with a new fee rate; with no strategies it should succeed
        client.lock_fees(&fee_manager, &vault_id, &Some(2000u32));

        let vault_client = defindex_vault::Client::new(&env, &vault_id);
        let (vault_fee, _protocol_fee) = vault_client.get_fees();
        assert_eq!(vault_fee, 2000);
    }

    #[test]
    fn test_integration_distribute_fees() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, fee_manager) = setup(&env);
        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };
        client.register_vault(&partner, &vault_id, &config);

        // With no strategies and no locked fees, distribute should succeed
        client.distribute_fees(&fee_manager, &vault_id);
    }

    #[test]
    fn test_integration_set_roles_through_proxy() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _fee_manager) = setup(&env);
        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };
        client.register_vault(&partner, &vault_id, &config);

        let vault_client = defindex_vault::Client::new(&env, &vault_id);

        let new_em = Address::generate(&env);
        client.set_vault_emergency_manager(&vault_id, &new_em);
        assert_eq!(vault_client.get_emergency_manager(), new_em);

        let new_rm = Address::generate(&env);
        client.set_vault_rebalance_manager(&vault_id, &new_rm);
        assert_eq!(vault_client.get_rebalance_manager(), new_rm);

        let new_fr = Address::generate(&env);
        client.set_vault_fee_receiver(&vault_id, &new_fr);
        assert_eq!(vault_client.get_fee_receiver(), new_fr);
    }

    #[test]
    fn test_integration_set_vault_manager_returns_control() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _fee_manager) = setup(&env);
        let partner = Address::generate(&env);
        let vault_id = setup_real_vault(&env, &partner);

        let config = VaultConfig {
            admin: partner.clone(),
            target_apy_bps: 400,
            max_fee_bps: 5000,
            min_fee_bps: 0,
        };
        client.register_vault(&partner, &vault_id, &config);

        let new_manager = Address::generate(&env);
        client.set_vault_manager(&vault_id, &new_manager);

        let vault_client = defindex_vault::Client::new(&env, &vault_id);
        assert_eq!(vault_client.get_manager(), new_manager);
    }
}
