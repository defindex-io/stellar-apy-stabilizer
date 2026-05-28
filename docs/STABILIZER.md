# APY Stabilizer Bot

A standalone, long-running service that adjusts DeFindex vault performance
fees once an hour so each vault's *net* APY tracks the partner-defined target.
It mirrors the cycle the DeFindex API exposes at `POST /vault-ops/stabilize/cron`
but runs in-process from this repository — no Google Cloud Scheduler, no API
dependency at tick time.

The bot lives at `src/stabilizer/` and is meant to be supervised by PM2.

---

## What it actually does

Every tick:

1. **Discover** every vault whose on-chain `manager` role points at the
   FeeProxy contract.
2. For each vault:
   - **Read** the per-vault configuration from FeeProxy: `target_apy_bps`,
     `min_fee_bps`, `max_fee_bps`.
   - **Measure** the live gross (pre-fee) APY by reading on-chain
     `fetch_total_managed_funds`, `total_supply`, `report` and computing the
     gross PPS, then comparing against the nearest indexer snapshot 24h
     before "now". Returns a decimal (e.g. `0.0977` = 9.77%).
   - **Decide** the required fee:
     - If live APY ≤ target → required = `min_fee_bps` (strategy can't even
       reach target, take only the minimum cut).
     - Otherwise → `required = round((1 − target / gross) × 10_000)` bps,
       clamped into `[min_fee_bps, max_fee_bps]`.
   - **Gate** on a 50 bps dead zone — if `|required − current| ≤ 50`, skip
     to avoid churn.
   - **Rate-limit** to ±100 bps per cycle (`MAX_FEE_DELTA_BPS_PER_CYCLE`),
     so one bad APY reading can't swing a vault's fee dramatically.
   - **Submit** `lock_fees(fee_manager, vault, applied_fee_bps)` to the
     FeeProxy — or log the intent if `VAULT_OPS_DRY_RUN=true`.
3. **Log** a per-tick summary (`processed=N adjusted=M skipped=K errors=E`)
   and sleep until the next tick.

Per-vault errors (RPC failure, simulation revert, DB hiccup) are caught and
recorded as `action: "error"` — they do not abort the cycle.

---

## Prerequisites

- Node 20+ (the bot uses Node's built-in `node:test` runner for unit tests)
- [pnpm](https://pnpm.io/installation) (this repo's TypeScript side uses pnpm)
- [PM2](https://pm2.keymetrics.io/) for production supervision: `npm i -g pm2`
- **Read-only Postgres role** on the DeFindex indexer database
- **Stellar mainnet RPC** endpoint (public is fine for low-volume; private
  recommended for production)
- The **fee_manager Stellar keypair** for the deployed FeeProxy contract
  (whichever address is currently registered as `fee_manager`)

---

## Quickstart

```bash
# 1. Install deps (from the repo root)
pnpm install

# 2. Configure environment
cp .env.example .env
# Edit .env — set INDEXER_DATABASE_URL, FEE_MANAGER_SECRET_KEY, SOROBAN_RPC

# 3. Run unit tests (no env or network needed)
pnpm test:stabilizer
# Expected: tests 16, pass 16, fail 0

# 4. First run — foreground, dry-run, fast interval
STABILIZER_INTERVAL_MS=60000 pnpm stabilizer
# You'll see the banner, preflight, a tick within a few seconds, and the
# per-vault analysis. With VAULT_OPS_DRY_RUN=true (the default) it does
# NOT submit any on-chain transaction. Ctrl-C to stop.
```

---

## Configuration

All configuration is via environment variables. See `.env.example` for the
template; the table below documents semantics.

| Variable                     | Required | Default                                | Purpose                                                                                                   |
|------------------------------|----------|----------------------------------------|-----------------------------------------------------------------------------------------------------------|
| `SOROBAN_RPC`                | yes      | —                                      | Mainnet Soroban RPC URL.                                                                                  |
| `INDEXER_DATABASE_URL`       | yes      | —                                      | Postgres connection string to the DeFindex indexer (`parsed.*` schema). Use a read-only role.             |
| `INDEXER_DATABASE_SSL`       | no       | `true` (with `rejectUnauthorized=false`) | Set to `false` only for a local dev Postgres without SSL.                                                 |
| `FEE_MANAGER_SECRET_KEY`     | yes      | —                                      | Stellar secret of the FeeProxy's registered `fee_manager`. Required to sign `lock_fees`.                  |
| `FEE_PROXY_ADDRESS_MAINNET`  | no       | hardcoded in `constants.ts`            | Override only if the FeeProxy is redeployed at a new address.                                             |
| `VAULT_OPS_DRY_RUN`          | no       | `true`                                 | When `true`, computes everything but skips the on-chain submit. Flip to `false` only after validation.    |
| `STABILIZER_INTERVAL_MS`     | no       | `3600000` (1h)                         | Sleep between ticks. Defaults to hourly. Tighten for testing.                                             |

**Internal constants** (in `src/stabilizer/constants.ts`, not env-driven —
change requires a code edit + restart):

- `DEAD_ZONE_BPS = 50` — minimum fee change before adjusting
- `MAX_FEE_DELTA_BPS_PER_CYCLE = 100` — fee change cap per tick
- `CONTROLLER_APY_WINDOW_DAYS = 1` — APY lookback window
- `SNAPSHOT_SEARCH_WINDOW_DAYS = 30` — how far back to look for the start
  snapshot if the 24h-ago slot has no indexer record

---

## Running

### Foreground (for first validation)

```bash
pnpm stabilizer
```

Use this mode when you change `.env`, when you're cross-checking the bot's
math against the API, or when something is wrong and you want to see logs
on stdout. Ctrl-C stops the loop cleanly.

### PM2 (for production)

```bash
# Start
pm2 start npm --name apy-stabilizer -- run stabilizer

# Logs (tail)
pm2 logs apy-stabilizer

# Persist across reboots
pm2 save

# Restart after .env changes
pm2 restart apy-stabilizer --update-env

# Stop
pm2 stop apy-stabilizer
```

PM2 captures stdout/stderr and restarts the process if it exits. The bot's
loop catches per-tick failures internally; only an uncaught startup error
(e.g., missing required env) will exit the process and trigger a PM2 restart.

---

## Workflow — dry-run → live

This is the recommended path from "freshly cloned" to "submitting on-chain
transactions":

**Phase 1: tests pass on your machine.**
```bash
pnpm test:stabilizer
```
16 unit tests on the pure math. No network, no DB, no env required.

**Phase 2: dry-run with a tight interval.**
```bash
# .env: VAULT_OPS_DRY_RUN=true   STABILIZER_INTERVAL_MS=60000
pnpm stabilizer
```
Each tick should:
- log `discovered N managed vault(s)` matching your expectation
- log a per-vault line for every managed vault (no `skipped_no_data` for
  active vaults)
- show `appliedFeeBps` values that match the comparable response from the
  API's `GET /vault-ops/stabilize/status?network=mainnet` per vault
- log `errors=0`

Iterate here. If the bot's numbers diverge from the API's status response
by more than a few bps, stop and root-cause — likely an indexer connection
issue or stale snapshot.

**Phase 3: dry-run at production cadence.**
```bash
# .env: VAULT_OPS_DRY_RUN=true   STABILIZER_INTERVAL_MS=3600000
pm2 start npm --name apy-stabilizer -- run stabilizer
pm2 save
```
Watch for an hour or two. Confirm log shape is what you expect.

**Phase 4: flip to live.**
```bash
# .env: VAULT_OPS_DRY_RUN=false
pm2 restart apy-stabilizer --update-env
pm2 logs apy-stabilizer
```
The next vault that needs adjusting will log a real Soroban transaction
hash instead of `tx=DRY_RUN`. Look that hash up on
[stellar.expert](https://stellar.expert) and confirm the `lock_fees`
invocation succeeded with the expected `new_fee_bps`.

If something looks wrong, set `VAULT_OPS_DRY_RUN=true` and
`pm2 restart apy-stabilizer --update-env` immediately. The bot will go
read-only on the next tick.

---

## Monitoring

Every tick produces structured stdout suitable for grep / log aggregation.
The format:

```
[2026-05-28T14:00:00.123Z] === stabilization tick (mainnet · proxy=CDEFL… · dryRun=false) ===
[2026-05-28T14:00:01.456Z] discovered 4 managed vault(s)
[2026-05-28T14:00:05.789Z] ✓ CD7T34Y5… current=1200 required=1450 applied=1300 (rate-limited) tx=fe8a…
[2026-05-28T14:00:09.234Z] ↺ CAEPJIHE… skipped_dead_zone (current=2000 required=2030 dead=50)
[2026-05-28T14:00:13.567Z] ↺ CB5YXWID… skipped_no_data (grossApy=null)
[2026-05-28T14:00:17.890Z] ✗ CD3HR7WN… error: simulate CD3HR7WN.get_vault_config failed: timeout
[2026-05-28T14:00:18.012Z] === tick done · processed=4 adjusted=1 skipped=2 errors=1 ===
[2026-05-28T14:00:18.013Z] sleeping 3600000ms until next tick
```

**What to watch for:**

| Signal                                                       | Meaning                                                                                          |
|--------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `errors=0` every tick                                        | Healthy.                                                                                         |
| `adjusted` count trending toward zero                        | Fees are converging on their targets.                                                            |
| `clampedByRateLimit=true` on many vaults                     | APY moving faster than 100 bps/cycle can correct — fees catching up over multiple ticks.         |
| Same vault errors repeatedly                                 | Investigate the message — likely RPC issue, contract auth, or a vault state the bot can't read.  |
| `discovered 0 managed vault(s)`                              | DB connection is up but no vault has `manager == FEE_PROXY_ADDRESS`. Check FeeProxy registration. |
| `indexer pg pool idle-client error: …` lines                 | DB connectivity blip. Bot survives. If frequent, check the network path to Postgres.             |

---

## Troubleshooting

**`SOROBAN_RPC is not set` / `INDEXER_DATABASE_URL is not set` / `FEE_MANAGER_SECRET_KEY is not set`**
The bot's `preflight()` failed. The env var isn't reaching the process —
common when launching via PM2 without `--update-env`, or when the `.env`
isn't being loaded. Run `printenv | grep SOROBAN_RPC` from the same shell
the bot launches in.

**`simulate <addr>.get_vault_config failed: …MissingValue…`**
The vault isn't registered with the FeeProxy. Either the indexer query
returned a vault that's no longer managed by the proxy (stale
`vault_role_change` data), or the FeeProxy address in the bot's config
doesn't match what's deployed.

**`simulate router.exec failed`**
The DeFindex Stellar router (`STELLAR_ROUTER` in `constants.ts`) doesn't
match the deployed router, or one of the vault's `REPORT` / `TOTAL_SUPPLY`
/ `FETCH_TOTAL_MANAGED_FUNDS` invocations is reverting. Confirm the router
address; if correct, simulate the same vault's `report` call directly with
the Stellar CLI for the actual revert reason.

**`tx … not successful (status=FAILED)` after going live**
The transaction landed on chain but reverted. Inspect on stellar.expert.
Common causes:
- `Auth` — your `FEE_MANAGER_SECRET_KEY` doesn't match the address
  currently registered as `fee_manager` on FeeProxy. Use FeeProxy admin's
  `set_fee_manager` to rotate.
- A contract-level constraint the bot didn't anticipate — file an issue
  with the tx hash and the bot's log line.

**`indexer pg pool idle-client error: …` recurring**
Network path to the indexer Postgres is unstable. The bot will keep working
(new connections are created lazily on the next query), but the underlying
issue is worth fixing — managed Postgres usually exposes a TCP keepalive
setting that helps.

**Tests fail with a stellar-sdk type error**
Run `pnpm test:stabilizer`, not `tsc`. The repo uses `tsx` at runtime and
does not depend on a clean `tsc --noEmit` run (the Stellar SDK ships
transitive type declarations that aren't always resolvable; the runtime
behavior is correct). If the unit tests pass, the code is fine.

---

## Project layout

```
src/stabilizer/
├── cron.ts              PM2 entry; preflight, banner, hourly while-loop
├── fee-stabilizer.ts    Pure math + runStabilizationCycle + processVault
├── proxy-contract.ts    FeeProxy reads (get_vault_config) and writes (lock_fees)
├── apy-calculation.ts   Live gross APY (live REPORT+TMF+SUPPLY + indexer start snapshot)
├── indexer-db.ts        Read-only pg.Pool, three SQL queries against parsed.*
├── stellar-rpc.ts       Lazy rpc.Server wrapper + simulate / multi-invoke / sign+submit
├── types.ts
├── constants.ts
└── __tests__/
    └── fee-stabilizer.test.ts   16 node:test cases on pure math
```

Design notes for contributors:

- **One file per concern.** Network and DB clients are lazy-initialized so
  the math is testable in isolation (no module-level side effects requiring
  env).
- **Per-vault errors are isolated.** A failing vault produces an `error`
  result and the cycle continues. Don't introduce throws that escape the
  per-vault `try/catch` in `processVault`.
- **The pure math is the unit-tested core.** `calculateRequiredFee`,
  `shouldAdjust`, `applyRateLimit` are exported from `fee-stabilizer.ts`
  and covered by the test suite. New control-loop tweaks should ship with
  new test cases.
- **Logging goes through `src/helpers/log.ts`** so every line gets a UTC
  timestamp and the format stays consistent across PM2 services in this
  repo (`apy-poller`, `cron-deposit-withdraw`, `stabilizer`).

---

## Operational notes

- The bot holds the `fee_manager` key in memory for the lifetime of the
  process. Keep that key isolated to the host that runs the bot, fund it
  with only enough XLM to pay transaction fees, and rotate via FeeProxy's
  `set_fee_manager` whenever the host changes.
- The indexer connection should be **read-only** at the database role
  level — the bot only `SELECT`s, but defense-in-depth matters.
- The hardcoded mainnet FeeProxy address in `constants.ts` is the source
  of truth in this repo. If the FeeProxy is redeployed, update that
  constant in a release commit (don't rely on the env override for
  long-lived production deployments).
- Testnet is intentionally not supported in this iteration —
  `FEE_PROXY_ADDRESS.testnet` is a placeholder and is dead code until a
  testnet proxy is deployed.
