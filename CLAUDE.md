# stellar-apy-stabilizer

## Entry summary

Soroban contracts plus an off-chain bot that let a DeFindex vault partner opt into a managed fee policy with an optional yield-boost campaign.
**Exposes two mainnet contracts.** `FeeProxy` (`mainnet.contracts.json:2`) holds a vault's Manager role and gates it: `register_vault`, `lock_fees`, `distribute_fees`, `release_fees`, `set_target_apy`, `set_fee_bounds`, vault passthroughs, two-step admin. Errors 3000–3099 (`contracts/fee-proxy/src/error.rs:6`).
`BoostTreasury` (`mainnet.contracts.json:3`) escrows per-vault boost budgets: `register_campaign`, `deposit`, `boost`, `transfer`, `reallocate`, `rescue_orphan`. Errors 4000–4099 (`contracts/boost-treasury/src/error.rs:6`).
Both emit typed events other services can index (`contracts/*/src/events.rs`).
**Consumes** DeFindex vault contracts (Manager interface, `contracts/fee-proxy/src/lib.rs:44`), the DeFindex indexer Postgres `parsed.*` schema read-only (`src/stabilizer/indexer-db.ts:81`), the DeFindex Stellar router for batched simulation (`src/stabilizer/constants.ts:16`), and the DeFindex API `/vault/:id/apy` (`src/poller/apy-history.ts:31`).
No HTTP server. Everything here is a contract, a long-running process, or a CLI script.

## Stack

- Rust + `soroban-sdk` 25.3.1, Cargo workspace over `contracts/*` (`Cargo.toml:2`). Release profile tuned for WASM size (`Cargo.toml:8`).
- TypeScript on Node, run through `tsx` (`package.json:24`), package manager pnpm (`pnpm-workspace.yaml:1`, `docs/STABILIZER.md:48`). Deps: `@stellar/stellar-sdk`, `pg`, `dotenv` (`package.json:16`).
- Bash + Stellar CLI for deploys and admin operations.

## Layout

| Path | What |
|---|---|
| `contracts/fee-proxy/`, `contracts/boost-treasury/` | The two Soroban contracts. |
| `src/stabilizer/` | The fee-control bot (PM2 process). |
| `src/poller/`, `src/vault/`, `src/backtest/` | APY history poller, deposit/withdraw cron, offline threshold backtest. |
| `src/helpers/`, `src/constants/` | Shared logging, DB, Stellar tx plumbing, vault address lists. |
| `src/bash/`, `src/*/bash/` | Stellar CLI operator scripts. |
| `external-contracts/` | `defindex_vault.optimized.wasm`, imported by the integration tests. |
| `docs/` | `STABILIZER.md` runbook, design proposal, audits, and `docs/modules/`. |

## Build, test, deploy

```bash
rustup target add wasm32v1-none && cargo install --locked stellar-cli   # README.md:40
stellar contract build                                                  # both contracts, README.md:49
cargo test                                                              # 40 fee-proxy + 51 boost-treasury tests
cargo test -p fee-proxy integration_tests                               # real vault WASM, README.md:74

pnpm install                                                            # docs/STABILIZER.md:62
pnpm test:stabilizer                                                    # 16 node:test cases, package.json:14
pnpm stabilizer                                                         # foreground bot, package.json:13
pm2 start npm --name apy-stabilizer -- run stabilizer                   # docs/STABILIZER.md:123
```

Contract deploys go through `src/fee-proxy/bash/deploy.sh` and `src/boost-treasury/bash/deploy.sh`. Both constructors call `admin.require_auth()`, so the admin must sign the deploy (`contracts/fee-proxy/src/lib.rs:54`). Neither script writes the deployed address to `mainnet.contracts.json`; do it by hand (`src/fee-proxy/bash/deploy.sh:248`).

Other scripts: `pnpm deposit`, `pnpm withdraw`, `pnpm cron`, `pnpm apy-poller`, `pnpm backtest:regime` (`package.json:8`).

## Key entry points

- `contracts/fee-proxy/src/lib.rs:51` and `contracts/boost-treasury/src/lib.rs:47` — the `#[contractimpl]` blocks.
- `src/stabilizer/cron.ts:27` — bot main loop. `src/stabilizer/fee-stabilizer.ts:59` — one cycle.
- `src/bash/invoke-xdr.sh` — build an unsigned invocation of any function on either contract for offline multisig signing.

## Gotchas

- **The bot's default FeeProxy address is stale.** `src/stabilizer/constants.ts:9` and `monitor.sql:32` use `CDEFLWJ…`, which is `old.mainnet.contracts.bck.json:2`. `mainnet.contracts.json:2` holds `CBXBQ7SI…`. Set `FEE_PROXY_ADDRESS_MAINNET` or reconcile before running live.
- **`VAULT_OPS_DRY_RUN` defaults to on** and only the literal string `"false"` turns it off (`src/stabilizer/constants.ts:40`). A dry run still logs `adjusted`, with `tx=DRY_RUN`.
- **`.env.example` covers only the stabilizer.** `DEFINDEX_API_URL`, `DEFINDEX_API_KEY`, `DATABASE_URL` and `STELLAR_SECRET_KEY` are used in `src/` but undocumented there. Never commit real values.
- **`README.md` has drifted from the contracts.** Signatures for `register_vault` and `register_campaign` changed, and `reallocate`, `rescue_orphan` and `FeesReleased` are missing from its lists. Read the source, or the module docs below.
- **Testnet is not wired up.** `FEE_PROXY_ADDRESS.testnet` is a placeholder (`src/stabilizer/constants.ts:10`) and there is no `testnet.contracts.json`.
- **Do not relax the fail-closed paths.** `AccountingCorrupted` on the `Tracked` decrements (`contracts/boost-treasury/src/lib.rs:270`) and `available()`'s clamp to 0 (`contracts/boost-treasury/src/storage.rs:41`) are deliberate.

## Cross-repo dependencies

- **-> DeFindex vault contract** (external, audited separately): FeeProxy takes its Manager role and calls `set_manager`, `lock_fees`, `distribute_fees`, `release_fees`, `upgrade`, `set_fee_receiver`, `set_emergency_manager`, `set_rebalance_manager`, `rescue`, `pause_strategy`, `unpause_strategy` — `contracts/fee-proxy/src/lib.rs:167`, `:220`, `:244`, `:259`, `:316`, `:324`, `:342`, `:348`, `:354`, `:361`, `:368`, `:375`. BoostTreasury calls `get_assets()` — `contracts/boost-treasury/src/lib.rs:144`. The bot reads `fetch_total_managed_funds`, `total_supply`, `report` — `src/stabilizer/constants.ts:66`. The cron calls `deposit`/`withdraw` — `src/helpers/vault.ts:24`, `:35`. Vault bytecode is vendored for tests at `external-contracts/defindex_vault.optimized.wasm` (`contracts/fee-proxy/src/test.rs:571`).
- **-> defindex-indexer** (Postgres, read-only): queries `parsed.v_vault_apy`, `parsed.vault_role_change`, `parsed.vault_transaction`, `parsed.vault_transaction_asset`, `parsed.vault_strategy_report` — `src/stabilizer/indexer-db.ts:83`, `:86`, `:131`, `:132`, `:176`; `monitor.sql:10` names the database.
- **-> defindex-api**: the poller calls `GET {DEFINDEX_API_URL}/vault/:id/apy?network=…` with a bearer key — `src/poller/apy-history.ts:31`. The router constant is copied from `defindex-api/src/helpers/constants.ts` per `src/stabilizer/constants.ts:14`. `docs/STABILIZER.md:5` positions the bot as an in-repo mirror of the API's `POST /vault-ops/stabilize/cron`.
- **-> DeFindex Stellar router contract** `CDAW42JDSDEI2DXEPP4E7OAYNCRUA4LGCZHXCJ4BV5WVI4O4P77FO4UV` mainnet / `CAG7OQAN4YO65ZLOYA5PWJKPYYE5BVH7QSRI4KAW7VBMIH6N6LG5ECSL` testnet: `exec` for batched read simulation — `src/stabilizer/constants.ts:15`.
- **-> paltalabs/defindex** (GitHub): the design proposal tracks issue `paltalabs/defindex#841` — `docs/APY_STABILIZER_PROPOSAL.md:4`.
- **<- an indexer consumes this repo's contract events** — `README.md:8`. Which indexer is not named in the source: _TBD, unverified_.
- No `@defindex/*` or `@soroswap/*` npm package is depended on (`package.json:16`). Soroswap appears only as an out-of-scope note in `docs/internal-audit.md:62`.

## Module Documentation Convention (MANDATORY)

Every module has a living doc at `docs/modules/<module>.md` (flat file, one per module). `docs/modules/README.md` is the index that routes a module's source path to its doc. These are the fast on-ramp for anyone, human or agent, touching a module.

**Progressive disclosure — do NOT load all docs at once.** When you are about to touch a module, open `docs/modules/README.md`, find the ONE doc matching the code you are changing, and read only that. Never pull the whole `docs/modules/` folder into context.

**The workflow rule:**
1. **Before modifying a module, read its `docs/modules/<module>.md` first.** It holds the file map, key methods with `file:line`, dependencies, and gotchas.
2. **After modifying a module, update its doc in the same change.** New or removed entrypoints, changed behavior, new gotchas, dependency changes — all go into the doc before the work is done. Bump the "Last verified" date.
3. Doc claims must be verified against source and cite `file:line`. Never document something you have not confirmed exists.
4. **Adding a new module?** Create its `docs/modules/<module>.md` and add a row to `docs/modules/README.md` in the same change.

Docs follow a shared template: Purpose, Structure, Endpoints/Public surface, Key methods (`file:line`), Dependencies, Gotchas and invariants, Testing.
