//! EVM storage adapter for the stablecoin H20 variant.
#![allow(clippy::collection_is_never_read)]

use alloc::string::String;

use alloy_primitives::{Address, U256};
use h20_precompile_macros::{StablecoinAccounting, Storable, TokenAccounting, contract};
use h20_precompile_storage::{H20PrecompileError, Handler, Result, StorageCtx};

use crate::{H20CoreStorage, IH20Factory};

/// Stablecoin-specific H20 storage rooted at the `hsk.h20.stablecoin` ERC-7201 namespace.
#[allow(clippy::collection_is_never_read)]
#[derive(Debug, Clone, Storable)]
#[namespace("hsk.h20.stablecoin")]
pub struct H20StablecoinExtensionStorage {
    /// Stablecoin currency identifier.
    #[accessor]
    #[mutator]
    pub currency: String, // offset 0
}

/// EVM-backed storage for a stablecoin H20 token.
#[contract]
#[derive(TokenAccounting, StablecoinAccounting)]
pub struct H20StablecoinStorage {
    pub h20: H20CoreStorage,
    pub stablecoin: H20StablecoinExtensionStorage,
}

/// Creation-time parameters for a stablecoin H20 token.
///
/// Passed to [`H20StablecoinStorage::initialize`] to write all fields atomically.
#[derive(Debug)]
pub struct H20StablecoinInit {
    /// ERC-20 token name.
    pub name: String,
    /// ERC-20 token symbol.
    pub symbol: String,
    /// Maximum total supply.
    pub supply_cap: U256,
    /// ISO 4217 fiat currency code (e.g. `"USD"`).
    pub currency: String,
}

impl<'a> H20StablecoinStorage<'a> {
    /// Creates a `H20StablecoinStorage` instance targeting `addr`.
    pub fn from_address(addr: Address, storage: StorageCtx<'a>) -> Self {
        Self::__new(addr, storage)
    }

    /// Writes all creation-time fields atomically.
    ///
    /// Validates that `currency` is non-empty and contains only `A-Z` characters
    /// before writing anything; reverts `MissingRequiredField` for empty and
    /// `InvalidCurrency` for non-A-Z values.
    pub fn initialize(&mut self, init: H20StablecoinInit) -> Result<()> {
        if init.currency.is_empty() {
            return Err(H20PrecompileError::revert(IH20Factory::MissingRequiredField {
                field: String::from("currency"),
            }));
        }
        if !init.currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(H20PrecompileError::revert(IH20Factory::InvalidCurrency {
                code: init.currency,
            }));
        }
        self.h20.name.write(init.name)?;
        self.h20.symbol.write(init.symbol)?;
        self.h20.supply_cap.write(init.supply_cap)?;
        self.stablecoin.currency.write(init.currency)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use alloy_primitives::{Address, U256, address, uint};
    use h20_precompile_storage::{Handler, StorableType, StorageCtx, setup_storage};

    use crate::{
        H20CoreStorage, H20StablecoinExtensionStorage, H20StablecoinStorage,
        h20_stablecoin::storage::{__packing_h20_stablecoin_extension_storage, slots},
    };

    const TOKEN: Address = address!("000000000000000000000000000000000000b022");
    const H20_ROOT: U256 =
        uint!(0x79a254405ca90aaaef001c57bda9b239cd737f68ab02f6809b70a3d701ac6100_U256);
    const STABLECOIN_ROOT: U256 =
        uint!(0x1e31a6aa9c13d9ecddf6866fc0c40873aefdceac9965b8b623b763b913725a00_U256);

    #[test]
    fn stablecoin_namespaces_match_hsk_roots() {
        assert_eq!(<H20CoreStorage as StorableType>::STORAGE_NAMESPACE_ROOT, H20_ROOT);
        assert_eq!(
            <H20StablecoinExtensionStorage as StorableType>::STORAGE_NAMESPACE_ID,
            "hsk.h20.stablecoin"
        );
        assert_eq!(
            <H20StablecoinExtensionStorage as StorableType>::STORAGE_NAMESPACE_ROOT,
            STABLECOIN_ROOT
        );

        assert_eq!(slots::H20, H20_ROOT);
        assert_eq!(slots::STABLECOIN, STABLECOIN_ROOT);
        assert_eq!(__packing_h20_stablecoin_extension_storage::CURRENCY_LOC.offset_slots, 0);
    }

    #[test]
    fn stablecoin_currency_is_rooted_at_extension_namespace() {
        let (mut storage, _) = setup_storage();

        StorageCtx::enter(&mut storage, |ctx| {
            let mut token = H20StablecoinStorage::from_address(TOKEN, ctx);
            token.h20.name.write(String::from("Stablecoin")).unwrap();
            token.stablecoin.currency.write(String::from("USD")).unwrap();

            assert_eq!(ctx.sload(TOKEN, H20_ROOT).unwrap(), short_string_word("Stablecoin"));
            assert_eq!(ctx.sload(TOKEN, STABLECOIN_ROOT).unwrap(), short_string_word("USD"));
        });
    }

    fn short_string_word(value: &str) -> U256 {
        let mut word = [0u8; 32];
        word[..value.len()].copy_from_slice(value.as_bytes());
        word[31] = (value.len() * 2) as u8;
        U256::from_be_bytes(word)
    }
}
