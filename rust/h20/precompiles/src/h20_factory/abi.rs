//! ABI definition for the `IH20Factory` interface.

use alloy_sol_types::sol;

sol! {
    #[derive(Debug, PartialEq, Eq)]
    interface IH20Factory {
        // ── Structs ─────────────────────────────────────────────────────────

        enum H20Variant {
            /// Asset H20 token variant.
            ASSET,
            /// Stablecoin H20 token variant.
            STABLECOIN
        }

        struct H20StablecoinCreateParams {
            uint8 version;
            string name;
            string symbol;
            address initialAdmin;
            string currency;
        }

        struct H20AssetCreateParams {
            uint8 version;
            string name;
            string symbol;
            address initialAdmin;
            uint8 decimals;
        }

        // ── Errors ───────────────────────────────────────────────────────────

        /// ETH was sent to a nonpayable factory function.
        error NonPayable();

        /// A token already exists at the address derived from `(variant, msg.sender, salt)`.
        error TokenAlreadyExists(address token);

        /// `variant` is not recognized or is `NONE`.
        error InvalidVariant();

        /// `version` is not supported for the requested variant.
        error UnsupportedVersion(uint8 version, H20Variant variant);

        /// A required string argument was empty.
        /// @param field  Name of the missing field (e.g. `"currency"`).
        error MissingRequiredField(string field);

        /// The stablecoin `currency` field was not on the ISO 4217 fiat allowlist.
        error InvalidCurrency(string code);

        /// The asset `decimals` field was outside the allowed range.
        error InvalidDecimals(uint8 decimals);

        /// One of the post-creation init calls failed.
        error InitCallFailed(uint256 index);

        // ── Events ───────────────────────────────────────────────────────────

        event H20Created(
            address indexed token,
            H20Variant indexed variant,
            string name,
            string symbol,
            uint8 decimals,
            bytes variantParams
        );

        /// ABI-encoded payload for the `variantParams` field of `H20Created`
        /// when variant is `STABLECOIN`.
        struct H20StablecoinEventParams {
            uint8 version;
            string currency;
        }

        // ── Functions ────────────────────────────────────────────────────────

        /// Creates an H20 token of the requested variant at a deterministic address.
        ///
        /// Default tokens start with an unbounded supply cap and the pausable plus mutable-cap
        /// capability bits enabled. Callers configure optional launch state atomically through
        /// `initCalls`, such as minting initial supply, lowering the supply cap, pausing, or setting
        /// metadata.
        function createH20(
            H20Variant variant,
            bytes32 salt,
            bytes calldata params,
            bytes[] calldata initCalls
        ) external returns (address token);

        /// Returns the address a `createH20` call would produce.
        function getH20Address(H20Variant variant, address sender, bytes32 salt) external view returns (address);

        /// Returns `true` if `token` has the H20 dynamic-token address prefix.
        function isH20(address token) external view returns (bool);

        /// Returns `true` if `token` has been initialized by this factory.
        function isH20Initialized(address token) external view returns (bool);
    }
}

impl IH20Factory::IH20FactoryCalls {
    /// Returns the stable metric label for this decoded factory call.
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::createH20(_) => "factory.createH20",
            Self::getH20Address(_) => "factory.getH20Address",
            Self::isH20(_) => "factory.isH20",
            Self::isH20Initialized(_) => "factory.isH20Initialized",
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256, keccak256};
    use alloy_sol_types::{SolCall, SolError, SolEvent};

    use crate::IH20Factory;

    #[test]
    fn h20_factory_abi_selectors_and_topics_are_frozen() {
        fn selector(signature: &str) -> [u8; 4] {
            keccak256(signature.as_bytes())[..4].try_into().unwrap()
        }

        assert_eq!(
            IH20Factory::createH20Call::SELECTOR,
            selector("createH20(uint8,bytes32,bytes,bytes[])")
        );
        assert_eq!(
            IH20Factory::getH20AddressCall::SELECTOR,
            selector("getH20Address(uint8,address,bytes32)")
        );
        assert_eq!(IH20Factory::isH20Call::SELECTOR, selector("isH20(address)"));
        assert_eq!(
            IH20Factory::isH20InitializedCall::SELECTOR,
            selector("isH20Initialized(address)")
        );
        assert_ne!(
            IH20Factory::createH20Call::SELECTOR,
            selector("createB20(uint8,bytes32,bytes,bytes[])")
        );
        assert_eq!(
            IH20Factory::H20Created::SIGNATURE_HASH,
            keccak256("H20Created(address,uint8,string,string,uint8,bytes)")
        );
        assert_eq!(
            IH20Factory::TokenAlreadyExists::SELECTOR,
            selector("TokenAlreadyExists(address)")
        );
    }

    #[test]
    fn factory_call_labels_are_stable() {
        assert_eq!(
            IH20Factory::IH20FactoryCalls::getH20Address(IH20Factory::getH20AddressCall {
                variant: IH20Factory::H20Variant::ASSET,
                sender: Address::ZERO,
                salt: B256::ZERO,
            })
            .as_label(),
            "factory.getH20Address"
        );
    }
}
