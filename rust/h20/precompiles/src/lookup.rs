//! Dynamic lookup for Beryl-native H20 precompiles.

use alloy_evm::precompiles::{DynPrecompile, PrecompileLookup, PrecompilesMap};
use alloy_primitives::Address;

use crate::{
    H20AssetPrecompile, H20Spec, H20StablecoinPrecompile, H20Variant, NoopPrecompileCallObserver,
    PrecompileCallObserver,
};

/// Dynamic precompile lookup installed when Beryl is active.
#[derive(Debug, Default, Clone, Copy)]
pub struct BerylLookup;

impl BerylLookup {
    /// Installs the Beryl dynamic precompile lookup.
    pub fn install(precompiles: &mut PrecompilesMap) {
        Self::install_with_observer(precompiles, NoopPrecompileCallObserver);
    }

    /// Installs the Beryl dynamic precompile lookup with an observer.
    pub fn install_with_observer<O>(precompiles: &mut PrecompilesMap, observer: O)
    where
        O: PrecompileCallObserver,
    {
        precompiles.set_precompile_lookup(BerylLookupWithObserver::new(observer));
    }

    /// Returns the H20 variant precompile encoded by `address`.
    pub fn lookup(address: &Address) -> Option<DynPrecompile> {
        Self::lookup_with_observer(address, NoopPrecompileCallObserver)
    }

    /// Returns the observed H20 variant precompile encoded by `address`.
    pub fn lookup_with_observer<O>(address: &Address, observer: O) -> Option<DynPrecompile>
    where
        O: PrecompileCallObserver,
    {
        match H20Variant::from_address(*address)? {
            H20Variant::Stablecoin => {
                Some(H20StablecoinPrecompile::create_precompile_with_observer(
                    *address,
                    H20Spec::Beryl,
                    observer,
                ))
            }
            H20Variant::Asset => Some(H20AssetPrecompile::create_precompile_with_observer(
                *address,
                H20Spec::Beryl,
                observer,
            )),
        }
    }
}

/// Dynamic Beryl precompile lookup with an observer.
#[derive(Debug, Clone)]
pub struct BerylLookupWithObserver<O> {
    observer: O,
}

impl<O> BerylLookupWithObserver<O> {
    /// Creates a Beryl lookup with `observer`.
    pub const fn new(observer: O) -> Self {
        Self { observer }
    }
}

impl<O> PrecompileLookup for BerylLookupWithObserver<O>
where
    O: PrecompileCallObserver,
{
    fn lookup(&self, address: &Address) -> Option<DynPrecompile> {
        BerylLookup::lookup_with_observer(address, self.observer.clone())
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, address};

    use crate::{H20Variant, BerylLookup};

    #[test]
    fn lookup_only_matches_structurally_valid_h20_addresses() {
        for variant in [H20Variant::Asset, H20Variant::Stablecoin] {
            let address = variant.compute_address(Address::repeat_byte(0x11), [0x22; 32].into()).0;
            assert!(BerylLookup::lookup(&address).is_some());
        }

        assert!(BerylLookup::lookup(&Address::repeat_byte(0x22)).is_none());
    }

    #[test]
    fn lookup_rejects_h20_singletons_legacy_b2_and_unknown_variants() {
        let singletons = [
            address!("0177FF0000000000000000000000000000000000"),
            address!("0177FF0000000000000000000000000000000001"),
            address!("0177FF0000000000000000000000000000000002"),
        ];
        for singleton in singletons {
            assert!(BerylLookup::lookup(&singleton).is_none());
        }

        let legacy_b2_address = address!("B200000000000000000000000000000000000000");
        assert!(BerylLookup::lookup(&legacy_b2_address).is_none());

        let unknown_variant =
            H20Variant::compute_address_for_discriminant(Address::ZERO, 0x02, [0u8; 32].into()).0;
        assert!(BerylLookup::lookup(&unknown_variant).is_none());
    }
}
