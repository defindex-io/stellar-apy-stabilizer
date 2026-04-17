# APY Stabilizer

Soroban contracts that let a DeFindex vault partner opt into a managed fee policy with an optional yield-boost campaign. Two contracts live here:

- **FeeProxy** — holds the vault's Manager role and exposes a narrow, auth-gated surface so an off-chain fee manager can adjust performance fees within partner-defined bounds.
- **BoostTreasury** — escrows partner-funded boost budgets and streams them into vaults to top up realized yield toward a target APY.

The two contracts are independent on-chain singletons. The off-chain fee service in [`defindex-api`](../defindex-api) orchestrates them together; the indexer in [`defindex-indexer`](../defindex-indexer) consumes their events.

## Repo Layout

```
contracts/
  fee-proxy/          FeeProxy contract (Rust / Soroban SDK 25.3)
  boost-treasury/     BoostTreasury contract
external-contracts/
  defindex_vault.optimized.wasm   Real vault WASM for integration tests
docs/
  APY_STABILIZER_PROPOSAL.md      Design proposal
```

Cargo workspace at the root pins `soroban-sdk = "25.3.1"` and release flags tuned for WASM size.

## Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli) ≥ 26.0

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli
```

## Build

Build both contracts to optimized WASM:

```bash
stellar contract build
```

Artifacts land in `target/wasm32-unknown-unknown/release/fee_proxy.wasm` and `target/wasm32-unknown-unknown/release/boost_treasury.wasm`.

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
- Per vault: `VaultConfig { admin, target_apy_bps, min_fee_bps, max_fee_bps }`

**Entrypoints**
- `register_vault` / `unregister_vault` — partner admin registers; proxy becomes Manager. Fee bounds validated (`max ≤ 10_000`, `min ≤ max`) and `target_apy_bps ≤ 100_000` (10% cap = `MAX_TARGET_APY_BPS`).
- `lock_fees(caller, vault, new_fee_bps)` — fee manager or vault admin; enforces the per-vault `[min, max]` corridor before calling through to the vault.
- `distribute_fees`, `release_fees` — fee manager or admin call vault routines via the Manager role.
- `set_target_apy`, `set_fee_bounds` — vault admin updates config; target APY cap re-validated.
- Passthroughs (`upgrade_vault`, `set_vault_manager`, `set_vault_fee_receiver`, `set_vault_emergency_manager`, `set_vault_rebalance_manager`, `rescue_vault`, `pause_vault_strategy`, `unpause_vault_strategy`) — partner admin acts on the vault through the proxy without losing the Manager role.
- `propose_admin` / `accept_admin` — two-step global admin rotation.
- `set_fee_manager` — admin rotates the off-chain fee manager key in one shot.

**Events** — `VaultRegistered`, `VaultUnregistered`, `FeesLocked`, `FeesDistributed`, `ConfigUpdated`, `FeeManagerUpdated`, `AdminProposed`, `AdminUpdated`.

## BoostTreasury

Per-vault escrow for top-up budgets. Anyone can deposit into a registered vault's campaign; only the configured `manager` address can call `boost` to stream funds from escrow to the vault as realized yield.

**State**
- Global: `admin`, `manager`, `pending_admin`
- Per vault: `Campaign { active, asset, total_deposited, total_boosted, total_withdrawn, last_boosted_at }` where `available = total_deposited - total_boosted - total_withdrawn`.

**Entrypoints**
- `register_campaign(vault)` — admin only. Reads the vault's `get_assets()` and rejects multi-asset vaults (one asset per campaign). Asset is locked on registration.
- `update_campaign(vault, active)` — admin toggles the active flag (deposits and boosts require `active = true`).
- `unregister_campaign(vault)` — admin only, requires `available == 0`.
- `deposit(caller, vault, amount)` — anyone authenticated; `token.transfer` from caller into escrow.
- `boost(vault, amount)` — manager only; transfers from escrow to the vault and stamps `last_boosted_at`. Reverts on `amount > available`.
- `transfer(vault, amount, to)` — admin-only withdrawal from escrow, accounted against `total_withdrawn`.
- `set_manager`, `propose_admin` / `accept_admin` — role rotation.

**Events** — `CampaignRegistered`, `CampaignUpdated`, `CampaignUnregistered`, `Deposited`, `Boosted`, `Transferred`, `ManagerUpdated`, `AdminProposed`, `AdminUpdated`.

## Deploy (Testnet Sketch)

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/fee_proxy.wasm \
  --source <account> --network testnet \
  -- __constructor --admin <G…> --fee_manager <G…>

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/boost_treasury.wasm \
  --source <account> --network testnet \
  -- __constructor --admin <G…> --manager <G…>
```

Constructor args are positional for `__constructor`; both contracts require the admin signature at deploy.

## Further Reading

- `docs/APY_STABILIZER_PROPOSAL.md` — original proposal
- `defindex-api/src/vault-ops/` — off-chain fee + boost orchestration service
- `defindex-indexer/src/parsers/` — event parsers and `parsed.*` schema
