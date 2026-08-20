//! Shared business logic for all Base-native token variants.

mod abi;
pub use abi::IH20;

mod core_storage;
pub use core_storage::H20CoreStorage;

mod ops;
pub use ops::{
    H20Guards, H20TokenRole, Burnable, Configurable, Eip712Domain, Mintable, Pausable, PermitArgs,
    Permittable, RoleManaged, Transferable,
};

mod pausable_feature;
pub use pausable_feature::H20PausableFeature;

mod policy_type;
pub use policy_type::H20PolicyType;

#[cfg(any(test, feature = "test-utils"))]
pub(super) mod test_utils;
#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::{
    FakePolicyAccounting, InMemoryTokenAccounting, TestStablecoinToken, TestToken,
};

mod token;
pub use token::Token;

mod token_accounting;
pub use token_accounting::{H20_MAX_SUPPLY_CAP, TokenAccounting};
