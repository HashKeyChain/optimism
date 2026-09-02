//! Precompile entry point for the stablecoin H20 variant.

use crate::H20Spec;
use alloy_evm::precompiles::DynPrecompile;
use alloy_primitives::{Address, Bytes};
use h20_precompile_storage::H20PrecompileError;

use crate::{
    H20StablecoinStorage, H20StablecoinToken, NoopPrecompileCallObserver, PolicyRegistryStorage,
    PolicyVersions, PrecompileCallObserver, macros::h20_precompile,
};

/// Entry point for the stablecoin H20 variant.
///
/// Wraps [`H20StablecoinToken`] dispatch behind a [`DynPrecompile`].
#[derive(Debug)]
pub struct H20StablecoinPrecompile;

impl H20StablecoinPrecompile {
    /// Returns a [`DynPrecompile`] that dispatches to [`H20StablecoinToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile(token_address: Address, upgrade: H20Spec) -> DynPrecompile {
        Self::create_precompile_with_observer(token_address, upgrade, NoopPrecompileCallObserver)
    }

    /// Returns a [`DynPrecompile`] that observes and dispatches to [`H20StablecoinToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile_with_observer<O>(
        token_address: Address,
        upgrade: H20Spec,
        observer: O,
    ) -> DynPrecompile
    where
        O: PrecompileCallObserver,
    {
        h20_precompile!(alloc::format!("H20StablecoinToken@{token_address}"), |ctx, calldata| {
            let observer = observer.clone();
            let Some(version) = PolicyVersions::from_spec(upgrade) else {
                return H20PrecompileError::Revert(Bytes::new()).into_precompile_result(0, 0);
            };
            H20StablecoinToken::with_storage_and_policy(
                H20StablecoinStorage::from_address(token_address, ctx),
                PolicyRegistryStorage::new(ctx),
                version,
            )
            .dispatch_with_observer(ctx, &calldata, upgrade, observer)
        })
    }
}
