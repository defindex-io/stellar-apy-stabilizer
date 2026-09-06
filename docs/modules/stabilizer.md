# Stabilizer Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/stabilizer/` · **Last verified:** 2026-09-04

## Purpose

The off-chain fee-control loop. Once an hour it discovers every vault whose on-chain Manager is the FeeProxy, measures each vault's live gross APY, computes the performance fee that would land net APY on the partner's target, and submits `lock_fees` to the FeeProxy. It is the only component in this repo that signs mainnet transactions with a hot key, so changes here have direct financial blast radius on live partner vaults.

Full operator runbook lives in `docs/STABILIZER.md`.

## Structure

| File | Purpose |
|---|---|
| `cron.ts` | Process entry point: banner, env preflight, infinite tick loop. |
| `fee-stabilizer.ts` | Pure controller math plus the per-cycle and per-vault orchestration. |
| `apy-calculation.ts` | Live gross APY from an on-chain read against an indexer start snapshot. |
| `proxy-contract.ts` | The two FeeProxy calls: `get_vault_config` read, `lock_fees` write. |
| `indexer-db.ts` | Lazy read-only `pg.Pool` and the three SQL queries against the indexer `parsed.*` schema. |
| `stellar-rpc.ts` | Lazy `rpc.Server`, keypair loading, simulate / multi-invoke / sign+submit. |
| `constants.ts` | Contract addresses, controller tuning, tx mechanics, method-name maps. |
| `types.ts` | Shared interfaces for config, indexer rows, and cycle results. |
| `__tests__/fee-stabilizer.test.ts` | `node:test` cases over the pure math. |

## Public surface

This is a process, not a library. It has no HTTP endpoints. The entry point is `main()` in `cron.ts`, launched by `pnpm stabilizer` (`package.json:13`).

Exported functions worth knowing:

| Export | Location | Notes |
|---|---|---|
| `runStabilizationCycle(network)` | `fee-stabilizer.ts:59` | One full tick. Returns a `StabilizationCycleResult`. |
| `calculateRequiredFee(strategyApy, targetApyBps)` | `fee-stabilizer.ts:25` | Pure. `round((1 - target/gross) * 10_000)`, floored at 0. |
| `shouldAdjust(current, required)` | `fee-stabilizer.ts:33` | Pure. Strict `>` against `DEAD_ZONE_BPS`. |
| `applyRateLimit(current, required, maxDelta?)` | `fee-stabilizer.ts:37` | Pure. Clamps the per-cycle move. |
| `calculateLiveGrossApy(network, vault, days)` | `apy-calculation.ts:32` | Hybrid chain + indexer APY measurement. |
| `getVaultConfig` / `lockFees` | `proxy-contract.ts:17`, `:38` | The FeeProxy boundary. |
| `getManagedVaultsWithApy` / `getVaultHistoricalStates` / `getHistoricalLockedFees` | `indexer-db.ts:77`, `:112`, `:167` | The indexer boundary. |

## Key methods

- **`processVault`** (`fee-stabilizer.ts:123`) — the decision tree for one vault. Reads config, measures APY, then branches: gross APY at or below target clamps required fee to `minFeeBps` (`fee-stabilizer.ts:163`); otherwise the formula result is clamped into `[minFeeBps, maxFeeBps]` (`fee-stabilizer.ts:174`). Every failure is caught and returned as `action: "error"` so one bad vault never aborts the cycle.
- **`submit`** (`fee-stabilizer.ts:183`) — applies the rate limit, then either short-circuits on `DRY_RUN` or calls `lockFees`. This is the single on-chain write in the module.
- **`calculateLiveGrossApy`** (`apy-calculation.ts:32`) — end point is a live chain read of `fetch_total_managed_funds`, `total_supply` and `report` batched through the DeFindex router (`apy-calculation.ts:44`); start point is the nearest indexer snapshot at or before `now - days`. Gross PPS adds back locked fees on both ends so the measured APY is pre-fee. Annualization uses 365.2425 days (`apy-calculation.ts:129`).
- **`findStartSnapshot`** (`apy-calculation.ts:142`) — widens the search by `SNAPSHOT_SEARCH_WINDOW_DAYS` before `fromDate`, picks the latest snapshot at or before the target, and falls back to the earliest snapshot in the window when none is old enough.
- **`simulateMultipleInvocations`** (`stellar-rpc.ts:97`) — wraps N calls into one DeFindex router `exec`. It passes the **vault address** as the router's `caller`, which is what makes the manager-gated `report` readable in simulation without a signature (`apy-calculation.ts:40` explains why).
- **`signAndSubmit`** (`stellar-rpc.ts:125`) — build, simulate, assemble, sign, send, then poll to `SUCCESS`. Never throws: it returns `{ success: false, errorMessage }` so the caller records an error result instead of crashing the loop.

## Dependencies

- **FeeProxy contract** — reads `get_vault_config`, writes `lock_fees` (`constants.ts:60`). Address from `FEE_PROXY_ADDRESS` (`constants.ts:6`).
- **DeFindex vault contracts** — reads `fetch_total_managed_funds`, `total_supply`, `report` (`constants.ts:66`).
- **DeFindex Stellar router** — `exec` for batched simulation. Mainnet `CDAW42JDSDEI2DXEPP4E7OAYNCRUA4LGCZHXCJ4BV5WVI4O4P77FO4UV` (`constants.ts:16`), sourced from `defindex-api/src/helpers/constants.ts` per the comment at `constants.ts:14`.
- **DeFindex indexer Postgres** — read-only against `parsed.v_vault_apy`, `parsed.vault_role_change`, `parsed.vault_transaction`, `parsed.vault_transaction_asset`, `parsed.vault_strategy_report`.
- **Stellar Soroban RPC** — `SOROBAN_RPC` (`stellar-rpc.ts:31`).
- npm: `@stellar/stellar-sdk`, `pg`, `dotenv` (`package.json:16`).
- Internal: `src/helpers/log.ts` for `log` and `sleep` (see [shared.md](shared.md)).

Env vars: `SOROBAN_RPC`, `INDEXER_DATABASE_URL`, `FEE_MANAGER_SECRET_KEY` are required and enforced by `preflight()` (`cron.ts:20`). Optional: `INDEXER_DATABASE_SSL` (`indexer-db.ts:21`), `FEE_PROXY_ADDRESS_MAINNET` (`constants.ts:8`), `VAULT_OPS_DRY_RUN` (`constants.ts:40`), `STABILIZER_INTERVAL_MS` (`constants.ts:43`).

## Gotchas and invariants

- **The hardcoded FeeProxy default does not match `mainnet.contracts.json`.** `constants.ts:9` defaults to `CDEFLWJMPR6DDNOEGP6KNPSPRWKPUG3DJLIOQZIS6EHIGNK7EGTQSA7R`, which is the address in `old.mainnet.contracts.bck.json:2`. `mainnet.contracts.json:2` holds `CBXBQ7SI5UOCDFGUKBAQS4HIZOUSVTSJYY4L2ZJRBHIYCKDDC3PQ3QTB`. `monitor.sql:32` also filters on the old address. Without `FEE_PROXY_ADDRESS_MAINNET` set, the bot targets the older proxy and will report `discovered 0 managed vault(s)` if the newer one holds the Manager roles. Reconcile before running live.
- **`DRY_RUN` is on unless the env var is the literal string `"false"`.** `(process.env.VAULT_OPS_DRY_RUN ?? "true") !== "false"` (`constants.ts:40`). Any other value, including `"0"` or `"FALSE"`, keeps it read-only.
- **A dry run still counts as `adjusted`.** `submit` returns `action: "adjusted"` with `txHash: "DRY_RUN"` (`fee-stabilizer.ts:196`), so `adjustmentsMade` in the cycle summary is not a count of on-chain writes. Check the `tx=` field in the log line.
- **Testnet is dead code.** `FEE_PROXY_ADDRESS.testnet` is the placeholder string `"C_TESTNET_TBD_AFTER_DEPLOYMENT"` (`constants.ts:10`) and `cron.ts:34` hardcodes `"mainnet"`.
- **Single-asset assumption throughout the APY math.** Only index 0 of the managed-funds array is read, both live (`apy-calculation.ts:55`) and historical (`apy-calculation.ts:93`). A multi-asset vault would silently produce a wrong APY.
- **Short windows return `null`, not a noisy number.** Below 10 minutes of actual elapsed time the annualization is abandoned (`apy-calculation.ts:115`), which surfaces as `skipped_no_data`.
- **The pg pool needs its `error` listener.** Without it, an idle-client error from a DB restart becomes an uncaught exception and kills the process mid-cycle (`indexer-db.ts:28`). Do not remove it.
- **Indexer TLS skips endpoint identity verification.** `rejectUnauthorized: false` (`indexer-db.ts:25`); the connection is encrypted but the cert chain is not validated. This mirrors the API's behavior and is deliberate.
- **Vault discovery depends on the `role_type = 'manager'` filter.** `getManagedVaultsWithApy` takes the latest role change per vault via `DISTINCT ON` (`indexer-db.ts:85`). Dropping that filter would match fee-receiver or emergency-manager rotations and pull in vaults the proxy does not control.
- **Module-level side effects are avoided on purpose.** The RPC server (`stellar-rpc.ts:29`) and the pg pool (`indexer-db.ts:11`) are both lazily constructed so the pure math is importable and testable with no env set. Keep it that way; `src/helpers/stellar.ts` is the counter-example that does construct at import time.
- **The keypair lives in process memory for the process lifetime** (`stellar-rpc.ts:37`). Never log it, and rotate via FeeProxy `set_fee_manager` when the host changes (`docs/STABILIZER.md:306`).

## Testing

- 16 `node:test` cases in `__tests__/fee-stabilizer.test.ts`, covering `calculateRequiredFee`, `shouldAdjust` (including both sides of the dead-zone boundary at `:44` and `:47`) and `applyRateLimit`.
- Run: `pnpm test:stabilizer` (`package.json:14`). No env, DB or network needed.
- **Gap:** everything with I/O is untested. `processVault`, `calculateLiveGrossApy`, all three SQL queries and `signAndSubmit` have no coverage. There is no mocking harness in the repo, so new control-loop logic should be pushed into pure functions in `fee-stabilizer.ts` where it can be tested.
- Per `docs/STABILIZER.md:262`, do not gate on `tsc --noEmit`; the repo runs through `tsx` and the Stellar SDK's transitive declarations do not always resolve cleanly.
