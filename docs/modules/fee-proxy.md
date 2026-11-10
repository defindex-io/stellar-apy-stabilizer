# FeeProxy Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `contracts/fee-proxy/` · **Last verified:** 2026-11-10

## Purpose

A Soroban contract that takes over the **Manager role** of a DeFindex vault and re-exposes a narrow, auth-gated surface. An off-chain fee manager key can move the vault's performance fee inside partner-defined bounds; the partner keeps a per-vault admin lane for everything else. Blast radius: it is the only thing standing between a shared bot key and full Manager rights on live partner vaults, so any change to its auth helpers or bounds validation is a security change.

Deployed mainnet address is tracked in `mainnet.contracts.json:2`.

## Structure

| File | Purpose |
|---|---|
| `src/lib.rs` | Contract entrypoints, auth helpers, cross-contract call helper. |
| `src/storage.rs` | `DataKey` enum, `VaultConfig` struct, typed getters/setters, TTL bumps. |
| `src/error.rs` | `ContractError` codes in the 3000–3099 range. |
| `src/events.rs` | `#[contractevent]` structs. |
| `src/test.rs` | Unit tests against a `MockVault` plus an `integration_tests` module using the real vault WASM. |

## Public surface

Constructor: `__constructor(admin, fee_manager)` (`src/lib.rs:71`). `admin.require_auth()` is called, so the admin must sign the deploy.

**Read-only** (no auth, no instance TTL bump):

| Function | Line | Notes |
|---|---|---|
| `get_admin` | `src/lib.rs:79` | Panics if unset (`src/storage.rs:45` unwraps). |
| `get_fee_manager` | `src/lib.rs:83` | Same unwrap behavior (`src/storage.rs:72`). |
| `get_pending_admin` | `src/lib.rs:87` | Returns `Option<Address>`. |
| `get_vault_config` | `src/lib.rs:91` | Panics `VaultNotRegistered` when absent. Side effect: bumps the vault-config persistent TTL (`src/storage.rs:92`). |

**Mutating** — the "Auth" column names the address whose `require_auth()` must be satisfied:

| Function | Line | Auth | Notes |
|---|---|---|---|
| `set_fee_manager(new_fee_manager)` | `src/lib.rs:98` | global `admin` | One-shot rotation, no two-step. Emits `FeeManagerUpdated`. |
| `propose_admin(new_admin)` | `src/lib.rs:117` | global `admin` | Overwrites any pending proposal. |
| `accept_admin(new_admin)` | `src/lib.rs:133` | `new_admin` | Must equal the pending slot or panics `Unauthorized` (`src/lib.rs:139`). |
| `register_vault(vault, config)` | `src/lib.rs:164` | `config.admin` | Reads `vault.get_manager()` and requires it to equal `config.admin` and not be the proxy (`src/lib.rs:56`, called at `:177`), then calls `vault.set_manager(proxy)` (`src/lib.rs:187`), which also requires the vault's current Manager auth. |
| `unregister_vault(vault)` | `src/lib.rs:202` | `config.admin` | Hands Manager back to `config.admin`, then deletes the config. |
| `lock_fees(caller, vault, new_fee_bps)` | `src/lib.rs:225` | fee manager **or** `config.admin` | See Gotchas — `None` behaves differently from `Some`. |
| `distribute_fees(caller, vault)` | `src/lib.rs:256` | fee manager **or** `config.admin` | Passes the proxy address as the vault's caller. |
| `release_fees(vault, strategy, amount)` | `src/lib.rs:271` | `config.admin` | Requires `amount > 0` (`src/lib.rs:275`). Emits `FeesReleased`. |
| `set_target_apy(vault, target_apy_bps)` | `src/lib.rs:291` | `config.admin` | Full `u32` range accepted, no validation. |
| `set_fee_bounds(vault, min_fee_bps, max_fee_bps)` | `src/lib.rs:307` | `config.admin` | Re-runs `validate_fee_bounds`. |
| `upgrade_vault(vault, new_wasm_hash)` | `src/lib.rs:333` | `config.admin` | Passthrough to vault `upgrade`. No event. |
| `set_vault_manager(vault, new_manager)` | `src/lib.rs:341` | `config.admin` | Self-eviction path, see Gotchas. |
| `set_vault_fee_receiver(vault, new_fee_receiver)` | `src/lib.rs:356` | `config.admin` | Passes the proxy as the vault-side caller (`src/lib.rs:361`). No event. |
| `set_vault_emergency_manager(vault, emergency_manager)` | `src/lib.rs:365` | `config.admin` | No event. |
| `set_vault_rebalance_manager(vault, rebalance_manager)` | `src/lib.rs:371` | `config.admin` | No event. |
| `rescue_vault(vault, strategy)` | `src/lib.rs:377` | `config.admin` | No event. |
| `pause_vault_strategy(vault, strategy)` | `src/lib.rs:384` | `config.admin` | No event. |
| `unpause_vault_strategy(vault, strategy)` | `src/lib.rs:391` | `config.admin` | No event. |

## Storage layout

`DataKey` (`src/storage.rs:11`):

| Key | Storage | Value |
|---|---|---|
| `Admin` | instance | `Address` |
| `PendAdmin` | instance | `Address`, removed on `accept_admin` |
| `FeeMgr` | instance | `Address` |
| `VaultCfg(Address)` | persistent | `VaultConfig` |

`VaultConfig { admin, target_apy_bps, max_fee_bps, min_fee_bps }` (`src/storage.rs:20`).

TTL policy (`src/storage.rs:3`): instance bump 30 days with a 29-day threshold; persistent bump 120 days with a 100-day threshold. `extend_instance_ttl` is called by every mutating entrypoint; the persistent bump happens inside `get_vault_config` / `set_vault_config` (`src/storage.rs:92`, `src/storage.rs:102`).

## Errors

`ContractError` (`src/error.rs:6`), deliberately in the 3000–3099 band so codes never collide with the DeFindex vault (100–199), strategies (200–299) or BoostTreasury (4000–4099):

`Unauthorized = 3000`, `VaultAlreadyRegistered = 3010`, `VaultNotRegistered = 3011`, `FeeOutOfBounds = 3020`, `InvalidFeeBounds = 3021`, `NoPendingAdmin = 3023`, `InvalidAmount = 3030`, `VaultManagedByProxy = 3012`, `NotVaultManager = 3013` (the last two were appended at the end of the enum with explicit codes, so no existing code shifted).

## Events

`VaultRegistered` (`src/events.rs:5`), `VaultUnregistered` (`:15`), `FeesLocked` (`:24`), `FeesDistributed` (`:32`), `FeesReleased` (`:39`), `ConfigUpdated` (`:49`), `FeeManagerUpdated` (`:59`), `AdminProposed` (`:66`), `AdminUpdated` (`:73`).

## Dependencies

- `soroban-sdk` 25.3.1, pinned at the workspace root (`Cargo.toml:6`). Crate type is `cdylib` only (`contracts/fee-proxy/Cargo.toml:7`).
- **DeFindex vault contract** (external, not in this repo). Called by symbol name through `call_vault` (`src/lib.rs:44`), plus `get_manager` through `get_vault_manager` (`src/lib.rs:48`): `set_manager`, `lock_fees`, `distribute_fees`, `release_fees`, `upgrade`, `set_fee_receiver`, `set_emergency_manager`, `set_rebalance_manager`, `rescue`, `pause_strategy`, `unpause_strategy`.
- Consumed by `src/stabilizer/` (see [stabilizer.md](stabilizer.md)), which calls `get_vault_config` and `lock_fees`.
- Operated by `src/fee-proxy/bash/` (see [ops-scripts.md](ops-scripts.md)).

## Gotchas and invariants

- **`lock_fees(None)` skips validation and emits nothing.** The bounds check only runs inside `if let Some(fee)` (`src/lib.rs:234`) and the `FeesLocked` event only fires for `Some` (`src/lib.rs:247`). `None` is a re-lock of the existing fee, so it is invisible to any event-driven indexer.
- **Auth failure is a panic, not a silent no-op.** `require_fee_manager_or_vault_admin` (`src/lib.rs:15`) resolves the config first, compares the caller against the fee manager and the vault admin, and only then calls `caller.require_auth()`. A caller who matches neither panics `Unauthorized` before any auth check runs, so nothing is ever signed for a wrong caller.
- **`caller` on `lock_fees` / `distribute_fees` is not the signer of the outer tx by construction.** It is the address whose auth is required; the auth-by-parameter shape was reviewed and accepted (audit finding M4, `docs/internal-audit.md`).
- **`set_vault_manager` can evict the proxy.** If `new_manager != proxy` the vault config is deleted and `VaultUnregistered` is emitted (`src/lib.rs:345`). If `new_manager == proxy` the config survives and **no event fires**, so an indexer watching only events will not see the no-op.
- **Registration is not idempotent.** A second `register_vault` for the same vault panics `VaultAlreadyRegistered` (`src/lib.rs:173`).
- **`target_apy_bps` has no upper bound.** `validate_fee_bounds` (`src/lib.rs:36`) only constrains `min_fee_bps <= max_fee_bps <= 10_000`. Target APY accepts the full `u32` range on both `register_vault` and `set_target_apy`.
- **`register_vault` checks the vault's current Manager itself (F-05).** It authenticates `config.admin` (`src/lib.rs:170`), then `require_config_admin_is_vault_manager` (`src/lib.rs:56`) reads `vault.get_manager()` and panics `VaultManagedByProxy` (3012) if the Manager is already the proxy, or `NotVaultManager` (3013) if it is not `config.admin`. Do not drop this in favour of the vault's own `set_manager` auth: when the Manager is already the proxy, the proxy's nested call is the direct invoker and satisfies that auth itself, so without the check anyone could register such a vault with themselves as `config.admin`. A vault left in that state (Manager set to the proxy by hand, no config) can no longer be registered; recover it through the vault's own role holders, not the proxy. `src/fee-proxy/bash/register_vault.sh:225` already enforces `config.admin == signer` up front.
- **Read-only getters do not bump instance TTL.** Only mutating entrypoints call `extend_instance_ttl`. A proxy that is only ever read from would eventually let its instance entry expire.
- **`README.md` has drifted from this contract.** `README.md:90` still documents `register_vault(admin, vault, config)` (the `admin` param was removed under audit finding B01), `README.md:94` says `release_fees` emits no event (it emits `FeesReleased`), and the event list at `README.md:102` omits `FeesReleased`. Trust the source.

## Testing

- 44 `#[test]` cases in `src/test.rs` (measured), including unauthorized-caller and invalid-bounds paths.
- Unit tests run against `MockVault` (`src/test.rs:16`), a minimal stand-in implementing the Manager surface the proxy calls.
- `mod integration_tests` (`src/test.rs:610`) imports the real vault bytecode via `contractimport!(file = "../../external-contracts/defindex_vault.optimized.wasm")` (`src/test.rs:615`), so Manager handoffs execute against real vault code rather than the mock.
- F-05 regressions: `test_register_vault_rejects_vault_already_managed_by_proxy` (`src/test.rs:195`) and `test_register_vault_rejects_config_admin_not_current_manager` (`src/test.rs:214`) against `MockVault`, plus real-vault variants at `src/test.rs:695` and `:727`. The real-vault 3012 case signs only as the stranger via `mock_auths`, so it reproduces the takeover without blanket auth mocking. Mock vaults in the happy-path tests now start with the partner as Manager, matching real registrations.
- Run: `cargo test -p fee-proxy`, or `cargo test -p fee-proxy integration_tests` for the WASM-backed subset (`README.md:72`).
