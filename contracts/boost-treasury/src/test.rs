#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    token, vec, Address, Env, Vec,
};

use crate::{
    storage::{VaultAssetStrategySet, VaultStrategy},
    BoostTreasury, BoostTreasuryClient,
};

// ---------------------------------------------------------------------------
// Mock vault — returns a single-asset AssetStrategySet from get_assets()
// ---------------------------------------------------------------------------

#[contract]
pub struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn __constructor(env: Env, asset: Address) {
        env.storage()
            .instance()
            .set(&soroban_sdk::Symbol::new(&env, "asset"), &asset);
    }

    pub fn get_assets(env: Env) -> Vec<VaultAssetStrategySet> {
        let asset: Address = env
            .storage()
            .instance()
            .get(&soroban_sdk::Symbol::new(&env, "asset"))
            .unwrap();
        vec![
            &env,
            VaultAssetStrategySet {
                address: asset,
                strategies: Vec::<VaultStrategy>::new(&env),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// Multi-asset mock vault (for rejection test)
// ---------------------------------------------------------------------------

#[contract]
pub struct MultiAssetMockVault;

#[contractimpl]
impl MultiAssetMockVault {
    pub fn __constructor(env: Env, asset_a: Address, asset_b: Address) {
        env.storage()
            .instance()
            .set(&soroban_sdk::Symbol::new(&env, "asset_a"), &asset_a);
        env.storage()
            .instance()
            .set(&soroban_sdk::Symbol::new(&env, "asset_b"), &asset_b);
    }

    pub fn get_assets(env: Env) -> Vec<VaultAssetStrategySet> {
        let asset_a: Address = env
            .storage()
            .instance()
            .get(&soroban_sdk::Symbol::new(&env, "asset_a"))
            .unwrap();
        let asset_b: Address = env
            .storage()
            .instance()
            .get(&soroban_sdk::Symbol::new(&env, "asset_b"))
            .unwrap();
        vec![
            &env,
            VaultAssetStrategySet {
                address: asset_a,
                strategies: Vec::<VaultStrategy>::new(&env),
            },
            VaultAssetStrategySet {
                address: asset_b,
                strategies: Vec::<VaultStrategy>::new(&env),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (BoostTreasuryClient<'_>, Address, Address) {
    let admin = Address::generate(env);
    let manager = Address::generate(env);
    let contract_id = env.register(BoostTreasury, (&admin, &manager));
    let client = BoostTreasuryClient::new(env, &contract_id);
    (client, admin, manager)
}

/// Creates a test token (USDC-like), returns (token_admin_client, token_client, asset_address)
fn create_test_token(env: &Env) -> (token::StellarAssetClient<'_>, token::TokenClient<'_>, Address) {
    let issuer = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let asset = sac.address();
    let admin_client = token::StellarAssetClient::new(env, &asset);
    let token_client = token::TokenClient::new(env, &asset);
    (admin_client, token_client, asset)
}

/// Registers a MockVault with the given asset and returns its address
fn register_mock_vault(env: &Env, asset: &Address) -> Address {
    env.register(MockVault, (asset,))
}

// ---------------------------------------------------------------------------
// Constructor tests
// ---------------------------------------------------------------------------

#[test]
fn test_constructor_sets_admin_and_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, manager) = setup(&env);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_manager(), manager);
}

// ---------------------------------------------------------------------------
// set_manager tests
// ---------------------------------------------------------------------------

#[test]
fn test_set_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _manager) = setup(&env);
    let new_manager = Address::generate(&env);
    client.set_manager(&new_manager);
    assert_eq!(client.get_manager(), new_manager);
}

// ---------------------------------------------------------------------------
// register_campaign tests
// ---------------------------------------------------------------------------

fn setup_with_campaign(
    env: &Env,
) -> (
    BoostTreasuryClient<'_>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'_>,
    token::TokenClient<'_>,
) {
    let (client, admin, manager) = setup(env);
    let (token_admin, token_client, asset) = create_test_token(env);
    let vault = register_mock_vault(env, &asset);
    client.register_campaign(&vault);
    (client, admin, manager, vault, asset, token_admin, token_client)
}

#[test]
fn test_register_campaign() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _manager, vault, asset, _token_admin, _token_client) =
        setup_with_campaign(&env);

    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.active, true);
    assert_eq!(campaign.asset, asset);
    assert_eq!(campaign.total_deposited, 0);
    assert_eq!(campaign.total_boosted, 0);
    assert_eq!(campaign.total_withdrawn, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #110)")]
fn test_register_campaign_already_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, _, _) = setup_with_campaign(&env);
    client.register_campaign(&vault); // second call panics
}

#[test]
#[should_panic(expected = "Error(Contract, #120)")]
fn test_register_campaign_multi_asset_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _manager) = setup(&env);
    let asset_a = Address::generate(&env);
    let asset_b = Address::generate(&env);
    let multi_vault = env.register(MultiAssetMockVault, (&asset_a, &asset_b));
    client.register_campaign(&multi_vault);
}
