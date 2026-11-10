use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    // 3000–3099 range: distinct from DeFindex Vault (100–199), strategies
    // (200–299), and BoostTreasury (4000–4099) so that contract-error
    // rendering never collides across the stack.
    Unauthorized = 3000,
    VaultAlreadyRegistered = 3010,
    VaultNotRegistered = 3011,
    FeeOutOfBounds = 3020,
    InvalidFeeBounds = 3021,
    NoPendingAdmin = 3023,
    InvalidAmount = 3030,
    // Appended after the original variants; codes are explicit so nothing shifts.
    /// `register_vault`: the vault's Manager is already this proxy (F-05).
    VaultManagedByProxy = 3012,
    /// `register_vault`: `config.admin` is not the vault's current Manager.
    NotVaultManager = 3013,
}
