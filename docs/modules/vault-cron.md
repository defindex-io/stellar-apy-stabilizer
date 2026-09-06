# Vault Cron Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/vault/` · **Last verified:** 2026-09-05

## Purpose

Three small entry points that deposit into and withdraw from a fixed list of DeFindex vaults on mainnet, either once or on a 4-hour alternating loop. Their job is to generate vault transactions so the indexer has price-per-share snapshots to measure APY against. They move real funds with a real key, so treat them as production scripts even though they exist for backtesting.

## Structure

| File | Purpose |
|---|---|
| `deposit.ts` | One-shot deposit across all vaults, exits non-zero on any failure. |
| `withdraw.ts` | One-shot withdraw across all vaults, same exit behavior. |
| `cron.ts` | Long-running loop that alternates deposit and withdraw every tick. |

The actual contract calls live in `src/helpers/vault.ts` and `src/helpers/stellar.ts` (see [shared.md](shared.md)). The files in this folder are thin wrappers around them.

## Public surface

Processes, not libraries. Launched by `pnpm deposit`, `pnpm withdraw`, `pnpm cron` (`package.json:8`).

## Key methods

- **`main()` in `cron.ts`** (`cron.ts:31`) — starts with `deposit`, sleeps `CRON_INTERVAL_MS`, then flips the action each iteration (`cron.ts:45`). A crashed tick is logged and the loop continues.
- **`runTick(action)`** (`cron.ts:22`) — resolves the keypair fresh each tick and delegates to `runOnAllVaults`.
- **`main()` in `deposit.ts` / `withdraw.ts`** (`deposit.ts:19`, `withdraw.ts:19`) — run one pass and `process.exit(failures > 0 ? 1 : 0)`, which makes them usable from an external scheduler.

## Dependencies

- **DeFindex vault contracts** — `deposit` and `withdraw` invoked by symbol (`src/helpers/vault.ts:24`, `:35`) against the four addresses in `VAULTS` (`src/constants/index.ts:3`).
- **Stellar Soroban RPC** via `SOROBAN_RPC` (`src/helpers/stellar.ts:11`).
- **A funded Stellar key** via `STELLAR_SECRET_KEY` (`src/helpers/stellar.ts:14`).
- Internal: `src/helpers/vault.ts`, `src/helpers/stellar.ts`, `src/helpers/log.ts`, `src/constants/index.ts`.

## Gotchas and invariants

- **`SOROBAN_RPC` is read at import time, not on first use.** `src/helpers/stellar.ts:11` constructs `rpc.Server` at module scope. Importing anything from that file without the env loaded yields a server pointed at `undefined`. This is why every entry point in this folder starts with `import "dotenv/config"` on line 1, and why import order matters. The stabilizer module solved the same problem lazily; do not copy this pattern into new code.
- **Neither `SOROBAN_RPC` for this path nor `STELLAR_SECRET_KEY` is in `.env.example`.** The template only documents the stabilizer's variables. Both are cast with `as string` and never validated.
- **Mainnet is hardcoded.** `NETWORK_PASSPHRASE = Networks.PUBLIC` (`src/constants/index.ts:38`). There is no network switch in this path.
- **Withdraw is denominated in shares, deposit in stroops.** `WITHDRAW_SHARES = 1_000_000n` (`src/constants/index.ts:33`) is a fixed share count, not the shares actually minted by the matching deposit. The comment there acknowledges this only holds while a vault is near 1:1. On a drifted vault the loop will not be balance-neutral.
- **`MIN_AMOUNT_OUT_STROOPS = 0n`** (`src/constants/index.ts:35`) means withdrawals accept any output amount. Acceptable for a test harness, unsafe as a template for user-facing code.
- **Failures are counted, not raised.** `runOnAllVaults` (`src/helpers/vault.ts:39`) catches per-vault errors and returns a failure count, so a partially failed pass still "completes".
- **`VAULTS` and `APY_POLL_VAULTS` are disjoint lists.** `VAULTS` holds four addresses (`src/constants/index.ts:3`); `APY_POLL_VAULTS` holds one (`src/constants/index.ts:13`) that appears in neither. Only `VAULT_NAMES` spans both. Changing one does not change the other.

## Testing

No automated tests. Verification is manual against mainnet, which means every run costs real funds and fees. Prefer reasoning about `src/helpers/vault.ts` and `src/helpers/stellar.ts` over running these scripts to "see what happens".
