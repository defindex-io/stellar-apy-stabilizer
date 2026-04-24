use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    Unauthorized = 100,
    VaultAlreadyRegistered = 110,
    VaultNotRegistered = 111,
    FeeOutOfBounds = 120,
    InvalidFeeBounds = 121,
    NoPendingAdmin = 123,
}
