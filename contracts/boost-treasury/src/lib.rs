#![no_std]

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, vec, Address, Env, Symbol, Vec,
};

mod error;
mod events;
mod storage;

mod test;

pub use error::ContractError;
pub use storage::Campaign;
use storage::{extend_instance_ttl, VaultAssetStrategySet};

// --- Auth helpers (private) ---

fn require_admin(env: &Env) {
    let admin = storage::get_admin(env);
    admin.require_auth();
}

fn require_manager(env: &Env) {
    let manager = storage::get_manager(env);
    manager.require_auth();
}

fn require_active_campaign(env: &Env, vault: &Address) -> Campaign {
    let campaign = storage::get_campaign(env, vault)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::CampaignNotRegistered));
    if !campaign.active {
        panic_with_error!(env, ContractError::CampaignInactive);
    }
    campaign
}

fn require_positive_amount(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, ContractError::InvalidAmount);
    }
}

#[contract]
pub struct BoostTreasury;

#[contractimpl]
impl BoostTreasury {
    pub fn __constructor(env: Env, admin: Address, manager: Address) {
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_manager(&env, &manager);
    }

    // --- Read-only ---

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    pub fn get_manager(env: Env) -> Address {
        storage::get_manager(&env)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        storage::get_pending_admin(&env)
    }

    pub fn get_campaign(env: Env, vault: Address) -> Campaign {
        storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered))
    }

    // --- Admin-only ---

    pub fn set_manager(env: Env, new_manager: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let old = storage::get_manager(&env);
        storage::set_manager(&env, &new_manager);

        events::ManagerUpdated {
            old,
            new_addr: new_manager,
        }
        .publish(&env);
    }

    /// Proposes a new admin. The new address must call `accept_admin` to take
    /// the role. Calling again with a different address overwrites the pending
    /// slot; calling with the current admin's own address effectively cancels.
    pub fn propose_admin(env: Env, new_admin: Address) {
        extend_instance_ttl(&env);
        let admin = storage::get_admin(&env);
        admin.require_auth();

        storage::set_pending_admin(&env, &new_admin);

        events::AdminProposed {
            current: admin,
            pending: new_admin,
        }
        .publish(&env);
    }

    /// Accepts a pending admin transfer. Must be called by the exact address
    /// that was previously proposed. Clears the pending slot on success.
    pub fn accept_admin(env: Env, new_admin: Address) {
        extend_instance_ttl(&env);
        new_admin.require_auth();

        let pending = storage::get_pending_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NoPendingAdmin));
        if new_admin != pending {
            panic_with_error!(&env, ContractError::Unauthorized);
        }

        let old = storage::get_admin(&env);
        storage::set_admin(&env, &new_admin);
        storage::remove_pending_admin(&env);

        events::AdminUpdated {
            old,
            new_addr: new_admin,
        }
        .publish(&env);
    }

    /// Registers a boost campaign for a single-asset DeFindex vault. Admin must
    /// pass `asset` explicitly and the contract asserts the vault's
    /// `get_assets()` returns the same address — this prevents a malicious or
    /// misconfigured vault from steering subsequent `deposit` / `boost` token
    /// transfers to an unexpected token.
    pub fn register_campaign(env: Env, vault: Address, asset: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        if storage::has_campaign(&env, &vault) {
            panic_with_error!(&env, ContractError::CampaignAlreadyRegistered);
        }

        // Verify the vault self-reports the expected asset.
        let assets: Vec<VaultAssetStrategySet> = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_assets"),
            vec![&env],
        );
        if assets.len() != 1 {
            panic_with_error!(&env, ContractError::MultiAssetVaultNotSupported);
        }
        let reported = assets.get_unchecked(0).address;
        if reported != asset {
            panic_with_error!(&env, ContractError::AssetMismatch);
        }

        let campaign = Campaign {
            active: true,
            asset: asset.clone(),
            total_deposited: 0,
            total_boosted: 0,
            total_withdrawn: 0,
            last_boosted_at: 0,
        };
        storage::set_campaign(&env, &vault, &campaign);

        // Track the vault in the campaign list so `rescue_orphan` can enumerate
        // tracked balances by asset.
        let mut list = storage::get_campaign_list(&env);
        list.push_back(vault.clone());
        storage::set_campaign_list(&env, &list);

        events::CampaignRegistered {
            vault,
            asset,
        }
        .publish(&env);
    }

    pub fn update_campaign(env: Env, vault: Address, active: bool) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let mut campaign = storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered));

        campaign.active = active;
        storage::set_campaign(&env, &vault, &campaign);

        events::CampaignUpdated { vault, active }.publish(&env);
    }

    pub fn unregister_campaign(env: Env, vault: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);

        let campaign = storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered));

        if campaign.available() != 0 {
            panic_with_error!(&env, ContractError::CampaignHasBalance);
        }

        storage::remove_campaign(&env, &vault);

        // Drop the vault from the campaign list so `rescue_orphan` does not
        // attempt to read its now-removed Campaign entry.
        let list = storage::get_campaign_list(&env);
        let mut new_list = Vec::new(&env);
        for v in list.iter() {
            if v != vault {
                new_list.push_back(v);
            }
        }
        storage::set_campaign_list(&env, &new_list);

        events::CampaignUnregistered { vault }.publish(&env);
    }

    // --- Anyone (authenticated) ---

    pub fn deposit(env: Env, caller: Address, vault: Address, amount: i128) {
        extend_instance_ttl(&env);
        caller.require_auth();
        require_positive_amount(&env, amount);

        let mut campaign = require_active_campaign(&env, &vault);

        token::Client::new(&env, &campaign.asset).transfer(
            &caller,
            env.current_contract_address(),
            &amount,
        );

        campaign.total_deposited = campaign
            .total_deposited
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Overflow));
        storage::set_campaign(&env, &vault, &campaign);

        events::Deposited {
            vault,
            depositor: caller,
            amount,
        }
        .publish(&env);
    }

    // --- Manager-only ---

    pub fn boost(env: Env, vault: Address, amount: i128) {
        extend_instance_ttl(&env);
        require_manager(&env);
        require_positive_amount(&env, amount);

        let mut campaign = require_active_campaign(&env, &vault);

        if amount > campaign.available() {
            panic_with_error!(&env, ContractError::InsufficientBudget);
        }

        token::Client::new(&env, &campaign.asset).transfer(
            &env.current_contract_address(),
            &vault,
            &amount,
        );

        campaign.total_boosted = campaign
            .total_boosted
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Overflow));
        campaign.last_boosted_at = env.ledger().timestamp();
        storage::set_campaign(&env, &vault, &campaign);

        events::Boosted { vault, amount }.publish(&env);
    }

    /// Sends `amount` tokens from a campaign's tracked budget to `to`. Admin
    /// only. Used for refunds (returning unspent budget to depositors) or for
    /// reallocating between vaults.
    ///
    /// ⚠️ This function grants the admin role full power to drain any
    /// campaign's `available()` budget to any address. Depositors should be
    /// aware that admin can redirect their funds — this is by design, the
    /// admin is the trusted operator of the treasury. For tokens that arrived
    /// OUTSIDE `deposit()` (direct transfers, dust, mistaken sends), use
    /// `rescue_orphan` instead, which is bounded by the campaign-tracked totals
    /// and cannot accidentally move funds attributed to a campaign.
    pub fn transfer(env: Env, vault: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        require_admin(&env);
        require_positive_amount(&env, amount);

        let campaign = storage::get_campaign(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CampaignNotRegistered));

        if amount > campaign.available() {
            panic_with_error!(&env, ContractError::InsufficientBudget);
        }

        token::Client::new(&env, &campaign.asset).transfer(
            &env.current_contract_address(),
            &to,
            &amount,
        );

        let new_total_withdrawn = campaign
            .total_withdrawn
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Overflow));
        let updated = Campaign {
            total_withdrawn: new_total_withdrawn,
            ..campaign
        };
        storage::set_campaign(&env, &vault, &updated);

        events::Transferred { vault, to, amount }.publish(&env);
    }

    /// Sweeps tokens out of the treasury that aren't accounted for in any
    /// campaign — direct transfers, refunds from a vault, dust, mistaken
    /// sends. Admin only.
    ///
    /// The function computes `orphan = balance(token) - sum_of_available()` for
    /// every campaign whose `asset == token`, and refuses to send more than
    /// `orphan`. This bound ensures that an admin typo cannot move tokens
    /// attributed to an active campaign — `rescue_orphan` is strictly safer
    /// than `transfer()` for handling unaccounted balances.
    pub fn rescue_orphan(env: Env, token: Address, to: Address, amount: i128) {
        extend_instance_ttl(&env);
        require_admin(&env);
        require_positive_amount(&env, amount);

        let balance = token::Client::new(&env, &token)
            .balance(&env.current_contract_address());

        let list = storage::get_campaign_list(&env);
        let mut tracked: i128 = 0;
        for vault in list.iter() {
            if let Some(campaign) = storage::get_campaign(&env, &vault) {
                if campaign.asset == token {
                    tracked = tracked
                        .checked_add(campaign.available())
                        .unwrap_or_else(|| {
                            panic_with_error!(&env, ContractError::Overflow)
                        });
                }
            }
        }

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
}
