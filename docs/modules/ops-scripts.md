# Ops Scripts Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/bash/`, `src/fee-proxy/bash/`, `src/boost-treasury/bash/` · **Last verified:** 2026-09-04

## Purpose

Interactive Bash wrappers around the Stellar CLI that deploy and operate both contracts. Each script prompts for anything not passed positionally, checks the signer's XLM balance, prints a confirmation summary, and only then submits. Two of them build unsigned XDR instead of submitting, so a multisig account can sign offline. These are the sanctioned path for day-2 admin operations on mainnet.

## Structure

| Script | Contract call | Signer must be |
|---|---|---|
| `src/fee-proxy/bash/deploy.sh` | `stellar contract deploy` + `__constructor(admin, fee_manager)` (`:234`) | deployer identity; admin too, unless the admin signs separately |
| `src/fee-proxy/bash/register_vault.sh` | `register_vault --vault --config` (`:249`) | the vault's current Manager, and `config.admin` |
| `src/fee-proxy/bash/unregister_vault.sh` | `unregister_vault` (`:221`) | the vault's registered `config.admin` |
| `src/fee-proxy/bash/set_target_apy.sh` | `set_target_apy` (`:249`) | the vault's registered `config.admin` |
| `src/boost-treasury/bash/deploy.sh` | `stellar contract deploy` + `__constructor(admin, manager)` (`:223`) | deployer identity; admin too |
| `src/boost-treasury/bash/register_campaign.sh` | `register_campaign --vault --asset` (`:251`) | BoostTreasury admin |
| `src/boost-treasury/bash/update_campaign.sh` | `update_campaign --vault --active` (`:219`) | BoostTreasury admin |
| `src/boost-treasury/bash/unregister_campaign.sh` | `unregister_campaign` (`:248`) | BoostTreasury admin |
| `src/boost-treasury/bash/deposit.sh` | `deposit` (`:226`) | the depositor (anyone with balance) |
| `src/boost-treasury/bash/deposit-xdr.sh` | `deposit`, `--build-only` (`:235`) | nobody at build time; signed offline |
| `src/boost-treasury/bash/boost.sh` | `boost` (`:223`) | BoostTreasury manager |
| `src/boost-treasury/bash/transfer.sh` | `transfer` (`:236`) | BoostTreasury admin |
| `src/boost-treasury/bash/rescue_orphan.sh` | `rescue_orphan` (`:244`) | BoostTreasury admin |
| `src/bash/invoke-xdr.sh` | any function on either contract, `--build-only` (`:238`) | nobody at build time; signed offline |

## Public surface

Every script accepts the same two calling conventions, documented in its own header comment: fully interactive (`./script.sh`) or positional (`./script.sh <network> <source_account> ...`). A `-` in a positional slot means "use the default"; a missing trailing arg is prompted for. `resolve_with_default` and `resolve_required` (`src/fee-proxy/bash/deploy.sh:75`, `:86`) implement that convention and are copy-pasted into every script.

## Key methods

- **`resolve_contract_id`** (`src/bash/invoke-xdr.sh:165`) — accepts either a literal `C...` address or a key from `<workspace_root>/<network>.contracts.json`, which is how every non-deploy script finds the deployed contract. Requires `jq`.
- **`ensure_deployer` / `ensure_source`** (`src/fee-proxy/bash/deploy.sh:135`, `src/bash/invoke-xdr.sh:138`) — resolves the identity, then reads its XLM balance through the network's XLM SAC with `--send no` and refuses to continue on a zero balance.
- **`ensure_wasm`** (`src/fee-proxy/bash/deploy.sh:169`) — builds the package with `stellar contract build --package fee-proxy` if the artifact is missing, then re-checks the path.
- **`run_with_spinner`** (`src/fee-proxy/bash/deploy.sh:98`) — captures stdout and stderr to a temp file and renders the spinner on stderr, so it is safe inside `$(...)`.
- **`fetch_vault_asset_default`** (`src/boost-treasury/bash/register_campaign.sh:181`) — calls the vault's `get_assets()` read-only to prefill the `--asset` argument, and bails out with a warning if the vault reports anything other than exactly one asset.
- **The XDR pipeline** in `invoke-xdr.sh` — `contract invoke --build-only` (`:238`), then `tx simulate` to attach the footprint and resource fee (`:248`), then a `tx decode | jq | tx encode` round-trip that stamps a `max_time` bound (`:259`), then `tx hash`. The simulation step doubles as a check that the call would actually succeed.

## Dependencies

- **Stellar CLI** on `PATH` (`README.md:37` asks for 26.0 or newer), plus `jq` for anything that reads the contracts file (`src/bash/invoke-xdr.sh:173`).
- **`<network>.contracts.json` at the repo root.** Only `mainnet.contracts.json` exists; there is no `testnet.contracts.json`, so testnet runs must pass literal `C...` addresses.
- **Rust toolchain with the `wasm32v1-none` target**, for the deploy scripts' build fallback.
- Network endpoints are hardcoded per script: mainnet RPC `https://rpc.lightsail.network`, testnet RPC `https://soroban-testnet.stellar.org`, with the matching passphrases and XLM SAC ids (`src/fee-proxy/bash/deploy.sh:34`).

## Gotchas and invariants

- **Deploy scripts do not write the deployed address anywhere.** Both print a warning and a `jq` one-liner for you to run by hand (`src/fee-proxy/bash/deploy.sh:248`, `src/boost-treasury/bash/deploy.sh:237`). Forgetting this step leaves every other script pointed at the previous deployment. The stale addresses in `old.mainnet.contracts.bck.json` and `src/stabilizer/constants.ts:9` are what that looks like in practice.
- **The unsigned-XDR scripts bake in the source account's sequence number at build time.** That account must not submit any other transaction before the signed envelope lands, or it fails (`src/bash/invoke-xdr.sh:288`). Both XDR scripts bound validity to one hour (`src/bash/invoke-xdr.sh:54`) so a forgotten envelope eventually dies.
- **`invoke-xdr.sh` only works when the authorized address *is* the transaction source.** Its header (`src/bash/invoke-xdr.sh:8`) spells out the constraint: it relies on envelope signatures satisfying `require_auth()`, which holds for admin-gated functions and `accept_admin`, but not for a function that authorizes some third address.
- **Inclusion fee is deliberately 0.1 XLM, not the CLI default.** `DEFAULT_INCLUSION_FEE=1000000` stroops (`src/fee-proxy/bash/deploy.sh:52`), because the CLI's default of 100 fails with `txINSUFFICIENT_FEE` during mainnet fee surges. Override with `INCLUSION_FEE=<stroops> ./script.sh`.
- **`register_vault.sh` refuses `config.admin != signer`** (`src/fee-proxy/bash/register_vault.sh:225`) even though the contract does not require it. This is a guard against the confusing double-auth failure you would otherwise get, since the nested `vault.set_manager` needs the current Manager's signature.
- **All amounts are raw token units, not decimals.** Every script that takes an `<amount>` says so in its header (for example `src/boost-treasury/bash/boost.sh:12`). There is no decimal conversion anywhere.
- **The helper block is duplicated across all fourteen scripts.** `die`, `prompt_required`, `prompt_default`, `resolve_with_default`, `resolve_required`, `run_with_spinner`, `ensure_network` and the network constants are copy-pasted. A fix to one is not a fix to the others; grep before assuming.
- **`set -euo pipefail` is on everywhere** (`src/fee-proxy/bash/deploy.sh:12`). An unset variable is a hard failure, which is why the arg resolvers use the `${1-__UNSET__}` sentinel rather than plain `${1:-}`.

## Testing

No automated tests. These scripts are validated by running them, and each one gates its submit behind a `[y/N]` confirmation prompt after printing a full summary of what it will do. For a dry check of an invocation without submitting, use `src/bash/invoke-xdr.sh`: its simulate step (`:248`) verifies the call would succeed and it never sends.
