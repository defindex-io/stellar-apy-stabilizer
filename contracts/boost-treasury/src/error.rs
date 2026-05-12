use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    // 4000–4099 range: distinct from DeFindex Vault (100–199) and strategy
    // contracts (200–299), to avoid Stellar SDK error-rendering collisions
    // where the same numeric code maps to a different enum variant on a
    // different contract.
    Unauthorized = 4000,
    NoPendingAdmin = 4001,
    CampaignAlreadyRegistered = 4010,
    CampaignNotRegistered = 4011,
    CampaignInactive = 4012,
    CampaignHasBalance = 4013,
    MultiAssetVaultNotSupported = 4020,
    InvalidAmount = 4030,
    InsufficientBudget = 4031,
    Overflow = 4040,
}
