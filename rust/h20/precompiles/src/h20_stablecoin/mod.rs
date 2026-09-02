//! `H20StablecoinToken` native precompile — stablecoin variant of the H20 token.

mod abi;
pub use abi::IH20Stablecoin;

mod accounting;
pub use accounting::StablecoinAccounting;

mod dispatch;

mod versions;
pub use versions::{StablecoinVersion, StablecoinVersions};

mod logic;
pub use logic::{H20StablecoinToken, Stablecoin, StablecoinV1};

mod precompile;
pub use precompile::H20StablecoinPrecompile;

mod storage;
pub use storage::{H20StablecoinExtensionStorage, H20StablecoinInit, H20StablecoinStorage};
