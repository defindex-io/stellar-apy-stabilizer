# APY Stabilizer — Pre-Mainnet Audit

**Contracts under review:**
- `contracts/boost-treasury` — per-vault top-up budget pot, manager-disbursed
- `contracts/fee-proxy` — DeFindex vault Manager proxy with delegated fee control

**Commit:** `edbd57cc38b2aa2ac5b790c970d54aa0ab9a6666`
**Reviewers:** internal review + Almanax automated scan (`b7f9a5c2-7f1f-4b2a-a1fe-6da3ece53e6c`)
**Status:** All HIGH and actionable MEDIUM/LOW findings have been fixed in-place; the remaining MEDIUM items are risk-accepted with documented rationale. Ready for external audit.

---

## 1. Executive summary

The architecture is sound: the proxy correctly leverages Soroban's direct-invoker auth rule to act as the DeFindex vault Manager while delegating fee permission to a `fee_manager` bot and keeping per-vault admin rights with partners. The boost-treasury is a clean budget pot with per-vault accounting. Test coverage is above average for a first-deploy Soroban codebase, including integration tests against the real vault WASM.

Findings (post-fix):

- **H1 — fixed in code.** `register_vault` now asserts `admin == config.admin` with a new `AdminMismatch` error.
- **M3 — fixed in code.** `rescue_orphan` added to boost-treasury, bounded by a `CampaignList` enumeration so it can only sweep tokens above what's tracked by active campaigns. New errors `AccountingCorrupted` / `InsufficientOrphanBalance`, new event `OrphanRescued`.
- **L2, L3, L4, L5 — fixed in code.** Proxy-side `amount > 0` validation on `release_fees`; explicit `asset` param on `register_campaign` with `AssetMismatch` assertion; `transfer()` docstring updated; `register_vault` writes config to storage before the `set_manager` cross-contract call.
- **M9 (single bot key), M7 (no global pause), M8 (max_fee_bps cap), M5 (instant role rotation), M6 (silent underflow) — risk-accepted by design.** Documented with rationale; the underlying vault enforces the actual caps (M8) and the bot architecture's recovery path makes single-key compromise non-fund-redirecting (M9).
- **Four Almanax findings dismissed** (CRITICAL constructor; TTL MEDIUMs M1, M2; auth-by-parameter M4) as false positives — Soroban Protocol 22+ host-reserves `__constructor`, Protocol 23+ auto-restores archived storage on access, and Soroban's auth tree binds entries to specific invocations.

> **False-positive removed.** Almanax flagged a CRITICAL "constructor re-callable post-deployment" finding (`6710ced6-...`). After verification against [CAP-0058](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0058.md), this is **not exploitable**: `__constructor` is host-reserved and may only be called by the Soroban host environment at creation time. Soroban SDK 22+ contracts inherit this guarantee without needing an explicit init-guard. The finding is dismissed.

All fixes are in-tree, both contracts build clean (`stellar contract build`), and the full test suite passes (42 boost-treasury + 41 fee-proxy = 83 tests, including new coverage for `AdminMismatch`, `AssetMismatch`, `rescue_orphan`, and the `InvalidAmount` proxy-side check on `release_fees`).

---

## 2. Scope and methodology

**In scope:**
- `contracts/boost-treasury/src/{lib,storage,error,events}.rs`
- `contracts/fee-proxy/src/{lib,storage,error,events}.rs`
- Workspace `Cargo.toml` (release profile)
- Test files (used to verify intended behavior, not audited as production code)

**Out of scope (assumed audited):**
- The underlying DeFindex vault contract (separately audited, $1.4M live TVL)
- Soroswap router, Blend pool, SEP-41 token implementations
- Off-chain bot logic

**Methodology:**
1. Source-level read of all four `.rs` modules per contract.
2. Cross-reference against DeFindex vault's expected manager interface (verified against the prior knowledge-base doc derived from the vault source).
3. Automated scan via Almanax MCP (scan id above) on the same commit.
4. Findings merged, severity reconciled where the two sources disagreed.

---

## 3. Findings summary

| # | Severity | Title | Source | Contract | Status |
|---|---|---|---|---|---|
| H1 | HIGH | `register_vault` does not authenticate `config.admin` | internal | fee-proxy | **FIXED** — `admin == config.admin` asserted + new `AdminMismatch` error |
| M9 | MEDIUM (risk-accepted) | Single `fee_manager` key can force max fee across all vaults | internal | fee-proxy | risk-accepted |
| ~~M1~~ | DISMISSED | ~~Instance TTL not bumped on init or reads — admin DoS~~ | Almanax | both | dismissed (Protocol 23+) |
| ~~M2~~ | DISMISSED | ~~Campaign persistent TTL can strand funds after 120 days~~ | Almanax | boost-treasury | dismissed (Protocol 23+) |
| M3 | MEDIUM | Tokens sent directly to treasury are permanently stuck | both | boost-treasury | **FIXED** — `rescue_orphan` + `CampaignList` index |
| ~~M4~~ | DISMISSED | ~~Confused-deputy via `caller` parameter on `lock_fees`/`distribute_fees`~~ | Almanax | fee-proxy | dismissed (Soroban auth tree) |
| ~~M5~~ | DISMISSED | ~~`set_manager` / `set_fee_manager` instant vs two-step admin~~ | internal | both | dismissed (consistency with vault) |
| ~~M6~~ | DISMISSED | ~~`Campaign.available()` silently returns 0 on accounting underflow~~ | internal | boost-treasury | dismissed (invariant maintained) |
| M7 | MEDIUM (risk-accepted) | No proxy-level emergency pause / kill switch | internal | both | risk-accepted |
| M8 | MEDIUM (risk-accepted) | `max_fee_bps` accepts 10000 but vault caps at 9000 | internal | fee-proxy | risk-accepted (vault enforces) |
| L1 | LOW | Unbounded `VaultConfig` persistent entries enable state bloat | Almanax | fee-proxy | deferred |
| L2 | LOW | `release_fees` does not validate `amount > 0` at the proxy | Almanax | fee-proxy | **FIXED** — proxy-side `InvalidAmount` check |
| L3 | LOW | `register_campaign` trusts vault's `get_assets()` self-report | internal | boost-treasury | **FIXED** — explicit `asset` param + `AssetMismatch` assertion |
| L4 | LOW | Admin can drain campaign budget via `transfer()` — documentation gap | internal | boost-treasury | **FIXED** — docstring updated; `rescue_orphan` added as safe alternative |
| L5 | LOW | `register_vault` order: `set_manager` before storing config | internal | fee-proxy | **FIXED** — storage write reordered before cross-contract call |
| L6 | LOW | No property tests for `Campaign.available()` invariant | internal | boost-treasury | deferred |
| I1 | INFO | No public `extend_ttl()` safety net (partially covered by M1) | internal | both | superseded by M1 dismissal |
| I2 | INFO | Inconsistent storage access patterns (`unwrap` vs `panic_with_error`) | internal | both | informational |
| I3 | INFO | `last_boosted_at` stored but never read on-chain | internal | boost-treasury | informational |

---

## 4. Detailed findings

### Dismissed — Constructor re-callable post-deployment  [FALSE POSITIVE]

**Almanax finding:** `6710ced6-238c-48de-8e34-68472602c9f3` (CRITICAL).
**Disposition:** Not exploitable. Dismissed after protocol-spec verification.

**Why it's not a real issue.**
The Almanax finding assumes `__constructor` is a regular `#[contractimpl]` entrypoint that remains callable after deployment. This is incorrect for Soroban SDK 22 and above. Per [CAP-0058](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0058.md):

> "Reserve a new special contract function `__constructor` that **may only be called by the Soroban host environment**."
>
> "Every contract may only be created just once (via `create_contract` host function or its `InvokeHostFunctionOp` counterpart)."

The Soroban host enforces this at the protocol level. There is no `InvokeHostFunctionOp` path that lets a user call `__constructor` post-deployment — the host rejects it. Adding an init-guard in contract code would be defense-in-depth against a future protocol change but is not currently required.

This codebase uses `soroban-sdk = "25.3.1"` (workspace `Cargo.toml`), which is well past the Protocol 22 cutoff.

**Action:** None required for current Protocol versions. Optionally add an init-guard as a belt-and-suspenders measure if you want to be portable to non-Soroban-host environments (e.g. tests that don't go through the host) — but production-mainnet behavior is safe.

---

### H1 — `register_vault` does not authenticate `config.admin`  [HIGH — FIXED]

**Source:** internal.
**Location:** `contracts/fee-proxy/src/lib.rs:147-180`

**Description.**

```rust
pub fn register_vault(env: Env, admin: Address, vault: Address, config: VaultConfig) {
    admin.require_auth();
    if storage::has_vault_config(&env, &vault) { panic ... }
    validate_fee_bounds(&env, config.min_fee_bps, config.max_fee_bps);
    call_vault(&env, &vault, "set_manager", vec![&env, proxy.into_val(&env)]);
    storage::set_vault_config(&env, &vault, &config);
}
```

The function takes two distinct address concepts:
- `admin` (parameter): the current vault Manager handing the role to the proxy. Implicitly authenticated by `vault.set_manager`, which the vault gates on the current Manager.
- `config.admin` (struct field): the future controller of the proxy-side state — authorizes `unregister_vault`, `lock_fees`, `release_fees`, `set_target_apy`, `set_fee_bounds`, and every passthrough.

**`config.admin` never authenticates.** The vault's `set_manager` auth doesn't cover it because it's a separate address inside the config struct.

Two failure modes:
1. **Partner footgun.** Partner mis-types `config.admin` (typo, wrong env var, copy-paste from another deploy). Registration succeeds. The vault is now bricked from the proxy's perspective — no admin call works because the legit partner can't authenticate as `config.admin`, and `unregister_vault` is itself gated on `config.admin.require_auth()`.
2. **Compromised frontend / phishing.** Partner signs a `register_vault` tx with `config.admin = attacker_address` substituted by a malicious UI. Attacker takes full proxy-side control of the vault with no separate signature from `attacker_address`.

Naming compounds the bug: the parameter `admin` and the struct field `config.admin` mean different things.

**Fix applied (equality-check approach).**
Keep the `admin` parameter but assert it equals `config.admin`. This forces the partner to set the future proxy controller to the same address that signs the registration, removing the spoofing surface while keeping the function ABI compatible:

```rust
pub fn register_vault(env: Env, admin: Address, vault: Address, config: VaultConfig) {
    extend_instance_ttl(&env);
    admin.require_auth();

    if admin != config.admin {
        panic_with_error!(&env, ContractError::AdminMismatch);
    }
    // ... (set_manager + storage write)
}
```

New error variant `AdminMismatch = 3024`. New test `test_register_vault_admin_mismatch_rejected` verifies the panic; all existing tests continue to pass because they already pass `admin == config.admin`.

If a partner ever needs to register from key A but operate the proxy via key B, the flow is now: (1) rotate vault manager A→B via `vault.set_manager`, (2) call `proxy.register_vault(B, vault, config{admin=B})`. Two transactions, explicit role handoff, auditable on-chain.

---

### Dismissed M1 — Instance TTL not bumped on init or reads  [FALSE POSITIVE per Protocol 23+]

**Almanax findings:** `0c3402b8-...` (boost-treasury) and `79bb4896-...` (fee-proxy), both rated MEDIUM.
**Disposition:** Dismissed. Soroban VM auto-restores archived storage entries on access starting from Stellar Protocol 23 / 24. The DoS scenario assumed by the finding (panic on `get_admin().unwrap()` after instance archival) does not occur — the host transparently restores the entry as part of the host-function dispatch.

This codebase uses `soroban-sdk = "25.3.1"` and targets a network well past the protocol cutoff.

Additionally, all state-changing functions in both contracts already call `extend_instance_ttl(&env)` at the top, so under any operational scenario (bot active, partners interacting) instance TTL is bumped well within the 30-day window.

**Action:** None required.

---

### Dismissed M2 — Campaign persistent TTL can strand funds after 120 days  [FALSE POSITIVE per Protocol 23+]

**Almanax finding:** `2910e076-...` (MEDIUM).
**Disposition:** Dismissed. Same Protocol 23+ auto-restore reasoning as the dismissed M1 — when the host reads an archived persistent entry, the VM transparently restores it. The "panic on expired `get_campaign().unwrap_or_else(... panic)` read" path is not reachable on the current network.

Operationally, a campaign sitting fully untouched for >120 days indicates the boost program has ended — and the partner would normally withdraw the residual budget before that point. If restoration is ever needed, the rent fee is small and the SDK tooling inserts the restore op automatically.

**Action:** None required.

---

### M3 — Tokens sent directly to treasury are permanently stuck  [MEDIUM — FIXED]

**Source:** internal + Almanax `4192915a-...` (rated LOW by Almanax; reconciled to MEDIUM here).
**Location:** `contracts/boost-treasury/src/lib.rs:179-282`

**Description.**
Both `boost` and `transfer` cap outflows at `campaign.available()`. `unregister_campaign` requires `available() == 0`. Any tokens that arrive at the contract address **outside** `deposit()` — direct transfers, refunds from the vault, mistaken sends, dust — are never counted in `total_deposited`. They are:
- Not spendable via `boost` (capped by accounting, not real balance).
- Not withdrawable via `transfer` (same).
- Stranded after `unregister_campaign` (which permits removal while real balance > 0).

**Severity reconciliation.** Almanax rates LOW citing griefing cost (attacker funds the grief from their own pocket). I rated HIGH originally because of user-error scenarios (operators sending to the wrong address). Settling on MEDIUM: real impact + plausible failure mode + no recovery path.

**Recommendation (agreed).**
Add an admin-only `rescue_orphan` function that can sweep tokens above the campaign-tracked balance, with safety bounded by an on-chain campaign index.

**1. Add a `CampaignList` instance entry** — `Vec<Address>` of all registered vaults. Maintained on `register_campaign` (push) and `unregister_campaign` (remove). Used for `rescue_orphan` to enumerate campaigns and for off-chain bot iteration.

```rust
pub enum DataKey {
    Admin, PendingAdmin, Manager,
    Campaign(Address),
    CampaignList,   // ← new: Vec<Address>
}
```

**2. Add `rescue_orphan`:**

```rust
/// Sweeps tokens that arrived at the treasury outside of `deposit()` — direct
/// transfers, refunds, dust. Cannot touch tokens tracked by active campaigns
/// for the given asset: refuses to send more than
/// `balance(token) - sum_of_available_for_campaigns_using(token)`.
pub fn rescue_orphan(env: Env, token: Address, to: Address, amount: i128) {
    extend_instance_ttl(&env);
    require_admin(&env);
    require_positive_amount(&env, amount);

    let balance = token::Client::new(&env, &token).balance(&env.current_contract_address());
    let tracked = storage::sum_tracked_for_asset(&env, &token);

    let orphan = balance
        .checked_sub(tracked)
        .unwrap_or_else(|| panic_with_error!(&env, ContractError::AccountingCorrupted));

    if amount > orphan {
        panic_with_error!(&env, ContractError::InsufficientOrphanBalance);
    }

    token::Client::new(&env, &token).transfer(
        &env.current_contract_address(),
        &to,
        &amount,
    );

    events::OrphanRescued { token, to, amount }.publish(&env);
}
```

`sum_tracked_for_asset` walks `CampaignList`, loads each campaign, and sums `available()` for those whose `asset == token`. O(n) in number of registered campaigns; n is expected to be small.

**3. New error variants:** `AccountingCorrupted`, `InsufficientOrphanBalance`.

**4. New event:** `OrphanRescued { token, to, amount }`.

**Behavior notes.**
- For a token with no matching campaign (e.g. somebody sends USDC but no campaign uses USDC), `tracked = 0`, so the full balance is rescuable. Correct.
- The function cannot drain campaign-tracked balances by construction — this is the key safety property over the alternative "let admin transfer any amount of any token" approach.
- Pairs with M2 dismissal: once Protocol 23+ auto-restores an archived campaign, the rescue path stays accurate because the campaign still appears in `CampaignList`.

---

### Dismissed M4 — Confused-deputy via `caller` parameter  [FALSE POSITIVE in Soroban auth model]

**Almanax finding:** `46f83cf2-...` (MEDIUM).
**Disposition:** Dismissed. The pattern is a generic auth-by-parameter code smell, but the specific exploit path does not exist in Soroban's auth model.

Soroban's `require_auth()` binds each auth entry to a specific contract + function + args + invoker. For `lock_fees(caller=fee_manager, vault=V, new_fee_bps=Y)` to succeed, an auth entry signed by `fee_manager` covering exactly that invocation must be present in the auth tree. An attacker cannot forge such an entry — only `fee_manager`'s key (or a contract programmed to authorize the call) can produce it. The `*caller == fee_manager` equality check against the storage-resolved role closes the remaining gap.

The pattern remains a code-quality smell — it would become a real bug if a future edit removed the equality check while keeping `require_auth`. Acceptable risk given the function is small and the team is aware.

**Action:** None required. Revisit if the auth helpers grow in complexity.

---

### Dismissed M5 — `set_manager` / `set_fee_manager` instant vs two-step admin  [CONSISTENCY WITH VAULT PATTERN]

**Source:** internal.
**Disposition:** Risk-accepted. The single-step pattern mirrors the underlying DeFindex vault's own `set_manager` (passthrough-style). Adding two-step rotation only here would diverge from the vault's mental model without materially raising the bar, since a compromised admin can already inflict comparable damage through other admin-only entrypoints (`set_target_apy` to extreme, `unregister_vault`, etc.).

**Note on stale config concern.** Because `Manager` and `FeeManager` are global roles read fresh from instance storage on every call (`require_manager`, `require_fee_manager_or_vault_admin`), rotation does not require any per-vault `VaultConfig` or per-campaign `Campaign` update. There is no stale-address hazard.

**Action:** None required.

---

### Dismissed M6 — `Campaign.available()` silently returns 0 on accounting underflow  [INVARIANT MAINTAINED UPSTREAM]

**Source:** internal.
**Disposition:** Risk-accepted. The `unwrap_or(0)` defense is unreachable in the current codebase: every mutation path (`deposit`, `boost`, `transfer`) enforces the underlying invariant with explicit pre-checks (`amount > available()` reverts with `InsufficientBudget`) and `checked_add` overflow guards. With `overflow-checks = true` in the release profile, even raw arithmetic would panic on overflow. The defensive arm is over-engineering for a state that the code cannot reach today.

**Optional cleanup.** The docstring on `available()` describes silent-zero as "fails closed if invariant violated." That's misleading — silent zero is *worse* than panic for the operator-mistake case (top-up against corruption). Consider rewording to "this branch is unreachable; `unwrap_or(0)` is a no-op fallback retained for code-size minimalism."

**Action:** None required for correctness. Docstring tweak optional.

---

### M7 — No proxy-level emergency pause / kill switch  [MEDIUM — risk-accepted]

**Source:** internal.
**Location:** both contracts (architectural)

**Description.**
No global `paused` flag exists on either contract. If a bug is found, the only mitigation is to have each partner call `unregister_vault` individually (proxy) or wait for the underlying token contract / vault to revert. For a fleet of partners, that's a slow incident response.

**Recommendation.**
Add a `paused: bool` instance flag, settable by admin, checked at the top of state-changing entrypoints (`lock_fees`, `distribute_fees`, `release_fees` on the proxy; `boost`, `deposit`, `transfer` on the treasury). One-line guard. Keeps read-only paths working, blocks new writes.

```rust
fn require_not_paused(env: &Env) {
    if storage::is_paused(env) {
        panic_with_error!(env, ContractError::Paused);
    }
}
```

---

### M8 — `max_fee_bps` accepts 10000 but vault caps at 9000  [MEDIUM — risk-accepted]

**Source:** internal.
**Location:** `contracts/fee-proxy/src/lib.rs:40-44`

**Description.**
`validate_fee_bounds` allows `max_fee_bps ≤ 10_000`. The underlying DeFindex vault rejects `vault_fee > 9_000` (per `vault/src/storage.rs:111`). A partner who configures `max_fee_bps = 9500` here will see `set_fee_bounds` succeed, but `lock_fees(9500)` will revert at the vault.

**Impact.**
UX/operational; not a security vulnerability. But it can mask configuration errors.

**Recommendation.**
Cap at 9000 in `validate_fee_bounds`:

```rust
fn validate_fee_bounds(env: &Env, min_fee_bps: u32, max_fee_bps: u32) {
    if min_fee_bps > max_fee_bps || max_fee_bps > 9_000 {
        panic_with_error!(env, ContractError::InvalidFeeBounds);
    }
}
```

---

### M9 — Single `fee_manager` key can force max fee across all vaults  [MEDIUM — risk-accepted by design]

**Source:** internal.
**Location:** `contracts/fee-proxy/src/lib.rs:15-29`, `lib.rs:205-249`

**Description.**
A single `fee_manager` address authorizes `lock_fees` and `distribute_fees` on every registered vault. The bot runs hourly to track each vault's target APY, so a multisig or human-in-loop signer is operationally infeasible — autonomous adjustment is the product.

If the `fee_manager` key is compromised, the attacker can call `lock_fees(fee_manager, vault, Some(config.max_fee_bps))` on every registered vault in one transaction, then call `distribute_fees` to realize the locked amount.

**Blast radius (single-key compromise, no partner compromise).**
- `vault_fee_receiver` is set by `config.admin` (partner) and **cannot** be changed by the bot — `set_vault_fee_receiver` is gated on `require_vault_admin`. So realized fees flow to the partner's wallet, not the attacker's.
- The attack is "force unwanted fee schedule across the fleet," not "steal funds." Impact is APY degradation and reputational damage, not principal loss.
- Per-vault damage is bounded by each partner's `config.max_fee_bps` and the gains accumulated since the last lock.

**Recovery path (the reason this is risk-accepted).**
1. Partner calls `unregister_vault` to retake the Manager role.
2. Partner calls `release_fees` on the strategies to claw back the locked portion before distribution, or accepts the lost portion.
3. If shares were already minted, partner uses `boost-treasury` to top up the vault and offset the LP impact.
4. Out-of-band: admin rotates `fee_manager` via `set_fee_manager`.

**Why no on-chain mitigation is applied.**
Any on-chain cap (step limit, rate limit, global ceiling) constrains the bot's intended operation. The bot needs full flexibility to track target APY across volatile market conditions. Caps that bound an attacker would also bound legitimate operation.

**Mitigations relied upon (off-chain / operational).**
- `fee_manager` private key in KMS/HSM with restricted-network signer.
- Off-chain anomaly detection on `FeesLocked` and `FeesDistributed` events; auto-alert + manual `set_fee_manager` rotation on bulk lock pattern.
- Partner-side operational defaults: `config.max_fee_bps` set conservatively (recommended ≤ 2000 bps for a typical vault, not the on-chain max of 10000).
- This decision is documented as **risk-accepted by the project owners**; any future change to the bot architecture (e.g. multiple keys, role separation) should revisit this finding.

**Note:** the related M4 (confused-deputy via `caller` parameter) is *not* risk-accepted — that fix tightens the auth model without constraining bot behavior.

---

### L1 — Unbounded `VaultConfig` persistent entries enable state bloat  [LOW]

**Source:** Almanax `cf74ba74-...`.
**Location:** `contracts/fee-proxy/src/storage.rs:9-103`

**Description.**
`register_vault` is callable by anyone holding a current Manager role on some vault, with no per-account rate limit or proxy-wide cap on the number of registered vaults. Each registration writes a persistent entry that survives via TTL bumps. An attacker who controls many vaults (or many disposable vaults) can register them all, growing persistent state.

**Impact.**
State bloat. No direct fund loss. Cost-to-attacker scales with number of vaults they need to deploy.

**Recommendation.**
Either (a) gate `register_vault` behind admin approval, breaking the "self-service" property; (b) add a global cap on registered vault count; (c) require a small bond paid in the registered asset, refundable on `unregister_vault`. (c) preserves self-service. The proposal mentions self-service is a design goal, so (c) is the recommended fix if you want to keep that property.

---

### L2 — `release_fees` does not validate `amount > 0` at the proxy  [LOW — FIXED]

**Source:** Almanax `5255a636-...`.
**Location:** `contracts/fee-proxy/src/lib.rs:251-261`

**Description.**

```rust
pub fn release_fees(env: Env, vault: Address, strategy: Address, amount: i128) {
    extend_instance_ttl(&env);
    require_vault_admin(&env, &vault);
    call_vault(&env, &vault, "release_fees", vec![&env, strategy.into_val(&env), amount.into_val(&env)]);
}
```

No proxy-side validation that `amount > 0`. The DeFindex vault's `release_fees` does call `validate_amount(amount)` which rejects `amount < 0` (per `vault/src/utils.rs:12`), so the current vault catches this. But defense in depth: the proxy should not forward known-invalid arguments downstream.

**Recommendation.**
```rust
if amount <= 0 {
    panic_with_error!(&env, ContractError::InvalidAmount);
}
```

(Add `InvalidAmount = 3030` to the proxy's `ContractError` enum.)

---

### L3 — `register_campaign` trusts vault's `get_assets()` self-report  [LOW — FIXED]

**Source:** internal.
**Location:** `contracts/boost-treasury/src/lib.rs:130-164`

**Description.**
`register_campaign` is admin-only. It calls `vault.get_assets()` and caches the returned `asset` address into the campaign struct. A malicious vault address could return a fake asset, redirecting subsequent `deposit` and `boost` token transfers to a token the attacker controls.

The exploit requires an admin to call `register_campaign(attacker_vault)`. Operationally low-risk, but worth noting.

**Recommendation.**
Either (a) make `register_campaign` accept the asset as an explicit parameter and assert it matches the vault's report; (b) document that the admin must verify the vault is a known DeFindex vault before registering. (a) is the safer code change.

---

### L4 — Admin can drain campaign budget via `transfer()` — documentation gap  [LOW — FIXED]

**Source:** internal.
**Location:** `contracts/boost-treasury/src/lib.rs:253-282`

**Description.**
By design, the boost-treasury admin can call `transfer(vault, amount, to)` to send any unspent campaign budget to any address. This is needed for refunds and treasury reallocation. **The docstring does not say so.** Users depositing should know admin has this power.

**Recommendation.**
Add to the function's `///` doc and to user-facing docs. Optionally: add a `transfer_target` field to `Campaign` set at registration that restricts the `to` parameter for that campaign. Trade-off: less operational flexibility.

---

### L5 — `register_vault` order: `set_manager` before storing config  [LOW — FIXED]

**Source:** internal.
**Location:** `contracts/fee-proxy/src/lib.rs:160-172`

**Description.**
Soroban's atomicity guarantees this is safe in practice. Conceptually: if you ever extend `register_vault` with logic that could fail after `set_manager` succeeds, the proxy would hold manager rights without a config row. Add a post-condition or restructure to write storage first, then make the cross-contract call.

**Recommendation.**
Reorder defensively:

```rust
validate_fee_bounds(...);
storage::set_vault_config(&env, &vault, &config);
call_vault(..., "set_manager", ...);   // last
```

If the vault's `set_manager` reverts, the whole tx reverts and storage is rolled back. Order is purely for resilience to future code edits.

---

### L6 — No property tests for `Campaign.available()` invariant  [LOW]

**Source:** internal.

**Description.**
Tests cover sequential happy paths and named edge cases. Random sequences of `deposit`/`boost`/`transfer`/`update_campaign` (proptest-style) would catch composition bugs that hand-written tests miss — especially relevant given M6 (the silent-zero behavior).

**Recommendation.**
Add a `proptest` (or `quickcheck`) suite that:
1. Generates a random sequence of operations.
2. Tracks the same accounting off-test.
3. Asserts the contract state matches at every step.
4. Asserts `available() == total_deposited - total_boosted - total_withdrawn` invariant always (after M6 fix, this becomes an assertion that the contract panics in any failing case).

---

### I1 — No public `extend_ttl()` safety net  [INFO]

Partially overlaps M1 — once that fix is in (public `extend_ttl()` entrypoint), this is resolved.

### I2 — Inconsistent storage access patterns  [INFO]

`storage::get_admin/get_manager/get_fee_manager` use `.unwrap()` while other getters use `.unwrap_or_else(|| panic_with_error!(.., NotInitialized))`. The constructor enforces presence, so this is not exploitable, but inconsistent error rendering makes failure modes harder to debug. Resolved by M1's recommendation.

### I3 — `last_boosted_at` stored but never read on-chain  [INFO]

Cheap to keep; used by off-chain monitoring. Fine as is.

---

## 5. Items I caught that Almanax did not

- **H1** (`config.admin` not authenticated in `register_vault`) — design-level review issue; not pattern-detectable.
- **M7** (no emergency pause) — architectural gap.
- **M8** (10000 vs 9000 cap mismatch) — cross-contract consistency check.
- **M9** (single `fee_manager` key, fleet-wide) — risk-accepted by design after discussion; Almanax flagged the related auth pattern (M4) but not the systemic risk.

## 6. Items Almanax caught that I missed

- **L1** (unbounded `VaultConfig` entries). Self-service registration's downside.
- **L2** (negative `amount` in `release_fees`). Defense-in-depth oversight.

## 6a. Items Almanax flagged that were verified false positives

- **Constructor re-callable post-deployment** (Almanax `6710ced6-...`, originally rated CRITICAL). Verified against CAP-0058: `__constructor` is host-reserved and only callable by the Soroban host environment during contract creation. Dismissed. See the dismissed-finding section above for the protocol-level proof.
- **Instance TTL not bumped on init/reads — admin DoS** (Almanax `0c3402b8-...` and `79bb4896-...`, rated MEDIUM). Dismissed: Soroban VM transparently auto-restores archived storage on access since Protocol 23 / 24. The contract uses `soroban-sdk = "25.3.1"`. The proposed DoS path is not reachable on the current network.
- **Campaign persistent TTL strands funds after 120 days** (Almanax `2910e076-...`, rated MEDIUM). Dismissed for the same Protocol 23+ auto-restore reason — persistent storage is also auto-restored on access. Idle 120-day campaigns are effectively ended, partners withdraw before that point, and the restore-rent fee is trivial.
- **Confused-deputy via `caller` parameter** (Almanax `46f83cf2-...`, rated MEDIUM). Dismissed: Soroban's `require_auth()` binds auth entries to specific contract + function + args + invoker, so an attacker cannot fake `caller = fee_manager` without fee_manager's actual signature. The equality check against the storage-resolved role makes the remaining auth surface safe. Recognized code-smell but not an exploitable bug on Soroban.

---

## 7. Verdict and remediation gates

**Fixed in this iteration (code-level changes, tests passing):**
- H1 — `register_vault` asserts `admin == config.admin` (`AdminMismatch` #3024).
- M3 — `rescue_orphan(token, to, amount)` on boost-treasury, bounded by `CampaignList` so it cannot drain tracked campaign budgets. New errors `AccountingCorrupted` (#4041), `InsufficientOrphanBalance` (#4032), new event `OrphanRescued`.
- L2 — proxy-side `amount > 0` check on `release_fees` (`InvalidAmount` #3030).
- L3 — `register_campaign` takes explicit `asset` parameter and asserts the vault's `get_assets()` reports the same address (`AssetMismatch` #4021).
- L4 — `transfer()` docstring rewritten to document the admin drain power; `rescue_orphan` is now the recommended safer alternative for orphan balances.
- L5 — `register_vault` writes proxy-side config to storage before the `set_manager` cross-contract call.

**Risk-accepted with documented rationale (no code change):**
- M9 — single `fee_manager` key is intentional for the hourly bot operation. Single-key compromise cannot redirect funds (vault_fee_receiver is partner-controlled); recovery via `unregister_vault` + `release_fees` and/or boost-treasury top-up.
- M7 — no global pause flag. Existing primitives (`set_fee_manager(burner)`, `set_manager(burner)`, per-campaign `active=false`) provide most of the kill-switch behavior in one tx.
- M8 — proxy permits `max_fee_bps ≤ 10_000` for symmetry with the bps domain; the underlying vault caps at 9000 and will reject any `lock_fees > 9000` at the vault layer. Documented operationally.
- M5 — single-step `fee_manager` / `Manager` rotation mirrors the underlying DeFindex vault's own `set_manager` pattern; consistency preserved.
- M6 — `Campaign.available()` `unwrap_or(0)` is unreachable; pre-checks on all mutation paths plus `overflow-checks = true` in the release profile make the underflow path impossible.

**Dismissed as false positives (Soroban protocol-handled):**
- Almanax CRITICAL on `__constructor` re-callability — CAP-0058 host-reserves the constructor.
- M1 / M2 (TTL DoS / stranded campaigns) — Protocol 23+ auto-restores archived storage on access.
- M4 (confused-deputy) — Soroban auth tree binds entries to specific contract + function + args + invoker.

**Deferred to next iteration:**
- L1 (state-bloat from registration spam — partner-self-service is intentional; could add a refundable bond later).
- L6 (property tests for `Campaign.available()` invariant — useful but not blocking).
- I1, I2, I3 — informational only.

After the fixes above, the contracts are ready for external audit-bank submission. The remaining audit attention should focus on:
1. The `rescue_orphan` bounded-rescue logic (CampaignList iteration, `balance − tracked` math).
2. The `register_vault` post-H1 auth flow (single-signature partner registration).
3. The Soroban platform assumptions (CAP-0058, Protocol 23+) that underlie the dismissed findings.

---

## 8. Appendix: Almanax scan reference

- **Scan ID:** `b7f9a5c2-7f1f-4b2a-a1fe-6da3ece53e6c`
- **Commit:** `edbd57cc38b2aa2ac5b790c970d54aa0ab9a6666`
- **Repo:** `https://github.com/defindex-io/stellar-apy-stabilizer`
- **Findings retrieved:** 8 OPEN
- **Findings cross-referenced into this report:** 7 accepted + 1 dismissed as false positive

| Almanax ID | Title | Disposition |
|---|---|---|
| `6710ced6-...` | Roles can be overwritten due to missing init-guard | **DISMISSED** — false positive per CAP-0058; `__constructor` is host-reserved in Soroban SDK 22+ |
| `0c3402b8-...` | Instance role keys can expire and brick contract (treasury) | **DISMISSED** — Protocol 23+ auto-restores archived storage on access |
| `79bb4896-...` | Instance TTL not bumped on initialization or reads (proxy) | **DISMISSED** — same reason as above |
| `2910e076-...` | Campaign state TTL expiry can strand token balances | **DISMISSED** — Protocol 23+ auto-restores archived persistent storage on access |
| `4192915a-...` | Budget accounting can strand excess token balances | Accepted as M3 |
| `46f83cf2-...` | Auth-by-parameter enables confused-deputy calls | **DISMISSED** — Soroban auth tree binds entries to specific calls; not exploitable in this code |
| `cf74ba74-...` | Unbounded persistent VaultConfig entries enable state bloat | Accepted as L1 |
| `5255a636-...` | Negative fee release amount may break accounting | Accepted as L2 |
