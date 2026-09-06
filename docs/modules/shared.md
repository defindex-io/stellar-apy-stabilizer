# Shared Helpers Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/helpers/`, `src/constants/` · **Last verified:** 2026-09-04

## Purpose

The small shared layer under the Node entry points: timestamped logging, a Postgres pool for the poller's own database, Stellar transaction plumbing for the vault cron, the vault `deposit`/`withdraw` calls, and the address and tuning constants those two modules read. Changing anything here touches every Node process in the repo, and `log.ts` in particular is imported by the stabilizer as well.

## Structure

| File | Purpose |
|---|---|
| `helpers/log.ts` | `log` (UTC-stamped console line) and `sleep`. Imported by every entry point including the stabilizer. |
| `helpers/db.ts` | Lazy `pg.Pool` on `DATABASE_URL`, plus the `apy_history` DDL and insert. Used only by the poller. |
| `helpers/stellar.ts` | Module-scope `rpc.Server`, keypair loading, build/simulate/sign/send/poll. Used only by the vault cron. |
| `helpers/vault.ts` | `deposit` / `withdraw` invocations and the all-vaults iterator. Used only by the vault cron. |
| `constants/index.ts` | Vault address lists, human labels, amounts, network passphrase, tx and interval tuning. |

## Public surface

| Export | Location | Notes |
|---|---|---|
| `log(message)`, `sleep(ms)` | `helpers/log.ts:1`, `:6` | `log` prefixes an ISO-8601 UTC timestamp so PM2 output is uniform across services. |
| `getPool()` | `helpers/db.ts:5` | Memoized pool on `DATABASE_URL`. |
| `ensureApyHistoryTable()` | `helpers/db.ts:11` | `CREATE TABLE IF NOT EXISTS apy_history` plus a `(vault_address, recorded_at DESC)` index. |
| `insertApySample(vaultAddress, vaultName, apy)` | `helpers/db.ts:25` | Parameterized insert. |
| `rpcServer` | `helpers/stellar.ts:11` | Constructed at import time. See Gotchas. |
| `getCallerKeypair()` | `helpers/stellar.ts:13` | Reads `STELLAR_SECRET_KEY`. |
| `sendAndConfirm(operation, kp)` | `helpers/stellar.ts:17` | Build, simulate, assemble, sign, send, poll to `SUCCESS`. Throws on any failure. |
| `depositToVault(vaultId, kp)` | `helpers/vault.ts:15` | `deposit(amounts_desired, amounts_min, from, invest)`. |
| `withdrawFromVault(vaultId, kp)` | `helpers/vault.ts:28` | `withdraw(withdraw_shares, min_amounts_out, from)`. |
| `runOnAllVaults(label, fn, kp)` | `helpers/vault.ts:39` | Iterates `VAULTS`, catches per-vault errors, returns a failure count. |

Constants worth knowing (`constants/index.ts`): `VAULTS` (four addresses, `:3`), `APY_POLL_VAULTS` (`:13`), `VAULT_NAMES` (`:19`), `DEPOSIT_AMOUNT_STROOPS` (`:28`), `WITHDRAW_SHARES` (`:33`), `NETWORK_PASSPHRASE` (`:38`), `CRON_INTERVAL_MS` (`:47`), `APY_POLL_INTERVAL_MS` (`:50`), `APY_POLL_NETWORK` (`:53`).

## Dependencies

- npm: `@stellar/stellar-sdk`, `pg` (`package.json:16`).
- Env: `DATABASE_URL` (`helpers/db.ts:7`), `SOROBAN_RPC` (`helpers/stellar.ts:11`), `STELLAR_SECRET_KEY` (`helpers/stellar.ts:14`). None of the three appears in `.env.example`.
- Consumed by [poller.md](poller.md) (`log`, `db`, constants), [vault-cron.md](vault-cron.md) (all of it), and [stabilizer.md](stabilizer.md) (`log` only).

## Gotchas and invariants

- **`helpers/stellar.ts:11` runs at import time.** `export const rpcServer = new rpc.Server(process.env.SOROBAN_RPC as string)` is a module-level side effect. Any file that imports from here, directly or transitively, must have loaded `dotenv/config` first or it gets a server built on `undefined`. The stabilizer deliberately avoids this with a lazy `getRpcServer()` (`src/stabilizer/stellar-rpc.ts:29`); prefer that shape for new code.
- **Three env vars, no validation.** All three are read with `as string` and none is preflight-checked. Failures surface as confusing runtime errors rather than a clear startup message.
- **Two different databases live in this repo.** `DATABASE_URL` here is the poller's own store; `INDEXER_DATABASE_URL` in the stabilizer is the read-only DeFindex indexer. They are unrelated and must not be pointed at the same place.
- **`helpers/db.ts` duplicates `src/stabilizer/indexer-db.ts` badly on purpose or by accident.** This pool has no SSL block and no `pool.on("error")` listener (`helpers/db.ts:5`), both of which the stabilizer's pool needs to survive managed Postgres. If this module ever runs against a managed provider, port those two pieces over.
- **`NETWORK_PASSPHRASE` here is a single value pinned to mainnet** (`constants/index.ts:38`), unlike the stabilizer's per-network record (`src/stabilizer/constants.ts:49`). There are also duplicate `TX_FEE`, `TX_TIMEOUT_SECONDS`, `POLL_INTERVAL_MS` and `POLL_TIMEOUT_MS` constants in both files with the same values; changing one does not change the other.
- **`VAULTS` and `APY_POLL_VAULTS` are intentionally separate lists** (`constants/index.ts:10`). Only `VAULT_NAMES` spans both.
- **`sendAndConfirm` throws; `signAndSubmit` in the stabilizer does not.** Callers of this one must wrap it, which is why `runOnAllVaults` has its own try/catch.

## Testing

No automated tests for any file in this module. `log` and `sleep` are trivial; `sendAndConfirm` and the vault calls are only exercised by running the vault cron against mainnet. If the vault call shapes at `helpers/vault.ts:12` ever change, the only signal is a failing on-chain simulation.
