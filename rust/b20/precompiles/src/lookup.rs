//! Dynamic lookup for Beryl-native B20 precompiles.

use alloy_evm::precompiles::{DynPrecompile, PrecompileLookup, PrecompilesMap};
use alloy_primitives::Address;

use crate::{
    B20AssetPrecompile, B20Spec, B20StablecoinPrecompile, B20Variant, NoopPrecompileCallObserver,
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

    /// Returns the B20 variant precompile encoded by `address`.
    pub fn lookup(address: &Address) -> Option<DynPrecompile> {
        Self::lookup_with_observer(address, NoopPrecompileCallObserver)
    }

    /// Returns the observed B20 variant precompile encoded by `address`.
    pub fn lookup_with_observer<O>(address: &Address, observer: O) -> Option<DynPrecompile>
    where
        O: PrecompileCallObserver,
    {
        match B20Variant::from_address(*address)? {
            B20Variant::Stablecoin => {
                Some(B20StablecoinPrecompile::create_precompile_with_observer(
                    *address,
                    B20Spec::Beryl,
                    observer,
                ))
            }
            B20Variant::Asset => Some(B20AssetPrecompile::create_precompile_with_observer(
                *address,
                B20Spec::Beryl,
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

    use crate::{B20Variant, BerylLookup};

    #[test]
    fn lookup_only_matches_structurally_valid_b20_addresses() {
        for variant in [B20Variant::Asset, B20Variant::Stablecoin] {
            let address = variant.compute_address(Address::repeat_byte(0x11), [0x22; 32].into()).0;
            assert!(BerylLookup::lookup(&address).is_some());
        }

        assert!(BerylLookup::lookup(&Address::repeat_byte(0x22)).is_none());
    }

    #[test]
    fn lookup_rejects_h20_singletons_legacy_b20_and_unknown_variants() {
        let singletons = [
            address!("0177FF0000000000000000000000000000000000"),
            address!("0177FF0000000000000000000000000000000001"),
            address!("0177FF0000000000000000000000000000000002"),
        ];
        for singleton in singletons {
            assert!(BerylLookup::lookup(&singleton).is_none());
        }

        let legacy_b20 = address!("B200000000000000000000000000000000000000");
        assert!(BerylLookup::lookup(&legacy_b20).is_none());

        let unknown_variant =
            B20Variant::compute_address_for_discriminant(Address::ZERO, 0x02, [0u8; 32].into()).0;
        assert!(BerylLookup::lookup(&unknown_variant).is_none());
    }
}
