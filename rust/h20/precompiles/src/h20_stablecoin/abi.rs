//! ABI definitions for the stablecoin H20 variant.
//!
//! [`IH20Stablecoin`] defines only the stablecoin-specific extension.
//! All inherited selectors come from [`crate::IH20`] defined in `h20/abi.rs`.

use alloy_sol_types::sol;

sol! {
    #[derive(Debug, PartialEq, Eq)]
    interface IH20Stablecoin {
        function currency() external view returns (string);
    }
}

impl IH20Stablecoin::IH20StablecoinCalls {
    /// Returns the stable label for this decoded stablecoin H20 call.
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::currency(_) => "precompile-h20-stablecoin-currency",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::IH20Stablecoin;

    #[test]
    fn stablecoin_call_labels_are_stable() {
        assert_eq!(
            IH20Stablecoin::IH20StablecoinCalls::currency(IH20Stablecoin::currencyCall {})
                .as_label(),
            "precompile-h20-stablecoin-currency"
        );
    }
}
