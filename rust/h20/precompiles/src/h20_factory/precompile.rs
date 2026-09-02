//! Precompile entry point for the `H20Factory`.

use crate::H20Spec;
use alloy_evm::precompiles::{DynPrecompile, PrecompilesMap};

use crate::{H20FactoryStorage, PrecompileCallObserver, macros::h20_precompile};

/// Entry point for the `H20Factory` precompile.
#[derive(Debug, Default, Clone, Copy)]
pub struct H20Factory;

impl H20Factory {
    /// Installs the `H20Factory` precompile with an observer, gated to the version active
    /// at `upgrade`.
    pub fn install_with_observer<O>(precompiles: &mut PrecompilesMap, upgrade: H20Spec, observer: O)
    where
        O: PrecompileCallObserver,
    {
        precompiles.extend_precompiles(core::iter::once((
            H20FactoryStorage::ADDRESS,
            Self::precompile_with_observer(upgrade, observer),
        )));
    }

    /// Creates the EVM precompile wrapper for `H20Factory` with an observer, gated to the
    /// version active at `upgrade`.
    pub fn precompile_with_observer<O>(upgrade: H20Spec, observer: O) -> DynPrecompile
    where
        O: PrecompileCallObserver,
    {
        h20_precompile!("H20Factory", |ctx, calldata| {
            let observer = observer.clone();
            H20FactoryStorage::new(ctx).dispatch_with_observer(ctx, &calldata, upgrade, observer)
        })
    }
}
