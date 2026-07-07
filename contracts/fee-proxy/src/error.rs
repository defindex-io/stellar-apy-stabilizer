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
}
