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
    PendingAdmin,
    Manager,
    Campaign(Address),
    /// Persistent list of every registered vault. Maintained by
    /// `register_campaign` / `unregister_campaign`. Read by `rescue_orphan` to
    /// enumerate tracked-balance per token.
    CampaignList,
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

// --- PendingAdmin ---

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, pending: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, pending);
}

pub fn remove_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
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

// --- CampaignList ---

pub fn extend_campaign_list_ttl(env: &Env) {
    env.storage().persistent().extend_ttl(
        &DataKey::CampaignList,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

pub fn get_campaign_list(env: &Env) -> Vec<Address> {
    let list: Option<Vec<Address>> = env
        .storage()
        .persistent()
        .get(&DataKey::CampaignList);
    if list.is_some() {
        extend_campaign_list_ttl(env);
    }
    list.unwrap_or_else(|| Vec::new(env))
}

pub fn set_campaign_list(env: &Env, list: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&DataKey::CampaignList, list);
    extend_campaign_list_ttl(env);
}
