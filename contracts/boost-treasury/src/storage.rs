use soroban_sdk::{contracttype, Address, Env, String, Vec};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;
const PERSISTENT_BUMP_AMOUNT: u32 = 120 * DAY_IN_LEDGERS;
const PERSISTENT_LIFETIME_THRESHOLD: u32 = PERSISTENT_BUMP_AMOUNT - 20 * DAY_IN_LEDGERS;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PendAdmin,
    Manager,
    Campaign(Address),
    /// Per-token running total of every campaign's `available()` budget for that
    /// token. Incremented on `deposit`, decremented on `boost` / `transfer`, so
    /// `rescue_orphan` can compute the orphan balance in O(1) instead of
    /// scanning every campaign. Keyed by token address; the variant name is
    /// ≤ 9 chars so its symbol stays `SymbolSmall` (see B04).
    Tracked(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Campaign {
    pub active: bool,
    pub asset: Address,
    pub total_deposited: i128,
    pub total_boosted: i128,
    pub total_withdrawn: i128,
    /// Ledger timestamp (seconds) of the most recent `boost()` call. 0 if never boosted.
    pub last_boosted_at: u64,
}

impl Campaign {
    /// Returns the remaining boost budget. Invariant enforced by
    /// `require_positive_amount` + `InsufficientBudget` checks: result is always
    /// ≥ 0 and never overflows. The `checked_sub` chain is defensive and fails
    /// closed (returns 0) if the invariant is ever violated.
    pub fn available(&self) -> i128 {
        self.total_deposited
            .checked_sub(self.total_boosted)
            .and_then(|v| v.checked_sub(self.total_withdrawn))
            .unwrap_or(0)
    }
}

// Minimal mirrors of vault types for decoding get_assets() — only what we need
#[contracttype]
#[derive(Clone)]
pub struct VaultStrategy {
    pub address: Address,
    pub name: String,
    pub paused: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct VaultAssetStrategySet {
    pub address: Address,
    pub strategies: Vec<VaultStrategy>,
}

// --- TTL ---

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

pub fn extend_campaign_ttl(env: &Env, vault: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Campaign(vault.clone()),
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

// --- Admin ---

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

// --- PendAdmin ---

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendAdmin)
}

pub fn set_pending_admin(env: &Env, pending: &Address) {
    env.storage().instance().set(&DataKey::PendAdmin, pending);
}

pub fn remove_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendAdmin);
}

// --- Manager ---

pub fn get_manager(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Manager).unwrap()
}

pub fn set_manager(env: &Env, manager: &Address) {
    env.storage().instance().set(&DataKey::Manager, manager);
}

// --- Campaign ---

pub fn get_campaign(env: &Env, vault: &Address) -> Option<Campaign> {
    let campaign = env
        .storage()
        .persistent()
        .get(&DataKey::Campaign(vault.clone()));
    if campaign.is_some() {
        extend_campaign_ttl(env, vault);
    }
    campaign
}

pub fn set_campaign(env: &Env, vault: &Address, campaign: &Campaign) {
    env.storage()
        .persistent()
        .set(&DataKey::Campaign(vault.clone()), campaign);
    extend_campaign_ttl(env, vault);
}

pub fn remove_campaign(env: &Env, vault: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::Campaign(vault.clone()));
}

pub fn has_campaign(env: &Env, vault: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Campaign(vault.clone()))
}

// --- Tracked (per-token available-budget total) ---

pub fn extend_tracked_ttl(env: &Env, token: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Tracked(token.clone()),
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

pub fn get_tracked_balance(env: &Env, token: &Address) -> i128 {
    let tracked: Option<i128> = env
        .storage()
        .persistent()
        .get(&DataKey::Tracked(token.clone()));
    if tracked.is_some() {
        extend_tracked_ttl(env, token);
    }
    tracked.unwrap_or(0)
}

pub fn set_tracked_balance(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Tracked(token.clone()), &amount);
    extend_tracked_ttl(env, token);
}
