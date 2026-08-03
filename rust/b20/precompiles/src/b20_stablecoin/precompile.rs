//! Precompile entry point for the stablecoin B-20 variant.

use crate::B20Spec;
use alloy_evm::precompiles::DynPrecompile;
use alloy_primitives::{Address, Bytes};
use b20_precompile_storage::BasePrecompileError;

use crate::{
    B20StablecoinStorage, B20StablecoinToken, NoopPrecompileCallObserver, PolicyRegistryStorage,
    PolicyVersions, PrecompileCallObserver, macros::base_precompile,
};

/// Entry point for the stablecoin B-20 variant.
///
/// Wraps [`B20StablecoinToken`] dispatch behind a [`DynPrecompile`].
#[derive(Debug)]
pub struct B20StablecoinPrecompile;

impl B20StablecoinPrecompile {
    /// Returns a [`DynPrecompile`] that dispatches to [`B20StablecoinToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile(token_address: Address, upgrade: B20Spec) -> DynPrecompile {
        Self::create_precompile_with_observer(token_address, upgrade, NoopPrecompileCallObserver)
    }

    /// Returns a [`DynPrecompile`] that observes and dispatches to [`B20StablecoinToken`] logic at
    /// `token_address`, gated to the version active at `upgrade`.
    pub fn create_precompile_with_observer<O>(
        token_address: Address,
        upgrade: B20Spec,
        observer: O,
    ) -> DynPrecompile
    where
        O: PrecompileCallObserver,
    {
        base_precompile!(alloc::format!("B20StablecoinToken@{token_address}"), |ctx, calldata| {
            let observer = observer.clone();
            let Some(version) = PolicyVersions::from_spec(upgrade) else {
                return BasePrecompileError::Revert(Bytes::new()).into_precompile_result(0, 0);
            };
            B20StablecoinToken::with_storage_and_policy(
                B20StablecoinStorage::from_address(token_address, ctx),
                PolicyRegistryStorage::new(ctx),
                version,
            )
            .dispatch_with_observer(ctx, &calldata, upgrade, observer)
        })
    }
}
