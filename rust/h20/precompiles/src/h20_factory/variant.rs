//! H20 token variant address derivation.

use alloy_primitives::{Address, B256, keccak256};
use alloy_sol_types::SolValue;

use crate::{ActivationFeature, IH20Factory};

/// H20 token variant encoded in token address byte `[10]`.
///
/// Discriminant values match the `H20Variant` ABI enum ordinals directly
/// (ASSET=0, STABLECOIN=1), so `uint8(variant)` in Solidity
/// equals the byte written at address position `[10]` with no offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum H20Variant {
    /// Asset H20 token.
    Asset = 0,
    /// Stablecoin H20 token.
    Stablecoin = 1,
}

impl H20Variant {
    /// Two-byte namespace prefix of every H20 token address.
    pub const PREFIX_BYTES: [u8; 2] = [0x01, 0x77];
    /// Byte that distinguishes the singleton namespace from dynamic tokens.
    pub const SINGLETON_DISCRIMINANT: u8 = 0xff;
    /// Number of zero bytes between the namespace and variant.
    pub const ZERO_BYTES: usize = 8;

    /// Variant discriminant for asset H20 tokens.
    pub const ASSET_DISCRIMINANT: u8 = Self::Asset as u8;

    /// Variant discriminant for stablecoin H20 tokens.
    pub const STABLECOIN_DISCRIMINANT: u8 = Self::Stablecoin as u8;

    /// Returns the currently supported creation-parameter version for this variant.
    ///
    /// Each variant owns its version independently so that one variant advancing to v2
    /// does not affect the others.
    pub const fn supported_version(self) -> u8 {
        match self {
            Self::Asset | Self::Stablecoin => 1,
        }
    }

    /// Returns the supported token variant for `variant`, if any.
    pub const fn from_discriminant(variant: u8) -> Option<Self> {
        match variant {
            Self::ASSET_DISCRIMINANT => Some(Self::Asset),
            Self::STABLECOIN_DISCRIMINANT => Some(Self::Stablecoin),
            _ => None,
        }
    }

    /// Returns the supported token variant for an ABI enum value, or `None` for unknown variants.
    pub const fn from_abi(variant: IH20Factory::H20Variant) -> Option<Self> {
        match variant {
            IH20Factory::H20Variant::ASSET => Some(Self::Asset),
            IH20Factory::H20Variant::STABLECOIN => Some(Self::Stablecoin),
            IH20Factory::H20Variant::__Invalid => None,
        }
    }

    /// Returns whether `variant` is supported by this factory.
    pub const fn is_supported_discriminant(variant: u8) -> bool {
        Self::from_discriminant(variant).is_some()
    }

    /// Returns the token variant encoded in `address`, if it has a supported H20 prefix.
    pub fn from_address(address: Address) -> Option<Self> {
        let bytes = address.as_slice();
        if bytes[..2] != Self::PREFIX_BYTES || bytes[2..10] != [0u8; Self::ZERO_BYTES] {
            return None;
        }

        Self::from_discriminant(bytes[10])
    }

    /// Returns whether `address` has the structural H20 token prefix.
    ///
    /// This intentionally does not validate the encoded variant discriminant.
    pub fn has_h20_prefix(address: Address) -> bool {
        let bytes = address.as_slice();
        bytes[..2] == Self::PREFIX_BYTES && bytes[2..10] == [0u8; Self::ZERO_BYTES]
    }

    /// Returns whether `address` belongs to the reserved H20 singleton namespace.
    pub fn is_h20_singleton_address(address: Address) -> bool {
        let bytes = address.as_slice();
        bytes[..2] == Self::PREFIX_BYTES && bytes[2] == Self::SINGLETON_DISCRIMINANT
    }

    /// Returns this variant's ABI discriminant.
    pub const fn discriminant(self) -> u8 {
        self as u8
    }

    /// Returns this variant as the generated ABI enum.
    pub const fn abi(self) -> IH20Factory::H20Variant {
        match self {
            Self::Asset => IH20Factory::H20Variant::ASSET,
            Self::Stablecoin => IH20Factory::H20Variant::STABLECOIN,
        }
    }

    /// Returns the fixed decimal precision for this variant, or `None` for variants (like
    /// `Asset`) where decimals are per-token and supplied at creation time via init params.
    pub const fn decimals(self) -> Option<u8> {
        match self {
            Self::Stablecoin => Some(6),
            Self::Asset => None,
        }
    }

    /// Returns the activation feature that controls creation of this variant.
    pub const fn activation_feature(self) -> ActivationFeature {
        match self {
            Self::Asset => ActivationFeature::H20Asset,
            Self::Stablecoin => ActivationFeature::H20Stablecoin,
        }
    }

    /// Returns the stable metric label for this H20 variant.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Stablecoin => "stablecoin",
        }
    }

    /// Builds this variant's H20 address prefix.
    pub const fn address_prefix(self) -> [u8; 11] {
        [Self::PREFIX_BYTES[0], Self::PREFIX_BYTES[1], 0, 0, 0, 0, 0, 0, 0, 0, self.discriminant()]
    }

    /// Computes this variant's deterministic token address for `creator` and `salt`.
    ///
    /// Returns the address and the 9-byte hash tail embedded in the address.
    pub fn compute_address(self, creator: Address, salt: B256) -> (Address, [u8; 9]) {
        let hash = keccak256((creator, salt).abi_encode());
        self.compute_address_from_hash(hash)
    }

    /// Computes the deterministic token address from a pre-computed `keccak256(creator, salt)` hash.
    ///
    /// Use when the hash is already available (e.g. after charging keccak gas via `ctx.metered_keccak256`)
    /// to avoid re-encoding and re-hashing.
    pub fn compute_address_from_hash(self, hash: B256) -> (Address, [u8; 9]) {
        let mut tail = [0u8; 9];
        tail.copy_from_slice(&hash[..9]);

        let mut addr_bytes = [0u8; 20];
        addr_bytes[..11].copy_from_slice(&self.address_prefix());
        addr_bytes[11..].copy_from_slice(&tail);

        (Address::from(addr_bytes), tail)
    }

    /// Computes a deterministic H20 token address for an ABI discriminant.
    pub fn compute_address_for_discriminant(
        creator: Address,
        variant: u8,
        salt: B256,
    ) -> (Address, [u8; 9]) {
        let hash = keccak256((creator, salt).abi_encode());

        let mut tail = [0u8; 9];
        tail.copy_from_slice(&hash[..9]);

        let mut addr_bytes = [0u8; 20];
        addr_bytes[..2].copy_from_slice(&Self::PREFIX_BYTES);
        addr_bytes[10] = variant;
        addr_bytes[11..].copy_from_slice(&tail);

        (Address::from(addr_bytes), tail)
    }

    /// Returns `true` when `address` has a supported H20 token variant prefix.
    pub fn is_h20_dynamic_address(address: Address) -> bool {
        Self::from_address(address).is_some()
    }

    /// Returns the variant discriminant encoded in `address`, if supported.
    pub fn variant_of(address: Address) -> Option<u8> {
        Self::from_address(address)?;
        Some(address.as_slice()[10])
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256, address};

    use super::H20Variant;

    const CREATOR: Address = address!("1111111111111111111111111111111111111111");
    const SALT: B256 = B256::repeat_byte(0x22);

    #[test]
    fn computed_addresses_use_the_h20_byte_layout() {
        for (variant, discriminant) in [
            (H20Variant::Asset, H20Variant::ASSET_DISCRIMINANT),
            (H20Variant::Stablecoin, H20Variant::STABLECOIN_DISCRIMINANT),
        ] {
            let (address, tail) = variant.compute_address(CREATOR, SALT);
            let bytes = address.as_slice();

            assert_eq!(&bytes[..2], &H20Variant::PREFIX_BYTES);
            assert_eq!(&bytes[2..10], &[0u8; H20Variant::ZERO_BYTES]);
            assert_eq!(bytes[10], discriminant);
            assert_eq!(&bytes[11..], &tail);
            assert_eq!(H20Variant::from_address(address), Some(variant));
        }
    }

    #[test]
    fn discriminant_address_computation_uses_the_same_h20_layout() {
        for variant in [H20Variant::Asset, H20Variant::Stablecoin] {
            assert_eq!(
                H20Variant::compute_address_for_discriminant(CREATOR, variant.discriminant(), SALT,),
                variant.compute_address(CREATOR, SALT)
            );
        }
    }

    #[test]
    fn unknown_variant_has_the_structural_prefix_but_is_not_supported() {
        let (address, _) = H20Variant::compute_address_for_discriminant(CREATOR, 0x02, SALT);

        assert!(H20Variant::has_h20_prefix(address));
        assert_eq!(H20Variant::from_address(address), None);
        assert!(!H20Variant::is_h20_dynamic_address(address));
    }

    #[test]
    fn legacy_h20_and_h20_singleton_addresses_are_not_dynamic_tokens() {
        let legacy_h20 = address!("B200000000000000000000000000000000000000");
        let singletons = [
            address!("0177FF0000000000000000000000000000000000"),
            address!("0177FF0000000000000000000000000000000001"),
            address!("0177FF0000000000000000000000000000000002"),
        ];

        assert!(!H20Variant::has_h20_prefix(legacy_h20));
        assert_eq!(H20Variant::from_address(legacy_h20), None);
        for singleton in singletons {
            assert!(!H20Variant::has_h20_prefix(singleton));
            assert!(H20Variant::is_h20_singleton_address(singleton));
            assert!(!H20Variant::is_h20_dynamic_address(singleton));
            assert_eq!(H20Variant::from_address(singleton), None);
        }
        assert!(!H20Variant::is_h20_singleton_address(address!(
            "0177FE0000000000000000000000000000000000"
        )));
    }
}
