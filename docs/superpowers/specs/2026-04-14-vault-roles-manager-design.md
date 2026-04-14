# Vault Roles Manager - Contract Design Spec

**Date:** 2026-04-14
**Proposal:** APY_STABILIZER_PROPOSAL.md (Solution A)
**SDK:** soroban-sdk 25.3.1

---

## Overview

A non-upgradable Soroban proxy contract. Partners set it as their vault's Manager role. It stores per-vault config (admin, target APY, fee bounds) and a global fee_manager (DeFindex bot). The bot calls `lock_fees`/`distribute_fees` within each vault's configured bounds. Partners retain full control via admin passthrough functions.

## File Structure

```
apy-stabilizer/
  contracts/
    vault-roles-manager/
      Cargo.toml
      src/
        lib.rs        # Contract impl - all public functions (~300 LOC)
        storage.rs    # DataKey enum, VaultConfig struct, getters/setters, TTL
        error.rs      # ContractError enum
        events.rs     # Event structs
        test.rs       # Unit + integration tests
```

## Dependencies

```toml
[dependencies]
soroban-sdk = "25.3.1"

[dev-dependencies]
soroban-sdk = { version = "25.3.1", features = ["testutils"] }
```

No `soroban-fixed-point-math` - this contract does no math. All APY calculations happen off-chain in the bot.

## Storage

### Global (Instance Storage)

| Key | Type | Description |
|-----|------|-------------|
| `Admin` | `Address` | Contract deployer / DeFindex team. Can only change `FeeManager`. |
| `FeeManager` | `Address` | DeFindex bot address. Can call `lock_fees`/`distribute_fees` on any registered vault. |

### Per-Vault (Persistent Storage, keyed by vault Address)

```rust
#[contracttype]
#[derive(Clone)]
pub struct VaultConfig {
    pub admin: Address,        // Partner - full passthrough for their vault
    pub target_apy_bps: u32,  // Target APY in basis points (400 = 4.00%)
    pub max_fee_bps: u32,     // Max fee bot can set (0-10000)
    pub min_fee_bps: u32,     // Min fee (usually 0)
}
```

### TTL Management

- Instance TTL extended automatically on every write function (threshold/bump pattern)
- Persistent per-vault TTL extended on config read/write
- No public `extend_ttl()` function needed - bot calls regularly, keeping it alive

## Constructor

```rust
pub fn __constructor(env: Env, admin: Address, fee_manager: Address)
```

- `admin.require_auth()`
- Stores `Admin` and `FeeManager` in instance storage
- No vaults registered at deploy - partners register after via `register_vault()`
- **Not upgradable** - no `upgrade()` function on this contract

## Functions

### Registration

| Function | Auth | Description |
|----------|------|-------------|
| `register_vault(admin, vault, config)` | `admin` (must be vault's current Manager) | Stores config + calls `vault.set_manager(proxy)` atomically |
| `unregister_vault(vault)` | vault's `admin` | Calls `vault.set_manager(admin)` to return control, removes config |

### Fee Management

| Function | Auth | Description |
|----------|------|-------------|
| `lock_fees(caller, vault, new_fee_bps)` | `fee_manager` OR vault's `admin` | Validates fee within `[min, max]` bounds, calls `vault.lock_fees(fee_bps)` |
| `distribute_fees(caller, vault)` | `fee_manager` OR vault's `admin` | Calls `vault.distribute_fees(proxy_address)` |
| `release_fees(vault, strategy, amount)` | vault's `admin` only | Calls `vault.release_fees(strategy, amount)` |

### Config (vault's admin only)

| Function | Auth | Description |
|----------|------|-------------|
| `set_target_apy(vault, target_apy_bps)` | vault's `admin` | Updates target APY in config |
| `set_fee_bounds(vault, min_fee_bps, max_fee_bps)` | vault's `admin` | Validates: min <= max, max <= 10000 |

### Passthrough (vault's admin only)

All functions require vault's `admin` auth and forward to the vault with the proxy as the authorized caller.

| Function | Vault Function Called |
|----------|---------------------|
| `upgrade_vault(vault, new_wasm_hash)` | `vault.upgrade(hash)` |
| `set_vault_manager(vault, new_manager)` | `vault.set_manager(new_manager)` - removes config if new_manager != proxy |
| `set_vault_fee_receiver(vault, receiver)` | `vault.set_fee_receiver(proxy, receiver)` - proxy passes itself as caller (it holds Manager role) |
| `set_vault_emergency_manager(vault, em)` | `vault.set_emergency_manager(em)` |
| `set_vault_rebalance_manager(vault, rm)` | `vault.set_rebalance_manager(rm)` |
| `rescue_vault(vault, strategy)` | `vault.rescue(strategy, proxy_address)` |
| `pause_vault_strategy(vault, strategy)` | `vault.pause_strategy(strategy, proxy_address)` |
| `unpause_vault_strategy(vault, strategy)` | `vault.unpause_strategy(strategy, proxy_address)` |

### Global Admin

| Function | Auth | Description |
|----------|------|-------------|
| `set_fee_manager(new_fee_manager)` | contract `Admin` | Updates global fee_manager address |

### Read-Only

| Function | Returns |
|----------|---------|
| `get_vault_config(vault)` | `VaultConfig` for that vault |
| `get_fee_manager()` | Global fee_manager address |
| `get_admin()` | Contract admin address |

## Authorization Model

Three caller types with distinct permissions:

1. **Contract `Admin`** (DeFindex team): Can only call `set_fee_manager()`. Cannot touch any vault.
2. **`FeeManager`** (DeFindex bot): Can call `lock_fees()` and `distribute_fees()` on any registered vault, within that vault's configured bounds.
3. **Vault `admin`** (Partner): Full control over their own vault - config, passthrough, fee ops, unregister. Zero access to other vaults.

Cross-contract auth: The proxy is set as vault's Manager. When proxy calls vault functions, the vault's `require_auth()` on the proxy address auto-passes (direct invoker rule).

For `register_vault()`: The partner pre-authorizes the `vault.set_manager(proxy)` sub-invocation when signing the transaction. Soroban's RPC simulation auto-discovers this auth tree.

## Errors

```rust
#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    // Auth
    Unauthorized = 100,
    // Registration
    VaultAlreadyRegistered = 110,
    VaultNotRegistered = 111,
    // Validation
    FeeOutOfBounds = 120,
    InvalidFeeBounds = 121,      // min > max or max > 10000
    InvalidTargetApy = 122,
    // Vault interaction
    VaultCallFailed = 130,
}
```

## Events

| Event | Topics | Data |
|-------|--------|------|
| `VaultRegistered` | vault, admin | target_apy_bps |
| `VaultUnregistered` | vault, admin | - |
| `FeesLocked` | vault | fee_bps |
| `FeesDistributed` | vault | - |
| `ConfigUpdated` | vault | target_apy_bps, max_fee_bps, min_fee_bps |
| `FeeManagerUpdated` | - | old, new |

## Key Design Decisions

1. **Not upgradable**: Partners trust immutable code. New features = new contracts on separate roles.
2. **Flat architecture**: ~400-500 LOC total. Easy to audit. No abstraction layers.
3. **No on-chain math**: Bot does all APY calculations off-chain using indexer data. Contract just validates bounds.
4. **Per-vault isolation**: Each vault's config is independent. Partner A cannot affect Partner B.
5. **Atomic registration**: `register_vault()` stores config AND calls `vault.set_manager(proxy)` in one tx.
6. **Fee bounds as safety rails**: Bot can only set fees within each vault's `[min, max]` range.
7. **Contract Admin separate from vault admins**: Admin can only rotate fee_manager. No vault access.
