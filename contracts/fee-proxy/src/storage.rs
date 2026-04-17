use soroban_sdk::{contracttype, Address, Env};

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
    FeeManager,
    VaultConfig(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultConfig {
    pub admin: Address,
    pub target_apy_bps: u32,
    pub max_fee_bps: u32,
    pub min_fee_bps: u32,
}

// --- TTL ---

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

pub fn extend_vault_config_ttl(env: &Env, vault: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::VaultConfig(vault.clone()),
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

// --- Admin ---

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap()
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

// --- FeeManager ---

pub fn get_fee_manager(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::FeeManager)
        .unwrap()
}

pub fn set_fee_manager(env: &Env, fee_manager: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::FeeManager, fee_manager);
}

// --- VaultConfig ---

pub fn get_vault_config(env: &Env, vault: &Address) -> Option<VaultConfig> {
    let config = env
        .storage()
        .persistent()
        .get(&DataKey::VaultConfig(vault.clone()));
    if config.is_some() {
        extend_vault_config_ttl(env, vault);
    }
    config
}

pub fn set_vault_config(env: &Env, vault: &Address, config: &VaultConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::VaultConfig(vault.clone()), config);
    extend_vault_config_ttl(env, vault);
}

pub fn remove_vault_config(env: &Env, vault: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::VaultConfig(vault.clone()));
}

pub fn has_vault_config(env: &Env, vault: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::VaultConfig(vault.clone()))
}
