use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignRegistered {
    #[topic]
    pub vault: Address,
    pub asset: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignUpdated {
    #[topic]
    pub vault: Address,
    pub active: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignUnregistered {
    #[topic]
    pub vault: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deposited {
    #[topic]
    pub vault: Address,
    #[topic]
    pub depositor: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Boosted {
    #[topic]
    pub vault: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transferred {
    #[topic]
    pub vault: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reallocated {
    #[topic]
    pub from_vault: Address,
    #[topic]
    pub to_vault: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerUpdated {
    pub old: Address,
    pub new_addr: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposed {
    pub current: Address,
    pub pending: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminUpdated {
    pub old: Address,
    pub new_addr: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrphanRescued {
    #[topic]
    pub token: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
}
