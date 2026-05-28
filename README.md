# APY Stabilizer

Soroban contracts that let a DeFindex vault partner opt into a managed fee policy with an optional yield-boost campaign. Two contracts live here:

- **FeeProxy** — holds the vault's Manager role and exposes a narrow, auth-gated surface so an off-chain fee manager can adjust performance fees within partner-defined bounds.
- **BoostTreasury** — escrows partner-funded boost budgets and streams them into vaults to top up realized yield toward a target APY.

The two contracts are independent on-chain singletons. An off-chain fee service orchestrates them together, and an indexer consumes their events.

The off-chain fee service (the **APY Stabilizer bot**) ships in this repo too — a long-running PM2 process under `src/stabilizer/` that runs the fee-control loop against the deployed FeeProxy. See [`docs/STABILIZER.md`](docs/STABILIZER.md) for setup, configuration, the dry-run → live workflow, and the operational runbook.

## Repo Layout

```
contracts/
  fee-proxy/          FeeProxy contract (Rust / Soroban SDK 25.3)
  boost-treasury/     BoostTreasury contract
external-contracts/
  defindex_vault.optimized.wasm   Real vault WASM for integration tests
src/
  stabilizer/         APY Stabilizer bot — hourly fee-control loop (TypeScript / PM2)
  poller/             APY-history poller (TypeScript / PM2)
  vault/              Deposit/withdraw cron used in backtesting (TypeScript / PM2)
docs/
  STABILIZER.md                   APY Stabilizer bot setup + operations
  APY_STABILIZER_PROPOSAL.md      Design proposal
  internal-audit.md               Contract security audit
  stride-threat-model.md          STRIDE threat model
.env.example          Environment template for the off-chain bot
```

Cargo workspace at the root pins `soroban-sdk = "25.3.1"` and release flags tuned for WASM size.

## Prerequisites

- Rust toolchain with `wasm32v1-none` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli) ≥ 26.0

```bash
rustup target add wasm32v1-none
cargo install --locked stellar-cli
```

## Build

Build both contracts to optimized WASM:

```bash
stellar contract build
```

Artifacts land in `target/wasm32v1-none/release/fee_proxy.wasm` and `target/wasm32v1-none/release/boost_treasury.wasm`.

To build a single contract:

```bash
stellar contract build --package fee-proxy
stellar contract build --package boost-treasury
```

## Test

Run the full suite (unit tests + integration tests that load the real vault WASM):

```bash
cargo test
```

Scoped runs:

```bash
cargo test -p fee-proxy
cargo test -p boost-treasury
cargo test -p fee-proxy integration_tests
```

Integration tests in `contracts/fee-proxy/src/test.rs` register the vault WASM from `external-contracts/` so Manager-role handoffs and fee flows execute against real vault bytecode rather than a mock.

## FeeProxy

A Manager-role holder for DeFindex vaults. Partners register their vault and delegate Manager to this contract; the proxy then exposes a scoped surface the off-chain fee manager (a shared service key) and the partner admin can both call, each constrained to their lane.

**State**
- Global: `admin`, `fee_manager`, `pending_admin`
- Per vault: `VaultConfig { admin, target_apy_bps, max_fee_bps, min_fee_bps }`

**Read-only** — `get_admin`, `get_fee_manager`, `get_pending_admin`, `get_vault_config`.

**Entrypoints**
- `register_vault(admin, vault, config)` — `admin` is the caller and must be the vault's **current Manager** (the proxy calls `vault.set_manager(proxy)` on success, which requires the old manager's auth). `config.admin` is the address that controls this vault through the proxy going forward — it does **not** need to equal `admin`, allowing partners to delegate to a different operational key. Validates `max_fee_bps ≤ 10_000` and `min_fee_bps ≤ max_fee_bps`; `target_apy_bps` accepts the full `u32` range.
- `unregister_vault` — vault admin only; returns Manager role to `config.admin` and removes the vault config.
- `lock_fees(caller, vault, new_fee_bps)` — fee manager or vault admin. If `new_fee_bps = Some(x)`, enforces `min ≤ x ≤ max` before the passthrough and emits `FeesLocked`. If `None`, skips validation and passes through without emitting (re-locks the existing fee).
- `distribute_fees` — fee manager or vault admin; calls the vault via the Manager role.
- `release_fees` — vault admin only; calls the vault via the Manager role (no event emitted).
- `set_target_apy` — vault admin; stores the new target (full `u32` range allowed).
- `set_fee_bounds` — vault admin; re-validates fee bounds.
- `set_vault_manager(vault, new_manager)` — vault admin. Passes through `set_manager` on the vault; if `new_manager ≠ proxy`, also removes the vault config and emits `VaultUnregistered` (the proxy can no longer control the vault).
- Passthroughs — vault admin acts on the vault through the proxy without losing the Manager role: `upgrade_vault`, `set_vault_fee_receiver`, `set_vault_emergency_manager`, `set_vault_rebalance_manager`, `rescue_vault`, `pause_vault_strategy`, `unpause_vault_strategy`.
- `propose_admin` / `accept_admin` — two-step global admin rotation.
- `set_fee_manager` — admin rotates the off-chain fee manager key in one shot.

**Events** — `VaultRegistered`, `VaultUnregistered`, `FeesLocked`, `FeesDistributed`, `ConfigUpdated`, `FeeManagerUpdated`, `AdminProposed`, `AdminUpdated`.

## BoostTreasury

Per-vault escrow for top-up budgets. Anyone can deposit into a registered vault's campaign; only the configured `manager` address can call `boost` to stream funds from escrow to the vault as realized yield.

**State**
- Global: `admin`, `manager`, `pending_admin`
- Per vault: `Campaign { active, asset, total_deposited, total_boosted, total_withdrawn, last_boosted_at }` where `available = total_deposited - total_boosted - total_withdrawn`.

**Read-only** — `get_admin`, `get_manager`, `get_pending_admin`, `get_campaign`.

**Entrypoints**
- `register_campaign(vault)` — admin only. Reads the vault's `get_assets()` and rejects multi-asset vaults (one asset per campaign — `MultiAssetVaultNotSupported`). Asset is captured on registration and immutable afterward.
- `update_campaign(vault, active)` — admin toggles the `active` flag. Deposits and boosts require `active = true`; `transfer` does **not** (admin escape hatch from an inactive campaign).
- `unregister_campaign(vault)` — admin only; requires `available == 0` (`CampaignHasBalance` otherwise).
- `deposit(caller, vault, amount)` — anyone with `caller.require_auth()`. Requires `amount > 0` and `active = true`. Pulls tokens from the caller into escrow via `token.transfer`.
- `boost(vault, amount)` — manager only. Requires `amount > 0`, `active = true`, and `amount ≤ available` (`InsufficientBudget` otherwise). Transfers from escrow to the vault and stamps `last_boosted_at = ledger.timestamp()`.
- `transfer(vault, amount, to)` — admin only. Requires `amount > 0` and `amount ≤ available`; does **not** require `active`. Transfers from escrow to `to` and accounts the amount against `total_withdrawn`.
- `set_manager` — admin rotates the manager key (the address authorized to call `boost`) in one shot.
- `propose_admin` / `accept_admin` — two-step global admin rotation.

**Events** — `CampaignRegistered`, `CampaignUpdated`, `CampaignUnregistered`, `Deposited`, `Boosted`, `Transferred`, `ManagerUpdated`, `AdminProposed`, `AdminUpdated`.

## Deploy (Testnet Sketch)

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/fee_proxy.wasm \
  --source <account> --network testnet \
  -- __constructor --admin <G…> --fee_manager <G…>

stellar contract deploy \
  --wasm target/wasm32v1-none/release/boost_treasury.wasm \
  --source <account> --network testnet \
  -- __constructor --admin <G…> --manager <G…>
```

The Stellar CLI maps constructor struct fields to flags (`--admin`, `--fee_manager`, `--manager`). Both contracts call `admin.require_auth()` in `__constructor`, so the admin must sign the deploy invocation.
