//! Precompile entry point for the asset B-20 variant.

use crate::H20Spec;
use alloy_evm::precompiles::DynPrecompile;
use alloy_primitives::{Address, Bytes};
use h20_precompile_storage::BasePrecompileError;

use crate::{
    H20AssetStorage, H20AssetToken, NoopPrecompileCallObserver, PolicyRegistryStorage,
    PolicyVersions, PrecompileCallObserver, macros::base_precompile,
};

/// Entry point for the asset B-20 token precompile.
///
/// Wraps [`H20AssetToken`] dispatch behind a [`DynPrecompile`] for
/// registration in a [`PrecompilesMap`].
#[derive(Debug)]
pub struct H20AssetPrecompile;

impl H20AssetPrecompile {
    /// Returns a [`DynPrecompile`] that dispatches to [`H20AssetToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile(token_address: Address, upgrade: H20Spec) -> DynPrecompile {
        Self::create_precompile_with_observer(token_address, upgrade, NoopPrecompileCallObserver)
    }

    /// Returns a [`DynPrecompile`] that observes and dispatches to [`H20AssetToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile_with_observer<O>(
        token_address: Address,
        upgrade: H20Spec,
        observer: O,
    ) -> DynPrecompile
    where
        O: PrecompileCallObserver,
    {
        base_precompile!(alloc::format!("H20AssetToken@{token_address}"), |ctx, calldata| {
            let observer = observer.clone();
            let Some(version) = PolicyVersions::from_spec(upgrade) else {
                return BasePrecompileError::Revert(Bytes::new()).into_precompile_result(0, 0);
            };
            H20AssetToken::with_storage_and_policy(
                H20AssetStorage::from_address(token_address, ctx),
                PolicyRegistryStorage::new(ctx),
                version,
            )
            .dispatch_with_observer(ctx, &calldata, upgrade, observer)
        })
    }
}
