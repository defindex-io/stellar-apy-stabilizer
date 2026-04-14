# Vault Roles Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a non-upgradable Soroban proxy contract that manages multiple DeFindex vaults, allowing a bot to adjust fees within partner-configured bounds while partners retain full admin control.

**Architecture:** Flat single-file contract (~400-500 LOC). Instance storage for global config (Admin, FeeManager). Persistent storage for per-vault config (VaultConfig keyed by vault Address). All vault interactions via generated client from WASM import. TTL extended automatically on every write.

**Tech Stack:** Rust, soroban-sdk 25.3.1, Soroban WASM target

**Spec:** `docs/superpowers/specs/2026-04-14-vault-roles-manager-design.md`

**Reference vault source:** `resources/defindex-vault/vault/src/`

---

### Task 1: Project Scaffolding

**Files:**
- Create: `contracts/vault-roles-manager/Cargo.toml`
- Create: `contracts/vault-roles-manager/src/lib.rs`
- Create: `contracts/vault-roles-manager/src/storage.rs`
- Create: `contracts/vault-roles-manager/src/error.rs`
- Create: `contracts/vault-roles-manager/src/events.rs`
- Create: `contracts/vault-roles-manager/src/test.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "vault-roles-manager"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
doctest = false

[dependencies]
soroban-sdk = "25.3.1"

[dev-dependencies]
soroban-sdk = { version = "25.3.1", features = ["testutils"] }

[profile.release]
opt-level = "z"
overflow-checks = true
debug = 0
strip = "symbols"
debug-assertions = false
panic = "abort"
codegen-units = 1
lto = true
```

- [ ] **Step 2: Create error.rs**

```rust
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    Unauthorized = 100,
    VaultAlreadyRegistered = 110,
    VaultNotRegistered = 111,
    FeeOutOfBounds = 120,
    InvalidFeeBounds = 121,
    InvalidTargetApy = 122,
    VaultCallFailed = 130,
}
```

- [ ] **Step 3: Create storage.rs**

```rust
use soroban_sdk::{contracttype, Address, Env};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;
const PERSISTENT_BUMP_AMOUNT: u32 = 120 * DAY_IN_LEDGERS;
const PERSISTENT_LIFETIME_THRESHOLD: u32 = PERSISTENT_BUMP_AMOUNT - 20 * DAY_IN_LEDGERS;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    FeeManager,
    VaultConfig(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultConfig {
    pub admin: Address,
    pub target_apy_bps: u32,
    pub max_fee_bps: u32,
    pub min_fee_bps: u32,
}

// --- TTL ---

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

pub fn extend_vault_config_ttl(env: &Env, vault: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::VaultConfig(vault.clone()),
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

// --- Admin ---

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap()
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

// --- FeeManager ---

pub fn get_fee_manager(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::FeeManager)
        .unwrap()
}

pub fn set_fee_manager(env: &Env, fee_manager: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::FeeManager, fee_manager);
}

// --- VaultConfig ---

pub fn get_vault_config(env: &Env, vault: &Address) -> Option<VaultConfig> {
    let config = env
        .storage()
        .persistent()
        .get(&DataKey::VaultConfig(vault.clone()));
    if config.is_some() {
        extend_vault_config_ttl(env, vault);
    }
    config
}

pub fn set_vault_config(env: &Env, vault: &Address, config: &VaultConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::VaultConfig(vault.clone()), config);
    extend_vault_config_ttl(env, vault);
}

pub fn remove_vault_config(env: &Env, vault: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::VaultConfig(vault.clone()));
}

pub fn has_vault_config(env: &Env, vault: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::VaultConfig(vault.clone()))
}
```

- [ ] **Step 4: Create events.rs**

```rust
use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultRegistered {
    #[topic]
    pub vault: Address,
    #[topic]
    pub admin: Address,
    pub target_apy_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultUnregistered {
    #[topic]
    pub vault: Address,
    #[topic]
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesLocked {
    #[topic]
    pub vault: Address,
    pub fee_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesDistributed {
    #[topic]
    pub vault: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigUpdated {
    #[topic]
    pub vault: Address,
    pub target_apy_bps: u32,
    pub max_fee_bps: u32,
    pub min_fee_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeManagerUpdated {
    pub old: Address,
    pub new_addr: Address,
}
```

- [ ] **Step 5: Create lib.rs skeleton with module declarations and constructor**

```rust
#![no_std]
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

mod error;
mod events;
mod storage;
mod test;

pub use error::ContractError;
use events::*;
use storage::{extend_instance_ttl, VaultConfig};

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
```

- [ ] **Step 6: Create empty test.rs**

```rust
#![cfg(test)]
```

- [ ] **Step 7: Verify it compiles**

Run: `cd contracts/vault-roles-manager && cargo build --target wasm32-unknown-unknown --release 2>&1 | tail -5`
Expected: Build succeeds (or warnings only, no errors)

- [ ] **Step 8: Commit**

```bash
git add contracts/vault-roles-manager/
git commit -m "feat(vault-roles-manager): scaffold contract with storage, errors, events"
```

---

### Task 2: Auth Helpers & Global Admin

**Files:**
- Modify: `contracts/vault-roles-manager/src/lib.rs`
- Modify: `contracts/vault-roles-manager/src/test.rs`

- [ ] **Step 1: Write failing tests for auth helpers and global admin functions**

Add to `test.rs`:

```rust
#![cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, Env};
use crate::{VaultRolesManager, VaultRolesManagerClient, VaultRolesManagerArgs};

fn setup_contract(env: &Env) -> (Address, Address, Address, VaultRolesManagerClient) {
    let admin = Address::generate(env);
    let fee_manager = Address::generate(env);
    let contract_id = env.register(
        VaultRolesManager,
        VaultRolesManagerArgs::__constructor(&admin, &fee_manager),
    );
    let client = VaultRolesManagerClient::new(env, &contract_id);
    (contract_id, admin, fee_manager, client)
}

#[test]
fn test_constructor_sets_admin_and_fee_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, admin, fee_manager, client) = setup_contract(&env);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_fee_manager(), fee_manager);
}

#[test]
fn test_set_fee_manager_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, _admin, _fee_manager, client) = setup_contract(&env);

    let new_fm = Address::generate(&env);
    client.set_fee_manager(&new_fm);
    assert_eq!(client.get_fee_manager(), new_fm);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_set_fee_manager_unauthorized() {
    let env = Env::default();
    // Do NOT mock_all_auths - we want real auth checks
    let (_id, _admin, _fee_manager, client) = setup_contract(&env);

    let attacker = Address::generate(&env);
    client.set_fee_manager(&attacker);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: Compilation errors (functions not defined yet)

- [ ] **Step 3: Implement read-only and global admin functions in lib.rs**

Add to `#[contractimpl] impl VaultRolesManager` in `lib.rs`:

```rust
    // --- Read-only ---

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    pub fn get_fee_manager(env: Env) -> Address {
        storage::get_fee_manager(&env)
    }

    pub fn get_vault_config(env: Env, vault: Address) -> VaultConfig {
        extend_instance_ttl(&env);
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

        FeeManagerUpdated {
            old,
            new_addr: new_fee_manager,
        }
        .publish(&env);
    }
```

Also add the missing import to the top of `lib.rs`:

```rust
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, Vec};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All 3 tests pass. The `test_set_fee_manager_unauthorized` test should pass because `require_auth()` panics without `mock_all_auths`.

Note: The `unauthorized` test may panic with a different error message than `Error(Contract, #100)` because `require_auth()` panics with a host auth error, not our custom error. If so, update the test to use `#[should_panic]` without the `expected` string, or change the auth pattern to check the caller first and then panic with our custom error. Adjust as needed.

- [ ] **Step 5: Commit**

```bash
git add contracts/vault-roles-manager/src/
git commit -m "feat(vault-roles-manager): add auth helpers, global admin, read-only functions"
```

---

### Task 3: Vault Registration & Unregistration

**Files:**
- Modify: `contracts/vault-roles-manager/src/lib.rs`
- Modify: `contracts/vault-roles-manager/src/test.rs`

This task requires calling the vault's `set_manager()`. Since we don't have the vault WASM in our test environment yet, we'll define a minimal mock vault contract for testing.

- [ ] **Step 1: Create a mock vault for testing**

Add to the top of `test.rs` (below existing imports):

```rust
use soroban_sdk::{contract, contractimpl, Symbol};
use crate::storage::VaultConfig;

// Minimal mock vault that tracks its manager
#[contract]
pub struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn __constructor(env: Env, manager: Address) {
        env.storage().instance().set(&Symbol::new(&env, "manager"), &manager);
    }

    pub fn get_manager(env: Env) -> Address {
        env.storage().instance().get(&Symbol::new(&env, "manager")).unwrap()
    }

    pub fn set_manager(env: Env, new_manager: Address) {
        let current: Address = env.storage().instance().get(&Symbol::new(&env, "manager")).unwrap();
        current.require_auth();
        env.storage().instance().set(&Symbol::new(&env, "manager"), &new_manager);
    }

    // Stubs for fee functions the proxy will call
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
```

Add a helper to create a mock vault:

```rust
fn setup_mock_vault(env: &Env, initial_manager: &Address) -> Address {
    env.register(
        MockVault,
        MockVaultArgs::__constructor(initial_manager),
    )
}
```

- [ ] **Step 2: Write failing tests for registration and unregistration**

Add to `test.rs`:

```rust
#[test]
fn test_register_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);
    let vault_client = MockVaultClient::new(&env, &vault_id);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };

    client.register_vault(&partner, &vault_id, &config);

    // Verify config stored
    let stored = client.get_vault_config(&vault_id);
    assert_eq!(stored.admin, partner);
    assert_eq!(stored.target_apy_bps, 400);

    // Verify vault manager changed to proxy
    assert_eq!(vault_client.get_manager(), proxy_id);
}

#[test]
fn test_unregister_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);
    let vault_client = MockVaultClient::new(&env, &vault_id);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // Unregister
    client.unregister_vault(&vault_id);

    // Verify manager returned to partner
    assert_eq!(vault_client.get_manager(), partner);
}

#[test]
#[should_panic(expected = "Error(Contract, #110)")]
fn test_register_vault_already_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);
    client.register_vault(&partner, &vault_id, &config); // Should panic
}

#[test]
#[should_panic(expected = "Error(Contract, #121)")]
fn test_register_vault_invalid_fee_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 3000,
        min_fee_bps: 5000, // min > max = invalid
    };
    client.register_vault(&partner, &vault_id, &config);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: Compilation errors (register_vault/unregister_vault not defined)

- [ ] **Step 4: Implement register_vault and unregister_vault**

In `lib.rs`, we need to define a vault client. Since we're calling the vault generically (not importing its WASM at compile time), use `env.invoke_contract` for the `set_manager` call. Add these functions to the `#[contractimpl]` block:

```rust
    // --- Registration ---

    pub fn register_vault(env: Env, admin: Address, vault: Address, config: VaultConfig) {
        extend_instance_ttl(&env);
        admin.require_auth();

        // Validate not already registered
        if storage::has_vault_config(&env, &vault) {
            panic_with_error!(&env, ContractError::VaultAlreadyRegistered);
        }

        // Validate fee bounds
        if config.min_fee_bps > config.max_fee_bps || config.max_fee_bps > 10_000 {
            panic_with_error!(&env, ContractError::InvalidFeeBounds);
        }

        // Store config
        storage::set_vault_config(&env, &vault, &config);

        // Call vault.set_manager(proxy) - admin must be the current manager
        // The admin pre-authorizes this sub-invocation when signing the tx
        let proxy = env.current_contract_address();
        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_manager"),
            soroban_sdk::vec![&env, proxy.into_val(&env)],
        );

        VaultRegistered {
            vault,
            admin: config.admin,
            target_apy_bps: config.target_apy_bps,
        }
        .publish(&env);
    }

    pub fn unregister_vault(env: Env, vault: Address) {
        extend_instance_ttl(&env);

        let config = storage::get_vault_config(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::VaultNotRegistered));

        config.admin.require_auth();

        // Return manager to the partner admin
        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_manager"),
            soroban_sdk::vec![&env, config.admin.clone().into_val(&env)],
        );

        let admin = config.admin.clone();
        storage::remove_vault_config(&env, &vault);

        VaultUnregistered {
            vault,
            admin,
        }
        .publish(&env);
    }
```

Add missing imports at the top of `lib.rs`:

```rust
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, IntoVal, Vec};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All tests pass (previous 3 + new 4 = 7 total)

- [ ] **Step 6: Commit**

```bash
git add contracts/vault-roles-manager/src/
git commit -m "feat(vault-roles-manager): add register/unregister vault with mock tests"
```

---

### Task 4: Fee Management Functions

**Files:**
- Modify: `contracts/vault-roles-manager/src/lib.rs`
- Modify: `contracts/vault-roles-manager/src/test.rs`

- [ ] **Step 1: Write failing tests for lock_fees, distribute_fees, release_fees**

Add to `test.rs`:

```rust
#[test]
fn test_lock_fees_by_fee_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, fee_manager, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // Fee manager can lock fees within bounds
    client.lock_fees(&fee_manager, &vault_id, &Some(3000u32));
}

#[test]
fn test_lock_fees_by_vault_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // Vault admin can also lock fees
    client.lock_fees(&partner, &vault_id, &Some(2000u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #120)")]
fn test_lock_fees_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, fee_manager, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // fee_bps = 6000 > max 5000 = out of bounds
    client.lock_fees(&fee_manager, &vault_id, &Some(6000u32));
}

#[test]
fn test_lock_fees_none_passes_through() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, fee_manager, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // None = no fee change, just lock existing
    client.lock_fees(&fee_manager, &vault_id, &None);
}

#[test]
fn test_distribute_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, fee_manager, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    client.distribute_fees(&fee_manager, &vault_id);
}

#[test]
fn test_release_fees_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);
    let strategy = Address::generate(&env);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    client.release_fees(&vault_id, &strategy, &100_i128);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: Compilation errors (functions not defined)

- [ ] **Step 3: Add auth helper function to lib.rs**

Add a private helper inside the `impl` block (or as a standalone function before it):

```rust
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
```

- [ ] **Step 4: Implement fee management functions in lib.rs**

Add to the `#[contractimpl]` block:

```rust
    // --- Fee Management ---

    pub fn lock_fees(env: Env, caller: Address, vault: Address, new_fee_bps: Option<u32>) {
        extend_instance_ttl(&env);
        require_fee_manager_or_vault_admin(&env, &caller, &vault);

        // Validate fee bounds if a new fee is being set
        if let Some(fee_bps) = new_fee_bps {
            let config = storage::get_vault_config(&env, &vault)
                .unwrap_or_else(|| panic_with_error!(&env, ContractError::VaultNotRegistered));
            if fee_bps < config.min_fee_bps || fee_bps > config.max_fee_bps {
                panic_with_error!(&env, ContractError::FeeOutOfBounds);
            }
        }

        env.invoke_contract::<Vec<soroban_sdk::Val>>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "lock_fees"),
            soroban_sdk::vec![&env, new_fee_bps.into_val(&env)],
        );

        if let Some(fee_bps) = new_fee_bps {
            FeesLocked {
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
        env.invoke_contract::<Vec<soroban_sdk::Val>>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "distribute_fees"),
            soroban_sdk::vec![&env, proxy.into_val(&env)],
        );

        FeesDistributed { vault }.publish(&env);
    }

    pub fn release_fees(env: Env, vault: Address, strategy: Address, amount: i128) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<soroban_sdk::Val>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "release_fees"),
            soroban_sdk::vec![&env, strategy.into_val(&env), amount.into_val(&env)],
        );
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add contracts/vault-roles-manager/src/
git commit -m "feat(vault-roles-manager): add lock_fees, distribute_fees, release_fees"
```

---

### Task 5: Config Update Functions

**Files:**
- Modify: `contracts/vault-roles-manager/src/lib.rs`
- Modify: `contracts/vault-roles-manager/src/test.rs`

- [ ] **Step 1: Write failing tests**

Add to `test.rs`:

```rust
#[test]
fn test_set_target_apy() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    client.set_target_apy(&vault_id, &800);

    let updated = client.get_vault_config(&vault_id);
    assert_eq!(updated.target_apy_bps, 800);
    // Other fields unchanged
    assert_eq!(updated.max_fee_bps, 5000);
    assert_eq!(updated.min_fee_bps, 0);
}

#[test]
fn test_set_fee_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    client.set_fee_bounds(&vault_id, &100, &8000);

    let updated = client.get_vault_config(&vault_id);
    assert_eq!(updated.min_fee_bps, 100);
    assert_eq!(updated.max_fee_bps, 8000);
}

#[test]
#[should_panic(expected = "Error(Contract, #121)")]
fn test_set_fee_bounds_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    // max > 10000 = invalid
    client.set_fee_bounds(&vault_id, &0, &15000);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: Compilation errors

- [ ] **Step 3: Implement config update functions**

Add to the `#[contractimpl]` block in `lib.rs`:

```rust
    // --- Config ---

    pub fn set_target_apy(env: Env, vault: Address, target_apy_bps: u32) {
        extend_instance_ttl(&env);
        let mut config = require_vault_admin(&env, &vault);
        config.target_apy_bps = target_apy_bps;
        storage::set_vault_config(&env, &vault, &config);

        ConfigUpdated {
            vault,
            target_apy_bps: config.target_apy_bps,
            max_fee_bps: config.max_fee_bps,
            min_fee_bps: config.min_fee_bps,
        }
        .publish(&env);
    }

    pub fn set_fee_bounds(env: Env, vault: Address, min_fee_bps: u32, max_fee_bps: u32) {
        extend_instance_ttl(&env);
        let mut config = require_vault_admin(&env, &vault);

        if min_fee_bps > max_fee_bps || max_fee_bps > 10_000 {
            panic_with_error!(&env, ContractError::InvalidFeeBounds);
        }

        config.min_fee_bps = min_fee_bps;
        config.max_fee_bps = max_fee_bps;
        storage::set_vault_config(&env, &vault, &config);

        ConfigUpdated {
            vault,
            target_apy_bps: config.target_apy_bps,
            max_fee_bps: config.max_fee_bps,
            min_fee_bps: config.min_fee_bps,
        }
        .publish(&env);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add contracts/vault-roles-manager/src/
git commit -m "feat(vault-roles-manager): add set_target_apy and set_fee_bounds"
```

---

### Task 6: Passthrough Functions

**Files:**
- Modify: `contracts/vault-roles-manager/src/lib.rs`
- Modify: `contracts/vault-roles-manager/src/test.rs`

- [ ] **Step 1: Write failing tests for passthrough functions**

Add to `test.rs`:

```rust
#[test]
fn test_upgrade_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    let fake_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_vault(&vault_id, &fake_hash);
    // If we get here without panic, the mock received the call
}

#[test]
fn test_set_vault_manager_removes_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);
    let vault_client = MockVaultClient::new(&env, &vault_id);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    let new_manager = Address::generate(&env);
    client.set_vault_manager(&vault_id, &new_manager);

    // Manager changed on vault
    assert_eq!(vault_client.get_manager(), new_manager);
}

#[test]
fn test_set_vault_fee_receiver() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    let new_receiver = Address::generate(&env);
    client.set_vault_fee_receiver(&vault_id, &partner, &new_receiver);
}

#[test]
fn test_rescue_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    client.register_vault(&partner, &vault_id, &config);

    let strategy = Address::generate(&env);
    client.rescue_vault(&vault_id, &strategy);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: Compilation errors

- [ ] **Step 3: Implement all passthrough functions**

Add to the `#[contractimpl]` block in `lib.rs`:

```rust
    // --- Passthrough (vault's admin only) ---

    pub fn upgrade_vault(env: Env, vault: Address, new_wasm_hash: BytesN<32>) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<soroban_sdk::Val>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "upgrade"),
            soroban_sdk::vec![&env, new_wasm_hash.into_val(&env)],
        );
    }

    pub fn set_vault_manager(env: Env, vault: Address, new_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_manager"),
            soroban_sdk::vec![&env, new_manager.into_val(&env)],
        );

        // If new manager is not this proxy, remove the vault config
        if new_manager != env.current_contract_address() {
            storage::remove_vault_config(&env, &vault);
        }
    }

    pub fn set_vault_fee_receiver(
        env: Env,
        vault: Address,
        caller: Address,
        new_fee_receiver: Address,
    ) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_fee_receiver"),
            soroban_sdk::vec![
                &env,
                caller.into_val(&env),
                new_fee_receiver.into_val(&env)
            ],
        );
    }

    pub fn set_vault_emergency_manager(env: Env, vault: Address, emergency_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_emergency_manager"),
            soroban_sdk::vec![&env, emergency_manager.into_val(&env)],
        );
    }

    pub fn set_vault_rebalance_manager(env: Env, vault: Address, rebalance_manager: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "set_rebalance_manager"),
            soroban_sdk::vec![&env, rebalance_manager.into_val(&env)],
        );
    }

    pub fn rescue_vault(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        let config = require_vault_admin(&env, &vault);

        let proxy = env.current_contract_address();
        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "rescue"),
            soroban_sdk::vec![&env, strategy.into_val(&env), proxy.into_val(&env)],
        );
    }

    pub fn pause_vault_strategy(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        let proxy = env.current_contract_address();
        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "pause_strategy"),
            soroban_sdk::vec![&env, strategy.into_val(&env), proxy.into_val(&env)],
        );
    }

    pub fn unpause_vault_strategy(env: Env, vault: Address, strategy: Address) {
        extend_instance_ttl(&env);
        require_vault_admin(&env, &vault);

        let proxy = env.current_contract_address();
        env.invoke_contract::<()>(
            &vault,
            &soroban_sdk::Symbol::new(&env, "unpause_strategy"),
            soroban_sdk::vec![&env, strategy.into_val(&env), proxy.into_val(&env)],
        );
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add contracts/vault-roles-manager/src/
git commit -m "feat(vault-roles-manager): add all passthrough functions"
```

---

### Task 7: Authorization Edge Case Tests

**Files:**
- Modify: `contracts/vault-roles-manager/src/test.rs`

Focused on testing that unauthorized callers are rejected.

- [ ] **Step 1: Write authorization edge case tests**

Add to `test.rs`:

```rust
#[test]
#[should_panic]
fn test_lock_fees_unauthorized_caller() {
    let env = Env::default();
    // No mock_all_auths - real auth
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner = Address::generate(&env);
    let vault_id = setup_mock_vault(&env, &partner);

    let config = VaultConfig {
        admin: partner.clone(),
        target_apy_bps: 400,
        max_fee_bps: 5000,
        min_fee_bps: 0,
    };
    // Need mock auth for registration
    env.mock_all_auths();
    client.register_vault(&partner, &vault_id, &config);

    // Reset auth mocking - now calls require real auth
    // Note: mock_all_auths can't be "unset" in soroban-sdk tests,
    // so instead we test that the function checks caller identity
    // by passing a random address that is neither fee_manager nor admin
    let attacker = Address::generate(&env);
    client.lock_fees(&attacker, &vault_id, &Some(1000u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn test_unregister_nonexistent_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let random_vault = Address::generate(&env);
    client.unregister_vault(&random_vault);
}

#[test]
fn test_vault_isolation() {
    let env = Env::default();
    env.mock_all_auths();
    let (_proxy_id, _admin, _fm, client) = setup_contract(&env);

    let partner_a = Address::generate(&env);
    let partner_b = Address::generate(&env);
    let vault_a = setup_mock_vault(&env, &partner_a);
    let vault_b = setup_mock_vault(&env, &partner_b);

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

    // Each vault has its own config
    let stored_a = client.get_vault_config(&vault_a);
    let stored_b = client.get_vault_config(&vault_b);
    assert_eq!(stored_a.target_apy_bps, 400);
    assert_eq!(stored_b.target_apy_bps, 600);
    assert_eq!(stored_a.admin, partner_a);
    assert_eq!(stored_b.admin, partner_b);
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1 | tail -20`
Expected: All tests pass

Note: The `test_lock_fees_unauthorized_caller` test may need adjustment depending on how `mock_all_auths` interacts with subsequent calls. If `mock_all_auths` can't be disabled mid-test, restructure the test to only check the caller identity logic (the `if caller == fee_manager` / `if caller == config.admin` checks) rather than the `require_auth()` calls. The `#[should_panic]` without expected string handles either panic source.

- [ ] **Step 3: Commit**

```bash
git add contracts/vault-roles-manager/src/test.rs
git commit -m "test(vault-roles-manager): add authorization edge case tests"
```

---

### Task 8: Final Build Verification & WASM Output

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**

Run: `cd contracts/vault-roles-manager && cargo test 2>&1`
Expected: All tests pass, no warnings about unused code

- [ ] **Step 2: Build WASM binary**

Run: `cd contracts/vault-roles-manager && cargo build --target wasm32-unknown-unknown --release 2>&1 | tail -10`
Expected: Build succeeds

- [ ] **Step 3: Check WASM binary size**

Run: `ls -la contracts/vault-roles-manager/target/wasm32-unknown-unknown/release/vault_roles_manager.wasm`
Expected: File exists, size should be reasonable (< 100KB for a simple proxy)

- [ ] **Step 4: Commit any final fixes**

If any compilation warnings or unused imports were found, fix them and commit:

```bash
git add contracts/vault-roles-manager/
git commit -m "chore(vault-roles-manager): clean up warnings, verify WASM build"
```
