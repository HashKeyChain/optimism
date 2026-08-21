//! `H20AssetToken` native precompile — asset variant of the H20 token.

mod abi;
pub use abi::IH20Asset;

mod accounting;
pub use accounting::AssetAccounting;

mod dispatch;

mod versions;
pub use versions::{AssetVersion, AssetVersions};

mod logic;
pub use logic::{Asset, AssetV1, H20AssetToken};

mod precompile;
pub use precompile::H20AssetPrecompile;

mod storage;
pub use storage::{H20AssetExtensionStorage, H20AssetInit, H20AssetStorage};
