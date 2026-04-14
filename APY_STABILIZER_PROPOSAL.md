# APY Stabilizer - Technical Proposal

**Date:** 2026-04-13
**Issue:** paltalabs/defindex#841
**Author:** DeFindex Team

---

## 1. Problem Statement

DeFindex vaults have variable APY driven by underlying strategy yields (e.g., Blend Protocol lending rates). Partners (neobanks, wallets) want to offer their users a **stable, predictable APY** - for example, a fixed 4% cap - regardless of market fluctuations.

### Current Limitations

1. **Only the Manager role can call `lock_fees()`** - the function that sets the vault fee rate and locks gains as fees.
2. **Partners hold all roles** (Manager, FeeReceiver, EmergencyManager, RebalanceManager) - typically using a single address for all.
3. **DeFindex has no access** to partner vaults to adjust fees dynamically.
4. **No granular permission delegation** - giving someone the Manager role gives them EVERYTHING: fee management, upgrades, rebalancing, role changes, emergency controls.
5. **Fee adjustment requires manual intervention** - there's no automated mechanism.

### What We Need

A system that allows DeFindex to dynamically adjust vault fees to maintain a target APY, **without modifying the existing vault contracts**, while partners retain full control over everything else.

---

## 2. Current Architecture (Relevant Parts)

### Vault Roles & Permissions


| Role                 | Can Do                                                                                                                    | Set By                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| **Manager**          | `lock_fees`, `distribute_fees`, `release_fees`, `rebalance`, `upgrade`, set all roles, `rescue`, pause/unpause strategies | Current Manager                |
| **VaultFeeReceiver** | `distribute_fees`, `set_fee_receiver` (itself)                                                                            | Manager or current FeeReceiver |
| **EmergencyManager** | `rescue`, `pause_strategy`                                                                                                | Manager                        |
| **RebalanceManager** | `rebalance`                                                                                                               | Manager                        |


### Fee Flow

```
Strategy Yields --> gains_or_losses accumulates
         |
         v
    lock_fees(fee_bps)  <-- Manager only
    +------------------------------------------+
    | fee = (gains x fee_bps) / 10,000         |
    | locked_fee += fee                        |
    | gains_or_losses = 0                      |
    +------------------------------------------+
         |
         v
    distribute_fees()  <-- Manager or FeeReceiver
    +------------------------------------------+
    | defindex_share = locked x defindex_rate   |
    | vault_share = locked - defindex_share     |
    | Transfer vault_share --> VaultFeeReceiver |
    | Transfer defindex_share --> DeFindex      |
    +------------------------------------------+
```

### Key Fact: Contracts CAN Be Roles

The vault's `set_role()` accepts any Soroban `Address` - no restriction on whether it's an account or a contract. Soroban's `require_auth()` works for both. **This is what makes both solutions possible.**

### APY Formula

```
APY = ((endPPS / startPPS) ^ (365.2425 / actualDays) - 1) x 100

where PPS = total_managed_funds / total_shares
```

### Fee-to-APY Relationship

```
user_facing_apy = gross_apy x (1 - vault_fee_bps / 10,000)

To target a specific APY:
  vault_fee_bps = max(0, (1 - target_apy / gross_apy) x 10,000)
```

Example: Gross APY = 8%, Target = 4% --> fee = 5,000 bps (50%)
Example: Gross APY = 5%, Target = 4% --> fee = 2,000 bps (20%)
Example: Gross APY = 3%, Target = 4% --> fee = 0 bps (need boost)

---

## 3. Solution A: Vault Roles Manager (Single Contract) + Off-Chain Bot

### Overview

Deploy a **single proxy contract** ("Vault Roles Manager") that manages **all partner vaults**. Each partner sets this same contract as their vault's Manager. The contract stores per-vault configuration (target APY, fee bounds, admin) in a mapping keyed by vault address. DeFindex gets fee management permission across all vaults; each partner retains full admin control over their own vault.

A **bot running in the existing defindex-api** monitors all registered vaults via the indexer and calls the proxy to adjust fees.

The contract is **not upgradable** - this is a feature, not a limitation. Partners can trust the contract won't change behavior after they set it as Manager. Future features (auto-rebalancing) are deployed as **separate contracts** set on separate roles, following the same pattern.

### Architecture

```
+----------------------------------------------------------+
|  Partner A's Vault    |  Partner B's Vault    |  ...     |
|  Manager = Proxy      |  Manager = Proxy      |          |
|  EmrgMgr = partner A  |  EmrgMgr = partner B  |          |
|  RebalMgr = partner A |  RebalMgr = partner B |          |
+-----------+-----------+-----------+-----------+----------+
            |                       |
            |   require_auth() on proxy = auto-authorized
            |                       |
+-----------v-----------------------v----------------------+
|          Vault Roles Manager Contract (single instance)  |
|                                                          |
|  Global:                                                 |
|  +--------------------------------------------------+   |
|  | fee_manager: DeFindex bot address                 |   |
|  +--------------------------------------------------+   |
|                                                          |
|  Per-Vault Config:  Map<VaultAddress, VaultConfig>       |
|  +--------------------------------------------------+   |
|  | Vault CABC...:                                    |   |
|  |   admin: Partner A address                        |   |
|  |   target_apy_bps: 400 (4.00%)                    |   |
|  |   max_fee_bps: 5000, min_fee_bps: 0              |   |
|  |                                                   |   |
|  | Vault CDEF...:                                    |   |
|  |   admin: Partner B address                        |   |
|  |   target_apy_bps: 500 (5.00%)                    |   |
|  |   max_fee_bps: 6000, min_fee_bps: 0              |   |
|  +--------------------------------------------------+   |
+----------------------------^-----------------------------+
                             |
                +------------+------------+
                |                         |
        +-------v------+         +-------v------+
        | Partners      |         |  DeFindex    |
        | (each vault's |         |  Bot (API)   |
        |  admin)       |         |              |
        |               |         |  For ALL     |
        |  - upgrade    |         |  registered  |
        |  - set roles  |         |  vaults:     |
        |  - rebalance  |         |  - lock_fees |
        |  - set target |         |  - distribute|
        |    APY        |         |    _fees     |
        |  - rescue     |         |              |
        |  - unregister |         |  Reads APY   |
        +---------------+         |  from indexer|
                                  +--------------+
```

### Why Single Contract (Not One Per Vault)

- **Scales automatically** - new partners just register, no new deployments.
- **Self-service onboarding** - partner calls `register_vault()` from the admin dashboard, sets proxy as Manager, done. No DeFindex intervention needed.
- **Single address to set as Manager** - every partner sets the same contract address. The indexer can easily detect which vaults use the proxy (Manager == known proxy address).
- **Bot simplicity** - one contract to read all configs from. No tracking multiple contract addresses.
- **Not upgradable = trust** - partners know the contract behavior won't change. If we need new features, we deploy a separate contract (e.g., auto-rebalancer on the RebalanceManager role).

### Proxy Contract Design

#### Storage

```rust
// Global
FeeManager: Address,      // DeFindex bot - same for all vaults

// Per-vault (Map<Address, VaultConfig>)
struct VaultConfig {
    admin: Address,        // Partner - full passthrough for their vault
    target_apy_bps: u32,  // Target APY in basis points (400 = 4.00%)
    max_fee_bps: u32,     // Maximum fee the bot can set (safety cap)
    min_fee_bps: u32,     // Minimum fee (usually 0)
}
```

#### Key Functions

```rust
// --- Registration (self-service) ---

fn register_vault(e: Env, admin: Address, vault: Address, config: VaultConfig)
    // admin.require_auth()
    // Stores config for this vault
    // Calls vault.set_manager(self_contract_address) on behalf of admin
    //   -> Soroban auth: admin pre-authorizes this sub-invocation when signing the tx
    //   -> Single transaction: register + set_manager happen atomically

fn unregister_vault(e: Env, vault: Address)
    // Requires: caller is vault's admin
    // Calls vault.set_manager(admin) to return Manager to the partner
    // Removes config for this vault

// --- Fee Management (FeeManager or vault's Admin) ---

fn lock_fees(e: Env, caller: Address, vault: Address, new_fee_bps: Option<u32>)
    // Validates: caller is fee_manager or vault's admin
    // Validates: new_fee_bps is within vault's [min_fee, max_fee] range
    // Calls: vault.lock_fees(new_fee_bps)

fn distribute_fees(e: Env, caller: Address, vault: Address)
    // Validates: caller is fee_manager or vault's admin
    // Calls: vault.distribute_fees(self_contract_address)

// --- Config (vault's Admin only) ---

fn set_target_apy(e: Env, vault: Address, target_apy_bps: u32)
    // Partner sets desired APY cap for their vault
    // Bot reads this on-chain to know the target

fn set_fee_bounds(e: Env, vault: Address, min_fee_bps: u32, max_fee_bps: u32)
    // Safety: prevents bot from setting extreme fees

// --- Passthrough (vault's Admin only) ---

fn upgrade_vault(e: Env, vault: Address, new_wasm_hash: BytesN<32>)
fn set_vault_manager(e: Env, vault: Address, new_manager: Address)
fn set_vault_fee_receiver(e: Env, vault: Address, caller: Address, receiver: Address)
fn rescue(e: Env, vault: Address, strategy: Address)
    // All require vault's admin auth
    // All forward to the vault with proxy as the authorized caller

// --- View ---

fn get_vault_config(e: Env, vault: Address) -> VaultConfig
    // Anyone can read a vault's config (target APY, fee bounds)
```

#### Safety Mechanisms

- **Fee bounds per vault**: Bot can only set fees within each vault's `[min_fee_bps, max_fee_bps]` range - partner configures this.
- **Admin override**: Each vault's admin can always call any function for their vault, including overriding bot decisions.
- **Transparent config**: Target APY and fee bounds are stored on-chain per vault, readable by anyone.
- **No lockout**: Admin can always call `set_vault_manager()` through the proxy to reclaim Manager, or `unregister_vault()` to remove their vault.
- **Not upgradable**: Contract code is immutable after deployment. Partners know exactly what they're opting into.
- **Vault isolation**: Each vault's config is independent. A partner can only modify their own vault's settings.

### Bot Design (defindex-api)

A cron job in the existing API that processes all registered vaults:

```
+--------------------------------------------------------------+
|                    APY Stabilizer Bot                         |
|                                                              |
|  Every N minutes:                                            |
|                                                              |
|  1. Fetch all vaults where Manager == proxy address          |
|     (from indexer or proxy contract storage)                 |
|                                                              |
|  2. For EACH registered vault:                               |
|     a. Read current PPS from indexer                         |
|     b. Calculate current user-facing APY (7-day rolling)     |
|     c. Read target_apy from proxy contract                   |
|     d. Calculate gross APY (before fees)                     |
|     e. Compute required fee_bps:                             |
|        fee = max(0, (1 - target_apy / gross_apy) x 10,000)  |
|     f. If fee changed significantly (> threshold):           |
|        - Call proxy.lock_fees(vault, new_fee_bps)            |
|        - Optionally call proxy.distribute_fees(vault)        |
|     g. Log action per vault                                  |
|                                                              |
|  3. Emit alerts if any vault APY drifts beyond tolerance     |
|                                                              |
|  Edge cases:                                                 |
|  - Gross APY < target --> set fee to 0                       |
|  - Gross APY ~ target --> no change (dead zone, avoid spam)  |
|  - Strategy losses --> lock_fees skips negative gains anyway |
+--------------------------------------------------------------+
```

#### Boost Mechanism

When APY drops below target and a partner wants to subsidize:

1. Partner (or DeFindex) transfers USDC directly to the vault contract address
2. Partner calls `rebalance()` (through proxy as admin) to invest the idle USDC into strategies
3. This increases PPS for all share holders, effectively boosting APY
4. This is a manual/campaign action, not automated

### Partner Onboarding Flow (Self-Service via Dashboard)

Single transaction - partner signs once, everything happens atomically:

```
  Partner                   Proxy Contract             DeFindex Bot         Vault
     |                           |                        |                  |
     |-- register_vault(config)->|                        |                  |
     |   admin=self              |                        |                  |
     |   target_apy=400          |  [stores config]       |                  |
     |   fee_bounds=(0, 5000)    |                        |                  |
     |                           |--- set_manager(proxy) -------------------->|
     |                           |   (partner pre-authorized                 |
     |                           |    this sub-invocation)  Manager = proxy  |
     |                           |                        |                  |
     |                    [Bot detects new vault via indexer]                |
     |                           |                        |                  |
     |                           |<-- lock_fees(vault, fee_bps) --|          |
     |                           |--- lock_fees(fee_bps) ------->|[executed] |
     |                           |   (auto-authorized)    |                  |
```

Unregistering works the same way in reverse - `unregister_vault()` calls `vault.set_manager(admin)` to return control, then removes the config. One tx.

### Future: Auto-Rebalancing (Separate Contract)

```
+--------------------------------------------------------------+
|  Same pattern, different role:                               |
|                                                              |
|  1. Deploy "Auto Rebalancer" contract (also not upgradable)  |
|  2. Partner sets it as RebalanceManager on their vault       |
|  3. Bot in defindex-api monitors and calls rebalance()       |
|  4. Completely independent from APY stabilizer               |
|                                                              |
|  Vault Roles:                                                |
|    Manager          = Vault Roles Manager (fee stabilizer)   |
|    RebalanceManager = Auto Rebalancer (separate contract)    |
|    EmergencyManager = partner address                        |
|    FeeReceiver      = partner address                        |
+--------------------------------------------------------------+
```

### Pros


| Advantage                        | Details                                                                                                    |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| **Works with current contracts** | Zero changes to vault code. Partner sets proxy as Manager, done.                                           |
| **Single deployment**            | One contract for all vaults. No per-partner deployments. Scales to 100+ vaults without new contracts.      |
| **Self-service onboarding**      | Partner registers via dashboard, sets proxy as Manager. No DeFindex intervention.                          |
| **Not upgradable = trust**       | Partners know contract code won't change. Immutable behavior.                                              |
| **Modular architecture**         | Future features (auto-rebalancing) are separate contracts on separate roles. Clean separation of concerns. |
| **Partner retains control**      | Each vault's admin can override anything, unregister, or reclaim Manager at any time.                      |
| **Simple fee calculation**       | Off-chain bot uses indexer data (already available) - no complex on-chain math.                            |
| **Easy to update logic**         | Bot logic changes are API deployments, not contract upgrades.                                              |
| **Transparent**                  | All configs on-chain per vault. Anyone can verify the bot's behavior.                                      |
| **Existing infrastructure**      | Bot lives in defindex-api. Indexer already has all the data.                                               |


### Cons


| Disadvantage                   | Details                                                                   | Mitigation                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| **Off-chain dependency**       | If the bot goes down, fees stop adjusting. APY drifts.                    | Health monitoring, alerts, redundant cron jobs, partner can always adjust manually.         |
| **Trust requirement**          | Partners must trust DeFindex with fee management (within bounds).         | On-chain fee bounds limit exposure. Admin override. Transparent config. Immutable contract. |
| **Manager replacement**        | Partner gives up direct Manager role. All Manager calls go through proxy. | Proxy is transparent passthrough. Partner can reclaim Manager at any time.                  |
| **Not upgradable**             | Can't add features to this contract.                                      | By design. New features = new contracts on separate roles.                                  |
| **Shared contract risk**       | Bug in proxy affects all vaults.                                          | Thorough testing + audit. Contract is small (~400 LOC). Partners can unregister instantly.  |
| **Bot needs a funded account** | Bot's Stellar account needs XLM for transaction fees.                     | Minimal cost (~0.001 XLM per tx). Easy to fund.                                             |


### Estimated Complexity

- **Proxy contract**: ~400-500 lines of Rust. Standard Soroban patterns. Per-vault storage maps. Medium complexity.
- **Bot (cron job)**: ~200-300 lines of TypeScript. Reads indexer, iterates vaults, calls contract. Low complexity.
- **Testing**: Contract unit tests + integration tests with vault. Medium effort.

---

## 4. Solution B: On-Chain APY Stabilizer

### Overview

Deploy a contract inspired by fee-vault-v2's approach that stores the target APY and **calculates the required fee on-chain** by reading the vault's state. The contract is set as the vault's Manager (same as Solution A) and contains the fee calculation logic within the contract itself.

### Architecture

```
+------------------------------------------------------+
|                    Partner's Vault                    |
|  Manager = APY Stabilizer Contract                   |
|                                                      |
|  Public view functions available:                    |
|  - fetch_total_managed_funds() --> total funds       |
|  - total_supply() (token) --> total shares           |
|  - report() --> strategy gains/losses, locked fees   |
|  - get_fees() --> current fee rates                  |
+-------------------------+----------------------------+
                          |
+-------------------------v----------------------------+
|           On-Chain APY Stabilizer Contract            |
|                                                      |
|  Stored State:                                       |
|  +------------------------------------------------+  |
|  | vault_address: Address                          |  |
|  | target_apy_bps: u32                             |  |
|  | last_pps: i128 (12 decimals)                    |  |
|  | last_timestamp: u64                             |  |
|  | fee_bounds: (min_bps, max_bps)                  |  |
|  | admin: Address (partner)                        |  |
|  +------------------------------------------------+  |
|                                                      |
|  On trigger (stabilize):                             |
|  1. Call vault.fetch_total_managed_funds()            |
|  2. Call vault.total_supply() (token interface)       |
|  3. Calculate current PPS                            |
|  4. Calculate gross APY from PPS change over time    |
|  5. Compute required fee_bps                         |
|  6. Call vault.lock_fees(fee_bps)                    |
|  7. Update stored last_pps and last_timestamp        |
|                                                      |
|  Also has: passthrough functions (same as Solution A)|
+--------------------------^---------------------------+
                           |
                   +-------+-------+
                   |  Trigger Bot  |
                   |  (minimal)    |
                   |               |
                   |  Just calls   |
                   |  stabilize()  |
                   |  periodically |
                   +---------------+
```

### Fee Calculation (On-Chain)

```rust
fn stabilize(e: Env) -> Result<(), ContractError> {
    let vault = vault_client(&e);

    // 1. Get current PPS
    let total_funds = vault.fetch_total_managed_funds(); // sum all assets
    let total_shares = vault.total_supply();
    let current_pps = (total_funds * SCALAR_12) / total_shares;

    // 2. Calculate gross APY since last check
    let last_pps = get_last_pps(&e);
    let last_ts = get_last_timestamp(&e);
    let elapsed = e.ledger().timestamp() - last_ts;

    // gross_growth = current_pps / last_pps
    // gross_apy = (gross_growth ^ (365.25 days / elapsed)) - 1
    // Simplified for on-chain (linear approximation for short periods):
    let growth_rate = ((current_pps - last_pps) * SCALAR_7) / last_pps;
    let annualized = (growth_rate * SECONDS_PER_YEAR) / elapsed;

    // 3. Calculate required fee
    let target = get_target_apy(&e);
    let fee_bps = if annualized <= target {
        0u32
    } else {
        ((annualized - target) * 10_000 / annualized) as u32
    };

    // 4. Clamp and apply
    let fee_bps = fee_bps.clamp(get_min_fee(&e), get_max_fee(&e));
    vault.lock_fees(Some(fee_bps));

    // 5. Update state
    set_last_pps(&e, current_pps);
    set_last_timestamp(&e, e.ledger().timestamp());

    Ok(())
}
```

### Inspired by fee-vault-v2

The fee-vault-v2 implements three rate strategies:

- **Type 0 (Take Rate)**: Fixed % of all earnings go to admin.
- **Type 1 (Capped Rate)**: Users earn up to a target APR; excess goes to admin.
- **Type 2 (Fixed Rate)**: Users always earn the fixed rate; admin subsidizes or captures difference.

We could adopt a similar model. The **Capped Rate** (Type 1) maps directly to our use case. The difference is that fee-vault-v2 wraps a Blend pool directly and intercepts b_rate changes, while we would read DeFindex vault PPS changes.

### Critical Limitation: PPS Calculation Complexity

The vault's `fetch_total_managed_funds()` returns a `Vec<CurrentAssetInvestmentAllocation>` with per-asset, per-strategy breakdowns. For multi-asset vaults, calculating a single PPS number on-chain requires:

1. Summing all asset values across all strategies
2. Converting different assets to a common denomination (requires price oracle or router)
3. Dividing by total shares

**For single-asset vaults** (e.g., USDC-only), this is straightforward. For multi-asset vaults, it requires a price oracle, which adds significant complexity and an external dependency.

### Pros


| Advantage                      | Details                                                    |
| ------------------------------ | ---------------------------------------------------------- |
| **Transparent & verifiable**   | Fee calculation logic is on-chain, auditable by anyone.    |
| **Reduced trust in off-chain** | Bot only triggers; it can't influence the fee calculation. |
| **Consistent behavior**        | Same logic runs every time, no off-chain bugs or drift.    |
| **Auditable history**          | Every state change is a transaction, queryable on-chain.   |


### Cons


| Disadvantage                       | Details                                                                                                                                                          | Severity                                             |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| **Still needs a bot trigger**      | Soroban has no cron jobs. Someone must call `stabilize()` periodically.                                                                                          | High - eliminates the main advantage over Solution A |
| **Multi-asset PPS is hard**        | Need price oracle for multi-asset vaults to compute PPS on-chain.                                                                                                | High - adds oracle dependency                        |
| **More gas costs**                 | On-chain computation (cross-contract calls to vault, math) costs more per execution.                                                                             | Medium                                               |
| **Harder to update**               | Changing fee logic requires a contract upgrade. Solution A: just redeploy the bot.                                                                               | Medium                                               |
| **Linear APY approximation**       | On-chain math limitations mean we approximate the exponential APY formula. Short periods amplify error.                                                          | Medium                                               |
| **Needs the proxy pattern anyway** | Even if fees are calculated on-chain, we still need permission delegation for partner passthrough functions. Would need to be this contract OR a separate proxy. | High - doesn't avoid Solution A's contract           |
| **Complex edge cases**             | Handling negative yields, multi-asset rebalancing, and rounding errors on-chain is harder than off-chain.                                                        | Medium                                               |
| **Larger contract**                | More code on-chain = larger deployment, more surface area for bugs.                                                                                              | Low                                                  |


---

## 5. Comparison Matrix


| Criteria                               | Solution A (Proxy + Bot)                                      | Solution B (On-Chain Stabilizer)                 |
| -------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------ |
| **Works with current vault contracts** | Yes                                                           | Yes                                              |
| **Needs off-chain bot**                | Yes (calculates + triggers)                                   | Yes (triggers only)                              |
| **Contract complexity**                | Low-Medium (~400-500 LOC, passthrough + ACL + per-vault maps) | High (~600-800 LOC, includes math + vault reads) |
| **Bot complexity**                     | Medium (reads indexer, calculates fee)                        | Low (just calls `stabilize()`)                   |
| **Multi-asset vault support**          | Easy (indexer already handles it)                             | Hard (needs price oracle on-chain)               |
| **Fee logic updates**                  | API deployment (minutes)                                      | Contract upgrade (needs partner approval)        |
| **Transparency**                       | Config on-chain, logic off-chain                              | Everything on-chain                              |
| **Gas cost per execution**             | 1 contract call (lock_fees)                                   | 3+ cross-contract reads + lock_fees              |
| **Extensible for rebalancing**         | Yes, add permission + bot logic                               | Yes, but more contract code                      |
| **Partner UX for target APY**          | Set on proxy contract or dashboard                            | Set on contract                                  |
| **Risk of bugs**                       | Bot bugs = wrong fee (correctable)                            | Contract bugs = wrong fee (needs upgrade)        |
| **Time to ship**                       | Shorter                                                       | Longer                                           |
| **Audit surface**                      | Smaller contract                                              | Larger contract                                  |


---

## 6. Security Analysis

### 6.1 Solution A (Proxy + Bot) - Attack Vectors & Mitigations

#### DOS on the Proxy Contract

**Spam registration**: Could an attacker call `register_vault()` repeatedly with fake vault addresses to bloat contract storage and increase rent costs?
- **Mitigated by design**: `register_vault()` calls `vault.set_manager(proxy)` internally. If the caller isn't the actual Manager of that vault, the `require_auth()` check inside the vault fails. You can only register a vault you actually control.
- **No anonymous registration possible** - every registration requires the current Manager's signature on the sub-invocation.

**Transaction spam on the proxy**: Could someone spam `lock_fees()` or `distribute_fees()` to run up the bot's costs or interfere with operations?
- **Mitigated by auth**: Only the `fee_manager` (DeFindex bot) or the vault's `admin` can call these functions. Unauthorized callers get rejected before any state change or cross-contract call.
- **No public entry points** exist that an attacker could call without proper authorization.

**Contract TTL exhaustion**: Soroban contracts have a TTL. If nobody interacts with the proxy for a long period, it gets archived.
- **Risk**: If the proxy gets archived, partners can't call Manager functions through it and the bot can't adjust fees. Partners would be effectively locked out of Manager operations until the contract is restored.
- **Mitigation**: Every function call must extend the contract instance TTL (`extend_instance_ttl`). The bot calls regularly (every cycle) which keeps it alive. As long as the bot runs OR any partner interacts, the TTL stays fresh.
- **Emergency recovery**: Soroban supports `restoreFootprint` to restore archived contracts. The data is not lost, just archived. Anyone can restore it.
- **Additional safeguard**: We should implement a public `extend_ttl()` function that anyone can call (costs only the caller's XLM) to keep the contract alive even if the bot and all partners are inactive.

#### DeFindex Bot Compromise

**What if the bot's private key is stolen?**
- **Blast radius is limited**: The bot (fee_manager) can ONLY call `lock_fees()` and `distribute_fees()`. It cannot:
  - Upgrade any vault
  - Change any roles
  - Rescue/withdraw funds
  - Rebalance strategies
  - Change its own permissions
  - Unregister vaults
- **Fee bounds cap the damage**: Even with full fee_manager access, the attacker can only set fees within each vault's configured `[min_fee_bps, max_fee_bps]` range. A vault with max_fee=5000 bps can't have its fee set to 9000 bps.
- **Worst case**: Attacker sets every vault's fee to its maximum for some period. Partners see APY drop, investigate, and override via admin. No funds are stolen - fees are still distributed to the legitimate fee receiver.
- **Response**: Rotate the fee_manager address on the proxy (requires DeFindex to deploy a new bot key, then update the global fee_manager setting).

#### Partner Admin Compromise

**What if a partner's admin key is stolen?**
- **Same risk as today**: The admin has full Manager-equivalent access to their vault through the proxy. A compromised admin can upgrade, rescue, change roles - everything.
- **No worse than current situation**: Without the proxy, partners already hold Manager directly. The proxy doesn't increase this attack surface.
- **Mitigation**: Recommend partners use multi-sig or hardware wallets for the admin address.

#### Single Contract = Single Point of Failure

**Bug in the proxy affects ALL vaults simultaneously.**
- **Risk**: A vulnerability in the proxy contract could be exploited across all registered vaults at once.
- **Mitigations**:
  - Contract is small (~400-500 LOC) and simple (passthrough + ACL). Small surface area.
  - Thorough unit tests + integration tests with actual vault contract.
  - External audit before mainnet deployment.
  - Contract is not upgradable, so the deployed code is the audited code forever.
- **Emergency response**: Any partner can call `unregister_vault()` to reclaim Manager instantly. This is a single transaction, no coordination needed.
- **vs per-vault proxies**: Per-vault proxies share the same WASM code anyway. A bug in the WASM affects all instances equally. Single contract is no worse in this regard.

#### Vault Isolation

**Can one partner's admin affect another partner's vault?**
- **No**: Every function checks that the caller is the specific vault's admin. Partner A's admin address has zero permissions on Partner B's vault config. The per-vault storage map enforces complete isolation.
- **The fee_manager is global** but it can only perform fee operations (lock/distribute) within each vault's configured bounds. It cannot modify vault configs.

### 6.2 Solution B (On-Chain) - Additional Security Concerns

Everything in 6.1 applies (the proxy pattern is shared), plus these additional risks:

#### On-Chain Math Breaks with Multi-Asset Vaults

This is the most critical security concern with Solution B. The on-chain fee calculation requires computing PPS, which requires converting all assets to a common denomination.

**Price oracle dependency:**
- Multi-asset vaults (e.g., USDC + XLM) need a price feed to compute a single PPS number.
- Oracle manipulation could lead to incorrect fee calculations.
- A flash loan could temporarily inflate one asset's price, spiking the computed PPS, causing the contract to set an artificially high fee - effectively stealing yield from users in the form of excess fees.

**Integer arithmetic edge cases:**
- Soroban has no floating point. All math is integer-based.
- Different assets have different decimal precisions.
- Division truncation accumulates: `(a / b) * c != (a * c) / b` in integer math.
- Small balances with large multipliers can overflow `i128`.
- Short time periods (e.g., 1 hour) amplify rounding errors when annualizing:
  ```
  growth in 1 hour = 0.000456% (tiny number)
  annualized = 0.000456% * 8,760 = 3.99%   (linear approx)
  actual APY = (1.00000456)^8760 = 4.07%    (compound)
  Error = 0.08% -- this compounds further with rounding
  ```
- With 2+ assets at different scales, these errors multiply.

**Specific failure scenarios:**
- **Asset with zero balance**: Division by zero if one asset in the vault has zero balance in a strategy.
- **Negative gains on one asset, positive on another**: Net PPS might look flat, but individual strategies have wildly different performance. A uniform fee rate applied to all strategies is wrong - you'd be charging fees on a losing strategy (which lock_fees already handles by skipping negative gains, but the PPS calculation doesn't account for this).
- **Rebalancing changes PPS without yield change**: If a partner rebalances between assets (e.g., sells XLM for USDC), the PPS might jump due to price impact, not yield. The on-chain contract would misinterpret this as yield and lock excessive fees.
- **New strategy added**: PPS calculation changes when the denominator shifts. The contract would need to handle this gracefully.

**Why this doesn't affect Solution A:**
- The bot uses the indexer, which already correctly handles multi-asset PPS calculation.
- The bot can use sophisticated logic: 7-day rolling averages, exclude rebalancing events, handle edge cases in code that's easy to update.
- If the calculation is wrong, fix the bot code and redeploy in minutes. If the on-chain calculation is wrong, you need a contract upgrade (which we said is not possible) or a full migration.

#### Timing Manipulation

**Could someone time their `stabilize()` call to game the fee calculation?**
- The `stabilize()` function reads the vault's current state at the moment of execution.
- If called right after a large deposit (PPS temporarily dips due to share dilution), the calculated APY would be artificially low, resulting in a lower fee.
- If called right after strategy gains are reported but before lock_fees, APY looks high, resulting in a higher fee.
- **Solution A avoids this** because the bot controls when it calls and uses rolling averages from the indexer, not point-in-time snapshots.

### 6.3 Common Security Considerations (Both Solutions)

#### Bot Downtime

- If the bot goes down, fees stop being adjusted. APY drifts from target.
- **This is NOT a security risk** - it's an operational risk. No funds are lost. The vault continues operating normally, just with a stale fee rate.
- Mitigations: health checks, alerting, redundant cron, partner can always adjust manually.

#### Indexer Data Integrity (Solution A)

- The bot trusts the indexer for APY data. If the indexer is compromised or returns stale data, the bot could set wrong fees.
- **Mitigated by fee bounds**: Even with bad data, fees stay within the configured range.
- **Mitigated by dead zone**: Bot only adjusts when deviation exceeds threshold, so stale data that's close to reality causes no action.
- **Detectable**: Fee adjustment logs show what the bot did and why. Anomalies are visible.

#### Contract Storage Rent (Soroban-Specific)

- All contract storage in Soroban requires rent to stay alive.
- Per-vault configs are small (~100-200 bytes each). Even 100 vaults is negligible rent.
- The bot's regular calls extend TTL, covering rent automatically.

---

## 7. Recommendation

### Go with Solution A: Vault Roles Manager + Off-Chain Bot

The on-chain approach (Solution B) does NOT eliminate the need for a bot - it only moves the fee calculation from the bot to the contract. This is the decisive factor: **if you need a bot anyway, keep the complex logic in the bot where it's easy to update**.

Solution B would make more sense if Soroban had automatic execution (cron-like triggers), but it doesn't. The "trustless, on-chain" advantage is diminished when you still need an off-chain trigger.

Additionally:

- Solution A's proxy contract is **needed by both approaches** for permission delegation.
- The bot already has access to rich indexer data (historical APY, PPS, strategy balances) that would be expensive to replicate on-chain.
- Multi-asset vault support comes free with the indexer but requires an oracle on-chain.
- Fee logic changes are API deployments (fast) vs contract upgrades (slow, needs partner approval).

### Hybrid Element Worth Keeping

Store the **target APY and fee bounds on-chain** in the proxy contract (from Solution B). This gives us:

- Partners can set their target APY via a transaction (could integrate into an admin dashboard).
- Bot reads target from chain - single source of truth.
- Transparent and verifiable configuration.
- No separate database for per-vault APY targets.

---

## 8. Implementation Plan

### Phase 1: Vault Roles Manager + APY Bot (Core)

**1.1 - Proxy Contract (Soroban/Rust)**

- Single contract managing all vaults via per-vault storage maps
- `register_vault()` / `unregister_vault()` for self-service onboarding
- Per-vault admin with passthrough for all Manager functions
- Global fee_manager role for the DeFindex bot
- Fee bounds validation per vault (bot can't exceed configured range)
- `get_vault_config()` view function for bot and dashboard reads
- Not upgradable (immutable after deployment)
- Unit tests + integration tests with vault contract

**1.2 - APY Stabilizer Bot (TypeScript, in defindex-api)**

- Cron job: runs on all registered vaults each cycle
- Discover vaults via indexer (where Manager == proxy address) or read proxy storage
- For each vault: read APY from indexer, read target from proxy, calculate fee
- Call proxy.lock_fees(vault, fee_bps) when adjustment needed (with dead zone)
- Call proxy.distribute_fees(vault) periodically
- Logging, alerting, health checks

**1.3 - Partner Onboarding (Self-Service)**

- Partner calls `register_vault(config)` on the proxy (from dashboard or CLI)
- Contract automatically calls `vault.set_manager(proxy)` in the same tx (partner pre-authorizes the sub-invocation)
- Bot auto-detects the new vault and starts managing fees
- No DeFindex intervention required. One transaction. One signature.

### Phase 2: Dashboard Integration

- "Stable APY" toggle in partner admin dashboard
- Partners set target APY, fee bounds via UI (signs tx to proxy)
- Real-time APY monitoring with target overlay
- Fee adjustment history and bot action log

### Phase 3: Auto-Rebalancing (Future, Separate Contract)

- Deploy a new "Auto Rebalancer" contract (same single-contract-for-all-vaults pattern)
- Partners set it as RebalanceManager on their vault
- Build rebalancing bot logic in defindex-api
- Completely independent from APY stabilizer - different contract, different role

---

## 9. Open Questions for Discussion

1. **Fee adjustment frequency**: How often should the bot check and adjust? Every hour? Every 6 hours? More frequent = tighter APY control but more transactions.
2. **APY calculation window**: Should we use a 7-day rolling APY or a shorter window (24h, 48h) for faster response to yield changes?
3. **Dead zone threshold**: What APY deviation from target should trigger an adjustment? (e.g., only adjust if current APY differs by > 0.5% from target to avoid transaction spam)
4. **Distribute frequency**: How often should `distribute_fees()` be called? Every lock_fees call? Daily? Weekly?
5. **Emergency override**: Should the bot have a kill switch? If yields crash, should it automatically set fees to 0 and alert the team?
6. **Multi-sig for admin**: Should we recommend partners use a multi-sig as the proxy admin for extra security?
7. **Boost mechanism governance**: When subsidizing APY below target, who authorizes the USDC transfer? Manual partner decision or automated from a reserve?
8. **Contract versioning**: Since the proxy is not upgradable, how do we handle the case where we deploy a v2 with bug fixes? Do partners manually migrate (unregister from v1, register on v2, set new Manager)? Should we build a migration helper?

---

## Appendix A: Vault Functions the Proxy Must Support

All functions take `vault: Address` as first param to identify which vault to operate on.


| Function                                | Required Role (on proxy)     | Vault Role Used             |
| --------------------------------------- | ---------------------------- | --------------------------- |
| `lock_fees(vault, new_fee_bps)`         | fee_manager OR vault's admin | Manager                     |
| `distribute_fees(vault)`                | fee_manager OR vault's admin | Manager                     |
| `release_fees(vault, strategy, amount)` | vault's admin                | Manager                     |
| `upgrade(vault, new_wasm_hash)`         | vault's admin                | Manager                     |
| `set_manager(vault, new_manager)`       | vault's admin                | Manager                     |
| `set_fee_receiver(vault, receiver)`     | vault's admin                | Manager                     |
| `set_emergency_manager(vault, new_em)`  | vault's admin                | Manager                     |
| `set_rebalance_manager(vault, new_rm)`  | vault's admin                | Manager                     |
| `rescue(vault, strategy)`               | vault's admin                | Manager or EmergencyManager |
| `pause_strategy(vault, strategy)`       | vault's admin                | Manager or EmergencyManager |
| `unpause_strategy(vault, strategy)`     | vault's admin                | Manager or EmergencyManager |


**Note on rebalance**: Rebalancing is intentionally excluded from this contract. The vault's `rebalance()` checks the `RebalanceManager` role, not Manager. Auto-rebalancing will be a **separate contract** deployed on the RebalanceManager role (Phase 3).

## Appendix B: Fee Calculation Examples

### Scenario 1: Capping at 4% when yield is high


| Metric                  | Value                                  |
| ----------------------- | -------------------------------------- |
| Gross APY (Blend yield) | 8.00%                                  |
| Target APY              | 4.00%                                  |
| Required fee_bps        | `(1 - 4/8) x 10,000 = 5,000 bps` (50%) |
| User gets               | 4.00% APY                              |
| Fee receiver gets       | 4.00% worth of gains                   |


### Scenario 2: Yield drops closer to target


| Metric                  | Value                                  |
| ----------------------- | -------------------------------------- |
| Gross APY (Blend yield) | 5.00%                                  |
| Target APY              | 4.00%                                  |
| Required fee_bps        | `(1 - 4/5) x 10,000 = 2,000 bps` (20%) |
| User gets               | 4.00% APY                              |
| Fee receiver gets       | 1.00% worth of gains                   |


### Scenario 3: Yield drops below target


| Metric                  | Value                                           |
| ----------------------- | ----------------------------------------------- |
| Gross APY (Blend yield) | 3.00%                                           |
| Target APY              | 4.00%                                           |
| Required fee_bps        | `0 bps` (can't charge negative fees)            |
| User gets               | 3.00% APY (below target)                        |
| Boost option            | Transfer USDC to vault + rebalance to subsidize |


### Scenario 4: Yield crashes


| Metric                  | Value                                   |
| ----------------------- | --------------------------------------- |
| Gross APY (Blend yield) | 0.50%                                   |
| Target APY              | 4.00%                                   |
| Required fee_bps        | `0 bps`                                 |
| User gets               | 0.50% APY                               |
| Bot action              | Alert team, set fee to 0, no distribute |


