//! `H20Factory` native precompile — creates H20 tokens at deterministic prefix-encoded addresses.

mod abi;
pub use abi::IH20Factory;

mod dispatch;

mod logic;
pub use logic::{CommonParams, Factory, FactoryV1, TokenCreateParams};

mod precompile;
pub use precompile::H20Factory;

mod storage;
pub use storage::H20FactoryStorage;

mod variant;
pub use variant::H20Variant;

mod versions;
pub use versions::{FactoryVersion, FactoryVersions};
