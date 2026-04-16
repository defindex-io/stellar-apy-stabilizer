use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    Unauthorized = 100,
    CampaignAlreadyRegistered = 110,
    CampaignNotRegistered = 111,
    CampaignInactive = 112,
    CampaignHasBalance = 113,
    MultiAssetVaultNotSupported = 120,
    InvalidAmount = 130,
    InsufficientBudget = 131,
}
