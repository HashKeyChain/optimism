//! Network-scoped H20 production configuration.

use alloy_primitives::Address;

use crate::{H20Config, H20ConfigError};

/// A production H20 configuration keyed by both L1 and L2 chain ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct H20NetworkConfig {
    /// L1 chain ID used by the rollup.
    pub l1_chain_id: u64,
    /// L2 chain ID of the rollup.
    pub l2_chain_id: u64,
    /// First L2 timestamp at which H20 is active.
    pub activation_time: u64,
    /// Static H20 activation administrator.
    pub activation_admin: Address,
}

impl H20NetworkConfig {
    /// Creates a network-scoped H20 configuration.
    pub const fn new(
        l1_chain_id: u64,
        l2_chain_id: u64,
        activation_time: u64,
        activation_admin: Address,
    ) -> Self {
        Self { l1_chain_id, l2_chain_id, activation_time, activation_admin }
    }

    /// Validates and returns the H20 consensus configuration.
    pub fn h20_config(self) -> Result<H20Config, H20ConfigError> {
        H20Config::new(Some(self.activation_time), Some(self.activation_admin))
    }
}

/// Approved HSK production H20 configurations.
///
/// This remains empty until HSK mainnet and testnet parameters are approved. Do not add fixture or
/// placeholder values here.
pub const PRODUCTION_H20_NETWORKS: &[H20NetworkConfig] = &[];

/// Returns the approved H20 configuration for an exact L1/L2 network pair.
pub fn network_h20_config(
    l1_chain_id: u64,
    l2_chain_id: u64,
) -> Result<Option<H20Config>, H20NetworkConfigError> {
    network_h20_config_from(PRODUCTION_H20_NETWORKS, l1_chain_id, l2_chain_id)
}

fn network_h20_config_from(
    networks: &[H20NetworkConfig],
    l1_chain_id: u64,
    l2_chain_id: u64,
) -> Result<Option<H20Config>, H20NetworkConfigError> {
    let mut matched = None;
    for network in networks {
        if network.l1_chain_id != l1_chain_id || network.l2_chain_id != l2_chain_id {
            continue;
        }
        if matched.is_some() {
            return Err(H20NetworkConfigError::DuplicateNetwork { l1_chain_id, l2_chain_id });
        }
        matched = Some(network.h20_config().map_err(H20NetworkConfigError::InvalidConfig)?);
    }
    Ok(matched)
}

/// Invalid built-in H20 network configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H20NetworkConfigError {
    /// A network entry contains an invalid H20 configuration.
    InvalidConfig(H20ConfigError),
    /// More than one entry has the same L1/L2 chain ID pair.
    DuplicateNetwork {
        /// Duplicate L1 chain ID.
        l1_chain_id: u64,
        /// Duplicate L2 chain ID.
        l2_chain_id: u64,
    },
}

impl core::fmt::Display for H20NetworkConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidConfig(error) => write!(f, "invalid H20 network configuration: {error}"),
            Self::DuplicateNetwork { l1_chain_id, l2_chain_id } => write!(
                f,
                "duplicate H20 network configuration for L1 chain {l1_chain_id} and L2 chain \
                 {l2_chain_id}"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for H20NetworkConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    const ADMIN: Address = Address::repeat_byte(0x11);
    const NETWORK: H20NetworkConfig = H20NetworkConfig::new(1, 177, 100, ADMIN);

    #[test]
    fn lookup_requires_exact_l1_and_l2_pair() {
        let config = network_h20_config_from(&[NETWORK], 1, 177).unwrap().unwrap();
        assert_eq!(config.activation_time(), Some(100));
        assert_eq!(config.activation_admin(), Some(ADMIN));
        assert_eq!(network_h20_config_from(&[NETWORK], 2, 177).unwrap(), None);
        assert_eq!(network_h20_config_from(&[NETWORK], 1, 178).unwrap(), None);
    }

    #[test]
    fn lookup_rejects_invalid_or_duplicate_entries() {
        let invalid = H20NetworkConfig::new(1, 177, 100, Address::ZERO);
        assert_eq!(
            network_h20_config_from(&[invalid], 1, 177),
            Err(H20NetworkConfigError::InvalidConfig(H20ConfigError::ZeroAdmin))
        );
        assert_eq!(
            network_h20_config_from(&[NETWORK, NETWORK], 1, 177),
            Err(H20NetworkConfigError::DuplicateNetwork { l1_chain_id: 1, l2_chain_id: 177 })
        );
    }

    #[test]
    fn production_networks_have_no_unapproved_placeholders() {
        assert!(PRODUCTION_H20_NETWORKS.is_empty());
        assert_eq!(network_h20_config(1, 177).unwrap(), None);
    }
}
