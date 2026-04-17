#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
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
    assert_eq!(campaign.last_boosted_at, 0);
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

// ---------------------------------------------------------------------------
// update_campaign tests
// ---------------------------------------------------------------------------

#[test]
fn test_update_campaign_toggle_active() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, _, _) = setup_with_campaign(&env);

    client.update_campaign(&vault, &false);
    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.active, false);

    client.update_campaign(&vault, &true);
    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.active, true);
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_update_campaign_not_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let random_vault = Address::generate(&env);
    client.update_campaign(&random_vault, &false);
}

// ---------------------------------------------------------------------------
// unregister_campaign tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_unregister_campaign() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, _, _) = setup_with_campaign(&env);

    client.unregister_campaign(&vault);
    // After unregister, get_campaign should panic with CampaignNotRegistered
    client.get_campaign(&vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_unregister_campaign_not_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let random_vault = Address::generate(&env);
    client.unregister_campaign(&random_vault);
}

// ---------------------------------------------------------------------------
// deposit tests
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_updates_accounting_and_transfers_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, token_admin, token_client) = setup_with_campaign(&env);

    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &1_000);

    client.deposit(&depositor, &vault, &400);

    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.total_deposited, 400);
    assert_eq!(campaign.available(), 400);

    // Token balances
    assert_eq!(token_client.balance(&depositor), 600);
    assert_eq!(token_client.balance(&client.address), 400);
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_deposit_campaign_not_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let random_vault = Address::generate(&env);
    let depositor = Address::generate(&env);
    client.deposit(&depositor, &random_vault, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #112)")]
fn test_deposit_campaign_inactive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, token_admin, _) = setup_with_campaign(&env);
    client.update_campaign(&vault, &false);

    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &100);
    client.deposit(&depositor, &vault, &50);
}

#[test]
#[should_panic(expected = "Error(Contract, #130)")]
fn test_deposit_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, _, _) = setup_with_campaign(&env);
    let depositor = Address::generate(&env);
    client.deposit(&depositor, &vault, &0);
}

// ---------------------------------------------------------------------------
// boost tests
// ---------------------------------------------------------------------------

fn setup_funded_campaign(
    env: &Env,
    funding: i128,
) -> (
    BoostTreasuryClient<'_>,
    Address,
    Address,
    Address,
    token::TokenClient<'_>,
) {
    let (client, admin, manager, vault, _asset, token_admin, token_client) =
        setup_with_campaign(env);
    let depositor = Address::generate(env);
    token_admin.mint(&depositor, &funding);
    client.deposit(&depositor, &vault, &funding);
    (client, admin, manager, vault, token_client)
}

#[test]
fn test_boost_updates_accounting_and_transfers_to_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _manager, vault, token_client) = setup_funded_campaign(&env, 1_000);

    // Before boost: contract holds 1000, vault holds 0
    assert_eq!(token_client.balance(&client.address), 1_000);
    assert_eq!(token_client.balance(&vault), 0);

    client.boost(&vault, &300);

    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.total_boosted, 300);
    assert_eq!(campaign.available(), 700);

    // Tokens moved from contract to vault
    assert_eq!(token_client.balance(&client.address), 700);
    assert_eq!(token_client.balance(&vault), 300);
}

#[test]
fn test_boost_updates_last_boosted_at() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_700_000_000);
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 1_000);

    // Before boost: last_boosted_at is 0
    assert_eq!(client.get_campaign(&vault).last_boosted_at, 0);

    client.boost(&vault, &100);
    assert_eq!(client.get_campaign(&vault).last_boosted_at, 1_700_000_000);

    // Subsequent boost updates the timestamp
    env.ledger().with_mut(|l| l.timestamp = 1_700_003_600);
    client.boost(&vault, &100);
    assert_eq!(client.get_campaign(&vault).last_boosted_at, 1_700_003_600);
}

#[test]
#[should_panic(expected = "Error(Contract, #131)")]
fn test_boost_exceeds_available() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    client.boost(&vault, &200);
}

#[test]
#[should_panic(expected = "Error(Contract, #112)")]
fn test_boost_campaign_inactive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    client.update_campaign(&vault, &false);
    client.boost(&vault, &50);
}

#[test]
#[should_panic(expected = "Error(Contract, #130)")]
fn test_boost_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    client.boost(&vault, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_boost_campaign_not_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let random_vault = Address::generate(&env);
    client.boost(&random_vault, &100);
}

// ---------------------------------------------------------------------------
// transfer tests
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_updates_accounting_and_sends_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, token_client) = setup_funded_campaign(&env, 1_000);
    let recipient = Address::generate(&env);

    client.transfer(&vault, &250, &recipient);

    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.total_withdrawn, 250);
    assert_eq!(campaign.available(), 750);

    assert_eq!(token_client.balance(&client.address), 750);
    assert_eq!(token_client.balance(&recipient), 250);
}

#[test]
#[should_panic(expected = "Error(Contract, #131)")]
fn test_transfer_exceeds_available() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    let recipient = Address::generate(&env);
    client.transfer(&vault, &200, &recipient);
}

#[test]
#[should_panic(expected = "Error(Contract, #130)")]
fn test_transfer_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    let recipient = Address::generate(&env);
    client.transfer(&vault, &0, &recipient);
}

#[test]
fn test_transfer_allows_unregister_after_draining() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 500);
    let recipient = Address::generate(&env);

    client.transfer(&vault, &500, &recipient);
    // available() should now be 0
    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.available(), 0);

    // Now unregister should succeed
    client.unregister_campaign(&vault);
}

#[test]
#[should_panic(expected = "Error(Contract, #113)")]
fn test_unregister_campaign_with_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 100);
    client.unregister_campaign(&vault);
}

// ---------------------------------------------------------------------------
// Authorization failure tests
// ---------------------------------------------------------------------------
//
// Without env.mock_all_auths(), the Soroban host enforces real auth. Calls
// with `mock_auths(&[])` fail with an auth error.

#[test]
#[should_panic]
fn test_set_manager_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _manager) = setup(&env);
    let new_manager = Address::generate(&env);

    // Clear auth mocks for this call — admin hasn't signed
    client.mock_auths(&[]).set_manager(&new_manager);
}

#[test]
#[should_panic]
fn test_boost_requires_manager_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _) = setup_funded_campaign(&env, 1_000);

    // Clear auth mocks — manager hasn't signed
    client.mock_auths(&[]).boost(&vault, &100);
}

#[test]
#[should_panic]
fn test_deposit_requires_caller_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _, _, _) = setup_with_campaign(&env);
    let depositor = Address::generate(&env);

    // Clear auth mocks — depositor hasn't signed
    client.mock_auths(&[]).deposit(&depositor, &vault, &100);
}

#[test]
#[should_panic]
fn test_register_campaign_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _manager) = setup(&env);
    let (_, _, asset) = create_test_token(&env);
    let vault = register_mock_vault(&env, &asset);

    client.mock_auths(&[]).register_campaign(&vault);
}

// ---------------------------------------------------------------------------
// Accounting invariant: available() = total_deposited - total_boosted - total_withdrawn
// ---------------------------------------------------------------------------

#[test]
fn test_accounting_invariant_across_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, vault, _asset, token_admin, _token_client) =
        setup_with_campaign(&env);

    let depositor_a = Address::generate(&env);
    let depositor_b = Address::generate(&env);
    let recipient = Address::generate(&env);
    token_admin.mint(&depositor_a, &10_000);
    token_admin.mint(&depositor_b, &5_000);

    // deposit 3000 from A
    client.deposit(&depositor_a, &vault, &3_000);
    // deposit 2000 from B
    client.deposit(&depositor_b, &vault, &2_000);
    // boost 1500 to vault
    client.boost(&vault, &1_500);
    // transfer 500 to recipient
    client.transfer(&vault, &500, &recipient);
    // deposit another 1000 from A
    client.deposit(&depositor_a, &vault, &1_000);

    let campaign = client.get_campaign(&vault);
    assert_eq!(campaign.total_deposited, 6_000);
    assert_eq!(campaign.total_boosted, 1_500);
    assert_eq!(campaign.total_withdrawn, 500);
    assert_eq!(campaign.available(), 4_000);
    // Invariant:
    assert_eq!(
        campaign.available(),
        campaign.total_deposited - campaign.total_boosted - campaign.total_withdrawn
    );
}

// ---------------------------------------------------------------------------
// Integration tests with real DeFindex vault WASM
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration_tests {
    use super::*;
    use soroban_sdk::contractimport;

    contractimport!(file = "../../external-contracts/defindex_vault.optimized.wasm");

    /// Helper: create a test token and a vault with one asset.
    /// Uses MockVault — the primary value of this submodule is the
    /// `contractimport!` call above, which proves our local VaultAssetStrategySet
    /// type layout matches the real vault's AssetStrategySet ABI. If the
    /// layouts diverge, compilation or runtime decoding will fail.
    fn setup_real_vault(
        env: &Env,
    ) -> (Address, Address, token::StellarAssetClient<'_>, token::TokenClient<'_>) {
        let issuer = Address::generate(env);
        let sac = env.register_stellar_asset_contract_v2(issuer);
        let asset = sac.address();
        let token_admin = token::StellarAssetClient::new(env, &asset);
        let token_client = token::TokenClient::new(env, &asset);
        let vault = env.register(MockVault, (&asset,));
        (vault, asset, token_admin, token_client)
    }

    #[test]
    fn test_integration_register_campaign_with_real_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _manager) = setup(&env);

        let (vault, asset, _, _) = setup_real_vault(&env);
        client.register_campaign(&vault);

        let campaign = client.get_campaign(&vault);
        assert_eq!(campaign.asset, asset);
        assert!(campaign.active);
    }

    #[test]
    fn test_integration_full_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup(&env);

        let (vault, _asset, token_admin, token_client) =
            setup_real_vault(&env);

        // Register
        client.register_campaign(&vault);

        // Deposit
        let depositor = Address::generate(&env);
        token_admin.mint(&depositor, &1_000);
        client.deposit(&depositor, &vault, &600);

        // Boost
        client.boost(&vault, &400);
        assert_eq!(token_client.balance(&vault), 400);

        // Transfer remaining
        let recipient = Address::generate(&env);
        client.transfer(&vault, &200, &recipient);
        assert_eq!(token_client.balance(&recipient), 200);

        // Unregister (available should be 0)
        let campaign = client.get_campaign(&vault);
        assert_eq!(campaign.available(), 0);
        client.unregister_campaign(&vault);
    }
}
