use alloy_primitives::{Address, B256, U256, address, b256};
use h20_precompile_macros::contract;
use h20_precompile_storage::Result;

use crate::{H20_MAX_SUPPLY_CAP, H20Variant};

/// Maximum total supply for all newly-created H20 tokens.
const DEFAULT_SUPPLY_CAP: U256 = H20_MAX_SUPPLY_CAP;

/// keccak256(0xef)
const FACTORY_MARKER_CODE_HASH: B256 =
    b256!("309b8896ee4c1ff7ec1966155373dee42663b6b40c3fedc70ba501684848d2a3");

/// The H20 token factory precompile.
#[contract(addr = Self::ADDRESS)]
pub struct H20FactoryStorage {}

impl<'a> H20FactoryStorage<'a> {
    /// Singleton precompile address for the `H20Factory`.
    pub const ADDRESS: Address = address!("0177FF0000000000000000000000000000000000");

    /// Initial supply cap for newly created default H20 tokens.
    pub const DEFAULT_SUPPLY_CAP: U256 = DEFAULT_SUPPLY_CAP;

    /// Returns whether `token` has the structural H20 dynamic-token prefix.
    ///
    /// This includes reserved or future variant discriminants in the H20 dynamic address range.
    pub fn is_h20(&self, token: Address) -> Result<bool> {
        Ok(H20Variant::has_h20_prefix(token))
    }

    /// Returns whether `token` is an H20 address that has been initialized by this factory.
    pub fn is_h20_initialized(&self, token: Address) -> Result<bool> {
        if !H20Variant::has_h20_prefix(token) {
            return Ok(false);
        }
        self.storage.with_account_info(token, |info| Ok(info.code_hash == FACTORY_MARKER_CODE_HASH))
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256, Bytes, address, keccak256};
    use h20_precompile_storage::{HashMapStorageProvider, StorageCtx};
    use revm::state::Bytecode;

    use super::FACTORY_MARKER_CODE_HASH;
    use crate::{H20FactoryStorage, H20Variant};

    #[test]
    fn factory_address_matches_canonical_precompile_address() {
        assert_eq!(
            H20FactoryStorage::ADDRESS,
            address!("0177FF0000000000000000000000000000000000")
        );
    }

    #[test]
    fn test_token_variant_compute_address_encodes_variant_and_hash_tail() {
        let creator = Address::repeat_byte(0x11);
        let salt = B256::repeat_byte(0x22);
        let (addr, tail) = H20Variant::Asset.compute_address(creator, salt);

        assert_eq!(addr.as_slice()[11..], tail);
        assert!(H20Variant::is_h20_dynamic_address(addr));
        assert_eq!(H20Variant::from_address(addr), Some(H20Variant::Asset));
    }

    #[test]
    fn test_address_derivation_uses_variant() {
        let creator = Address::repeat_byte(0x11);
        let salt = B256::repeat_byte(0x33);
        let (asset_token, _) = H20Variant::Asset.compute_address(creator, salt);
        let (stablecoin, _) = H20Variant::Stablecoin.compute_address(creator, salt);

        assert_ne!(asset_token, stablecoin);
        assert_eq!(H20Variant::from_address(asset_token), Some(H20Variant::Asset));
        assert_eq!(H20Variant::from_address(stablecoin), Some(H20Variant::Stablecoin));
    }

    #[test]
    fn test_supported_variants_are_h20_prefixes() {
        let creator = Address::repeat_byte(0x11);
        let salt = B256::repeat_byte(0x44);
        let (asset, _) = H20Variant::compute_address_for_discriminant(creator, 0, salt);
        let (stablecoin, _) = H20Variant::compute_address_for_discriminant(creator, 1, salt);

        assert!(H20Variant::is_supported_discriminant(0));
        assert!(H20Variant::is_supported_discriminant(1));
        assert!(!H20Variant::is_supported_discriminant(2));
        assert!(H20Variant::is_h20_dynamic_address(asset));
        assert!(H20Variant::is_h20_dynamic_address(stablecoin));
        assert_eq!(H20Variant::from_address(asset), Some(H20Variant::Asset));
        assert_eq!(H20Variant::from_address(stablecoin), Some(H20Variant::Stablecoin));
    }

    #[test]
    fn test_abi_enum_ordinals_match_solidity() {
        assert_eq!(H20Variant::ASSET_DISCRIMINANT, 0);
        assert_eq!(H20Variant::STABLECOIN_DISCRIMINANT, 1);
        assert_eq!(H20Variant::Asset.discriminant(), 0);
        assert_eq!(H20Variant::Stablecoin.discriminant(), 1);
    }

    #[test]
    fn test_is_h20_accepts_future_structural_prefixes() {
        let mut storage = HashMapStorageProvider::new(1);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0x13);
        let (future_variant, _) = H20Variant::compute_address_for_discriminant(caller, 0xff, salt);

        StorageCtx::enter(&mut storage, |ctx| {
            let factory = H20FactoryStorage::new(ctx);
            assert!(factory.is_h20(future_variant).unwrap());
            assert_eq!(H20Variant::from_address(future_variant), None);
        });
    }

    #[test]
    fn test_is_h20_false_for_non_prefix_address() {
        let mut storage = HashMapStorageProvider::new(1);
        let random_addr = Address::repeat_byte(0x42);

        StorageCtx::enter(&mut storage, |ctx| {
            let factory = H20FactoryStorage::new(ctx);
            assert!(!factory.is_h20(random_addr).unwrap());
        });
    }

    #[test]
    fn test_factory_marker_code_hash_constant_matches_keccak256_of_marker_byte() {
        assert_eq!(
            FACTORY_MARKER_CODE_HASH,
            keccak256([0xef_u8]),
            "FACTORY_MARKER_CODE_HASH must equal keccak256([0xef])"
        );
    }

    #[test]
    fn test_is_h20_initialized_rejects_arbitrary_code_at_h20_prefix_address() {
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0x22);
        let (addr, _) = H20Variant::Asset.compute_address(caller, salt);
        let mut storage = HashMapStorageProvider::new(1);

        StorageCtx::enter(&mut storage, |ctx| {
            ctx.set_code(addr, Bytecode::new_legacy(Bytes::from_static(&[0x60, 0x00]))).unwrap();

            let factory = H20FactoryStorage::new(ctx);
            assert!(factory.is_h20(addr).unwrap(), "address must have H20 prefix");
            assert!(
                !factory.is_h20_initialized(addr).unwrap(),
                "arbitrary code at H20-prefix address must not be reported as factory-initialized"
            );
        });
    }

    #[test]
    fn variant_supported_versions_are_nonzero() {
        // Each variant has its own match arm in supported_version() so adding a new
        // variant without an explicit version is a compile error, preventing silent
        // constant sharing.
        assert!(H20Variant::Stablecoin.supported_version() > 0);
        assert!(H20Variant::Asset.supported_version() > 0);
    }
}
