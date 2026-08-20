//! Shared harness for the B-20 precompile golden test suites.
//!
//! The four golden integration tests (`h20_asset_v1_golden`, `h20_stablecoin_v1_golden`,
//! `h20_factory_v1_golden`, `h20_policy_v1_golden`) each compile this module independently,
//! so not every binary uses every item. The fixtures and helpers live in [`utils`]; this
//! module only re-exports them.
#![allow(dead_code, unreachable_pub)]

mod utils;
pub use utils::*;
