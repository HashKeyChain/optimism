//! Pause-bit helpers for H20 tokens.

use alloy_primitives::U256;
use h20_precompile_storage::{H20PrecompileError, Result};

use crate::IH20;

/// Helpers for mapping H20 pausable features into storage bits.
#[derive(Debug, Clone, Copy)]
pub struct H20PausableFeature;

impl H20PausableFeature {
    /// Returns an enum-conversion panic when `feature` is outside the H20 pause enum.
    pub const fn ensure_valid(feature: IH20::PausableFeature) -> Result<()> {
        match feature {
            IH20::PausableFeature::TRANSFER |
            IH20::PausableFeature::MINT |
            IH20::PausableFeature::BURN => Ok(()),
            IH20::PausableFeature::__Invalid => Err(H20PrecompileError::enum_conversion_error()),
        }
    }

    /// Returns the storage bit for a pausable feature.
    pub fn mask(feature: IH20::PausableFeature) -> U256 {
        U256::ONE.checked_shl(usize::from(feature as u8)).unwrap_or(U256::ZERO)
    }
}
