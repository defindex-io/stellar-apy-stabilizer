# Module Documentation Index

Living docs — one per module. **Read the relevant doc before modifying a module; update it in the same change.** See the "Module Documentation Convention" section in `CLAUDE.md` for the workflow.

## On-chain (Rust / Soroban)

| Doc | Module | One-liner |
|---|---|---|
| [fee-proxy.md](fee-proxy.md) | `contracts/fee-proxy/` | Holds the DeFindex vault Manager role and exposes an auth-gated fee surface to an off-chain fee manager. |
| [boost-treasury.md](boost-treasury.md) | `contracts/boost-treasury/` | Per-vault escrow for partner-funded boost budgets, streamed into vaults by a manager key. |

## Off-chain (TypeScript / Node)

| Doc | Module | One-liner |
|---|---|---|
| [stabilizer.md](stabilizer.md) | `src/stabilizer/` | Hourly fee-control loop that keeps each vault's net APY near its target by calling FeeProxy `lock_fees`. |
| [poller.md](poller.md) | `src/poller/` | Hourly poller that records DeFindex API vault APY into a local `apy_history` table. |
| [vault-cron.md](vault-cron.md) | `src/vault/` | Deposit/withdraw cron used to generate vault activity for backtesting. |
| [backtest.md](backtest.md) | `src/backtest/` | Offline CSV backtest of a divergence threshold for a dual-signal fee gate (not wired into the bot). |
| [shared.md](shared.md) | `src/helpers/`, `src/constants/` | Logging, Postgres pool, Stellar tx helpers and vault-address constants shared by the Node entry points. |

## Operations

| Doc | Module | One-liner |
|---|---|---|
| [ops-scripts.md](ops-scripts.md) | `src/bash/`, `src/fee-proxy/bash/`, `src/boost-treasury/bash/` | Interactive Stellar CLI scripts to deploy and operate both contracts, including unsigned-XDR builds for multisig. |
