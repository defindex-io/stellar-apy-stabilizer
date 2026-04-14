use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultRegistered {
    #[topic]
    pub vault: Address,
    #[topic]
    pub admin: Address,
    pub target_apy_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultUnregistered {
    #[topic]
    pub vault: Address,
    #[topic]
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesLocked {
    #[topic]
    pub vault: Address,
    pub fee_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesDistributed {
    #[topic]
    pub vault: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigUpdated {
    #[topic]
    pub vault: Address,
    pub target_apy_bps: u32,
    pub max_fee_bps: u32,
    pub min_fee_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeManagerUpdated {
    pub old: Address,
    pub new_addr: Address,
}
