# Poller Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/poller/` · **Last verified:** 2026-09-04

## Purpose

A long-running process that asks the DeFindex API for each tracked vault's APY once an hour and appends the reading to a local `apy_history` Postgres table. It builds the historical series used for offline analysis; nothing in the fee-control path reads from it.

## Structure

| File | Purpose |
|---|---|
| `apy-history.ts` | The whole module: banner, HTTP fetch, per-vault record, tick loop. |

Storage and vault lists live outside the folder: `src/helpers/db.ts` and `src/constants/index.ts` (see [shared.md](shared.md)).

## Public surface

A process, not a library. No exports. Entry point `main()` at `apy-history.ts:65`, launched by `pnpm apy-poller` (`package.json:11`).

## Key methods

- **`fetchVaultApy(vaultAddress)`** (`apy-history.ts:28`) — `GET {DEFINDEX_API_URL}/vault/{address}/apy?network={APY_POLL_NETWORK}` with a `Bearer` token from `DEFINDEX_API_KEY`. Rejects non-2xx and any body whose `apy` is not a number (`apy-history.ts:41`).
- **`recordOneVault(vaultAddress)`** (`apy-history.ts:47`) — resolves a human label from `VAULT_NAMES` (falling back to the raw address), fetches, inserts, and swallows failures into a log line so one bad vault does not stop the tick.
- **`main()`** (`apy-history.ts:65`) — calls `ensureApyHistoryTable()` once at boot, then loops `runTick` / `sleep` forever with a per-tick try/catch.

## Dependencies

- **DeFindex API** — the `/vault/:id/apy` endpoint. Base URL and key come from `DEFINDEX_API_URL` and `DEFINDEX_API_KEY` (`apy-history.ts:29`).
- **A Postgres database of its own**, reached through `DATABASE_URL` (`src/helpers/db.ts:7`). This is *not* the indexer DB the stabilizer uses.
- Internal: `src/helpers/db.ts` for `ensureApyHistoryTable` and `insertApySample`; `src/helpers/log.ts` for `log` and `sleep`; `src/constants/index.ts` for `APY_POLL_VAULTS`, `VAULT_NAMES`, `APY_POLL_INTERVAL_MS`, `APY_POLL_NETWORK`.
- npm: `pg`, `dotenv`. HTTP uses the Node global `fetch`, no client library.

## Gotchas and invariants

- **None of this module's env vars are in `.env.example`.** `DEFINDEX_API_URL` and `DEFINDEX_API_KEY` (`apy-history.ts:29`) and `DATABASE_URL` (`src/helpers/db.ts:7`) are absent from the template, which only covers the stabilizer. Both API vars are cast with `as string` and never validated, so a missing value produces a request to `undefined/vault/...` rather than a clear preflight failure. Contrast with the stabilizer's `preflight()`.
- **`APY_POLL_VAULTS` is deliberately separate from `VAULTS`.** Polling scope is decoupled from the deposit/withdraw cron's scope (`src/constants/index.ts:10`). It currently holds a single vault (`src/constants/index.ts:14`).
- **The API returns a percentage, the stabilizer works in decimals.** The value is stored verbatim and logged with a `%` suffix (`apy-history.ts:52`), while `calculateLiveGrossApy` returns a decimal. Do not mix the two series without converting.
- **The table is created on every boot.** `ensureApyHistoryTable` runs `CREATE TABLE IF NOT EXISTS` plus an index at startup (`src/helpers/db.ts:12`), so the poller needs DDL rights on its database.
- **This pool has no `error` listener and no SSL config** (`src/helpers/db.ts:5`), unlike the stabilizer's pool. An idle-client error will surface as an uncaught exception.
- **Interval is a compile-time constant**, not env-driven: 1 hour at `src/constants/index.ts:50`.

## Testing

No automated tests. There is no test script for this module in `package.json` and no `__tests__` folder under `src/poller/`. Verification is manual: run `pnpm apy-poller` and check the per-vault log lines and the rows landing in `apy_history`.
