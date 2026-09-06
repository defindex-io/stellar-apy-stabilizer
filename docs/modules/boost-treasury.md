# BoostTreasury Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `contracts/boost-treasury/` · **Last verified:** 2026-09-04

## Purpose

A Soroban contract that escrows partner-funded top-up budgets, one campaign per DeFindex vault, and lets a designated manager key stream them into the vault as realized yield. It custodies real tokens, so every change to the accounting fields or to `available()` is a fund-safety change.

Deployed mainnet address is tracked in `mainnet.contracts.json:3`.

## Structure

| File | Purpose |
|---|---|
| `src/lib.rs` | Contract entrypoints and auth/validation helpers. |
| `src/storage.rs` | `DataKey`, `Campaign` (+ `available()`), vault-type mirrors, TTL bumps, tracked-balance accessors. |
| `src/error.rs` | `ContractError` codes in the 4000–4099 range. |
| `src/events.rs` | `#[contractevent]` structs. |
| `src/test.rs` | Unit tests with mock vaults plus an `integration_tests` module using the real vault WASM. |

## Public surface

Constructor: `__constructor(admin, manager)` (`src/lib.rs:49`). `admin.require_auth()` is called, so the admin must sign the deploy.

**Read-only:**

| Function | Line | Notes |
|---|---|---|
| `get_admin` | `src/lib.rs:57` | Unwraps; panics if unset. |
| `get_manager` | `src/lib.rs:61` | Unwraps; panics if unset. |
| `get_pending_admin` | `src/lib.rs:65` | `Option<Address>`. |
| `get_campaign` | `src/lib.rs:69` | Panics `CampaignNotRegistered` when absent. Side effect: bumps the campaign persistent TTL (`src/storage.rs:122`). |

**Mutating:**

| Function | Line | Auth | Notes |
|---|---|---|---|
| `set_manager(new_manager)` | `src/lib.rs:76` | `admin` | One-shot rotation, no two-step. |
| `propose_admin(new_admin)` | `src/lib.rs:93` | `admin` | Overwrites any pending proposal. |
| `accept_admin(new_admin)` | `src/lib.rs:109` | `new_admin` | Must equal the pending slot (`src/lib.rs:115`). |
| `register_campaign(vault, asset)` | `src/lib.rs:135` | `admin` | Reads the vault's `get_assets()` (`src/lib.rs:144`); rejects multi-asset vaults and any asset mismatch. Starts `active = true`. |
| `update_campaign(vault, active)` | `src/lib.rs:175` | `admin` | Toggles the active flag only. |
| `unregister_campaign(vault)` | `src/lib.rs:188` | `admin` | Requires `available() == 0` (`src/lib.rs:195`). |
| `deposit(caller, vault, amount)` | `src/lib.rs:208` | `caller` (anyone) | Requires `amount > 0` and an active campaign. Pulls tokens via `token::transfer` (`src/lib.rs:215`). |
| `boost(vault, amount)` | `src/lib.rs:243` | `manager` | Requires active campaign and `amount <= available()`. Stamps `last_boosted_at` from the ledger (`src/lib.rs:264`). |
| `transfer(vault, amount, to)` | `src/lib.rs:288` | `admin` | Draws down a campaign's `available()` to any address. Does **not** require `active`. |
| `reallocate(from_vault, to_vault, amount)` | `src/lib.rs:331` | `admin` | Moves tracked budget between same-asset campaigns; no token leaves the treasury. |
| `rescue_orphan(token, to, amount)` | `src/lib.rs:383` | `admin` | Sweeps only the unattributed balance; see Gotchas. |

## Storage layout

`DataKey` (`src/storage.rs:11`):

| Key | Storage | Value |
|---|---|---|
| `Admin` | instance | `Address` |
| `PendAdmin` | instance | `Address`, removed on `accept_admin` |
| `Manager` | instance | `Address` |
| `Campaign(Address)` | persistent | `Campaign`, keyed by vault |
| `Tracked(Address)` | persistent | `i128`, keyed by token |

`Campaign { active, asset, total_deposited, total_boosted, total_withdrawn, last_boosted_at }` (`src/storage.rs:26`). `available() = total_deposited - total_boosted - total_withdrawn` via a `checked_sub` chain that returns 0 on violation (`src/storage.rs:41`).

`VaultStrategy` / `VaultAssetStrategySet` (`src/storage.rs:52`, `:60`) are local mirrors of the DeFindex vault's return types, kept minimal so `get_assets()` can be decoded without a vault dependency.

TTL policy matches FeeProxy: instance 30 days, persistent 120 days (`src/storage.rs:3`). Campaign and Tracked entries are bumped on every read and write (`src/storage.rs:122`, `:132`, `:163`, `:172`).

## Errors

`ContractError` (`src/error.rs:6`), in the 4000–4099 band to avoid collisions with the vault and strategy contracts:

`Unauthorized = 4000`, `NoPendingAdmin = 4001`, `CampaignAlreadyRegistered = 4010`, `CampaignNotRegistered = 4011`, `CampaignInactive = 4012`, `CampaignHasBalance = 4013`, `MultiAssetVaultNotSupported = 4020`, `AssetMismatch = 4021`, `SameVault = 4022`, `InvalidAmount = 4030`, `InsufficientBudget = 4031`, `InsufficientOrphanBalance = 4032`, `Overflow = 4040`, `AccountingCorrupted = 4041`.

## Events

`CampaignRegistered` (`src/events.rs:5`), `CampaignUpdated` (`:13`), `CampaignUnregistered` (`:21`), `Deposited` (`:28`), `Boosted` (`:38`), `Transferred` (`:46`), `Reallocated` (`:56`), `ManagerUpdated` (`:66`), `AdminProposed` (`:73`), `AdminUpdated` (`:80`), `OrphanRescued` (`:87`).

## Dependencies

- `soroban-sdk` 25.3.1 from the workspace (`Cargo.toml:6`); `cdylib` only (`contracts/boost-treasury/Cargo.toml:7`).
- **SEP-41 token contract** for the campaign asset, via `token::Client` (`src/lib.rs:215`, `:254`, `:300`, `:403`).
- **DeFindex vault contract** for `get_assets()` at registration time only (`src/lib.rs:144`). After that the asset is frozen in the campaign.
- Operated by `src/boost-treasury/bash/` (see [ops-scripts.md](ops-scripts.md)). No TypeScript module in this repo calls it.

## Gotchas and invariants

- **`Tracked(token)` is the load-bearing accounting invariant.** It is the running sum of every campaign's `available()` for that token. It is incremented on `deposit` (`src/lib.rs:228`), decremented on `boost` (`src/lib.rs:268`) and `transfer` (`src/lib.rs:314`), and deliberately left alone by `register_campaign` (`src/lib.rs:166`), `unregister_campaign` (`src/lib.rs:200`) and `reallocate` (`src/lib.rs:363`), each of which is a no-op for the per-token total. Any new entrypoint that changes a campaign's `available()` **must** update `Tracked` in the same call or `rescue_orphan` will mis-compute the orphan balance.
- **`rescue_orphan` is the safe sweep; `transfer` is not.** `rescue_orphan` bounds itself by `balance(token) - Tracked(token)` (`src/lib.rs:395`) so an admin typo cannot touch campaign-attributed funds. `transfer` can drain any campaign's whole `available()` to any address, by design (`src/lib.rs:276` docstring). Use `rescue_orphan` for dust and mistaken sends, `transfer` only for deliberate refunds.
- **A negative orphan is fatal on purpose.** If `balance < tracked` the `checked_sub` panics `AccountingCorrupted` (`src/lib.rs:397`) rather than sweeping. Same for the decrement paths (`src/lib.rs:270`, `:316`). These fail closed; do not "fix" them with `unwrap_or(0)`.
- **`transfer` works on inactive campaigns, `deposit` and `boost` do not.** `require_active_campaign` (`src/lib.rs:29`) gates deposit and boost; `transfer` fetches the campaign directly (`src/lib.rs:293`). This is the intended escape hatch out of a paused campaign.
- **The asset is captured once and never re-validated.** `register_campaign` asserts the vault self-reports the same asset, then stores it (`src/lib.rs:152`). If the vault later changes assets, the campaign keeps transferring the original token.
- **`reallocate` requires both campaigns registered and same-asset** (`src/lib.rs:345`), and rejects `from == to` with `SameVault` (`src/lib.rs:336`). It moves budget by bumping the source's `total_withdrawn` and the destination's `total_deposited`, so the source's history looks like a withdrawal.
- **`available()` fails closed at 0.** The `checked_sub` chain returns 0 rather than a negative (`src/storage.rs:41`), which means an accounting bug shows up as "no budget", not as an underflow. Audit finding M6 accepted this silently-clamping behavior.
- **`README.md` has drifted from this contract.** `README.md:115` documents `register_campaign(vault)` without the `asset` parameter, and neither `reallocate` nor `rescue_orphan` appears in the entrypoint or event lists (`README.md:114`, `README.md:124`). Trust the source.

## Testing

- 51 `#[test]` cases in `src/test.rs` (measured). `docs/internal-audit.md` quotes 50, which predates a later addition.
- Two mocks: a single-asset `MockVault` (`src/test.rs:19`) and a `MultiAssetMockVault` (`src/test.rs:50`) used to exercise `MultiAssetVaultNotSupported`.
- `mod integration_tests` (`src/test.rs:920`) uses `contractimport!(file = "../../external-contracts/defindex_vault.optimized.wasm")` (`src/test.rs:924`) specifically to prove the local `VaultAssetStrategySet` mirror decodes the real vault's `get_assets()` return type.
- Run: `cargo test -p boost-treasury` (`README.md:73`).
