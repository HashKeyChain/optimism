//! Contains the full superchain data.

use crate::L1Config;

use super::ChainList;
use alloy_primitives::map::HashMap;
use hsk_h20_config::{H20Config, H20ConfigError, H20NetworkConfigError, network_h20_config};
use kona_genesis::{ChainConfig, L1ChainConfig, RollupConfig, Superchains};

/// The registry containing all the superchain configurations.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Registry {
    /// The list of chains.
    pub chain_list: ChainList,
    /// Map of chain IDs to their chain configuration.
    pub op_chains: HashMap<u64, ChainConfig>,
    /// Map of chain IDs to their rollup configurations.
    pub rollup_configs: HashMap<u64, RollupConfig>,
    /// Map of l1 chain IDs to their l1 configurations.
    pub l1_configs: HashMap<u64, L1ChainConfig>,
}

impl Registry {
    /// Read the chain list.
    pub fn read_chain_list() -> ChainList {
        let chain_list = include_str!(concat!(env!("KONA_REGISTRY_DIR"), "/chainList.json"));
        serde_json::from_str(chain_list).expect("Failed to read chain list")
    }

    /// Read superchain configs.
    pub fn read_superchain_configs() -> Superchains {
        let superchain_configs = include_str!(concat!(env!("KONA_REGISTRY_DIR"), "/configs.json"));
        serde_json::from_str(superchain_configs).expect("Failed to read superchain configs")
    }

    /// Initialize the superchain configurations from the chain list.
    pub fn from_chain_list() -> Self {
        let chain_list = Self::read_chain_list();
        let superchains = Self::read_superchain_configs();
        let mut op_chains = HashMap::default();
        let mut rollup_configs = HashMap::default();

        for superchain in superchains.superchains {
            for mut chain_config in superchain.chains {
                chain_config.l1_chain_id = superchain.config.l1.chain_id;
                if let Some(a) = &mut chain_config.addresses {
                    a.zero_proof_addresses();
                }
                apply_production_h20_override(&mut chain_config).unwrap_or_else(|error| {
                    panic!(
                        "invalid H20 configuration for L1 chain {} and L2 chain {}: {error}",
                        chain_config.l1_chain_id, chain_config.chain_id
                    )
                });
                let mut rollup = chain_config.as_rollup_config();
                rollup.superchain_config_address = superchain.config.superchain_config_addr;
                rollup_configs.insert(chain_config.chain_id, rollup);
                op_chains.insert(chain_config.chain_id, chain_config);
            }
        }

        Self { chain_list, op_chains, rollup_configs, l1_configs: L1Config::build_l1_configs() }
    }
}

fn apply_production_h20_override(chain_config: &mut ChainConfig) -> Result<(), H20OverrideError> {
    let h20_override = network_h20_config(chain_config.l1_chain_id, chain_config.chain_id)
        .map_err(H20OverrideError::InvalidNetworkConfig)?;
    apply_h20_override(chain_config, h20_override)
}

fn apply_h20_override(
    chain_config: &mut ChainConfig,
    h20_override: Option<H20Config>,
) -> Result<(), H20OverrideError> {
    let registry_config = H20Config::new(chain_config.h20_time, chain_config.h20_activation_admin)
        .map_err(H20OverrideError::InvalidRegistryConfig)?;
    let Some(h20_override) = h20_override else {
        return Ok(());
    };

    if registry_config.is_enabled() && registry_config != h20_override {
        return Err(H20OverrideError::Conflict);
    }
    chain_config.h20_time = h20_override.activation_time();
    chain_config.h20_activation_admin = h20_override.activation_admin();
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum H20OverrideError {
    InvalidRegistryConfig(H20ConfigError),
    InvalidNetworkConfig(H20NetworkConfigError),
    Conflict,
}

impl core::fmt::Display for H20OverrideError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidRegistryConfig(error) => {
                write!(f, "invalid Registry H20 configuration: {error}")
            }
            Self::InvalidNetworkConfig(error) => error.fmt(f),
            Self::Conflict => f.write_str(
                "Registry H20 configuration conflicts with the built-in production override",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_op_hardforks::{
        OP_MAINNET_ISTHMUS_TIMESTAMP, OP_MAINNET_JOVIAN_TIMESTAMP, OP_MAINNET_KARST_TIMESTAMP,
        OP_SEPOLIA_ISTHMUS_TIMESTAMP, OP_SEPOLIA_JOVIAN_TIMESTAMP, OP_SEPOLIA_KARST_TIMESTAMP,
    };
    use alloy_primitives::address;

    const H20_ADMIN: alloy_primitives::Address =
        address!("1111111111111111111111111111111111111111");

    #[test]
    fn test_read_chain_configs() {
        let superchains = Registry::from_chain_list();
        assert!(superchains.chain_list.len() > 1);
        let op_mainnet = superchains.op_chains.get(&10).expect("OP Mainnet config missing");
        assert_eq!(op_mainnet.name, "OP Mainnet");
        assert_eq!(op_mainnet.chain_id, 10);
        assert_eq!(op_mainnet.l1_chain_id, 1);
        assert_eq!(
            op_mainnet.batch_inbox_addr,
            address!("ff00000000000000000000000000000000000010")
        );
        assert!(op_mainnet.governed_by_optimism);
    }

    #[test]
    fn test_read_rollup_configs() {
        let superchains = Registry::from_chain_list();
        assert_eq!(
            *superchains.rollup_configs.get(&10).unwrap(),
            crate::test_utils::OP_MAINNET_CONFIG
        );
    }

    #[test]
    fn test_h20_override_is_injected_before_rollup_conversion() {
        let mut chain_config = ChainConfig { chain_id: 177, l1_chain_id: 1, ..Default::default() };
        let h20_override = H20Config::new(Some(100), Some(H20_ADMIN)).unwrap();

        apply_h20_override(&mut chain_config, Some(h20_override)).unwrap();

        assert_eq!(chain_config.h20_time, Some(100));
        assert_eq!(chain_config.h20_activation_admin, Some(H20_ADMIN));
        assert_eq!(chain_config.as_rollup_config().h20_config().unwrap(), h20_override);
    }

    #[test]
    fn test_h20_override_accepts_equal_registry_config_and_rejects_conflicts() {
        let h20_override = H20Config::new(Some(100), Some(H20_ADMIN)).unwrap();
        let mut equal = ChainConfig {
            h20_time: Some(100),
            h20_activation_admin: Some(H20_ADMIN),
            ..Default::default()
        };
        apply_h20_override(&mut equal, Some(h20_override)).unwrap();

        let mut conflicting = ChainConfig {
            h20_time: Some(101),
            h20_activation_admin: Some(H20_ADMIN),
            ..Default::default()
        };
        assert_eq!(
            apply_h20_override(&mut conflicting, Some(h20_override)),
            Err(H20OverrideError::Conflict)
        );
    }

    #[test]
    fn test_h20_override_rejects_incomplete_registry_config() {
        let mut chain_config = ChainConfig { h20_time: Some(100), ..Default::default() };
        assert_eq!(
            apply_h20_override(&mut chain_config, None),
            Err(H20OverrideError::InvalidRegistryConfig(H20ConfigError::MissingAdmin))
        );
    }

    #[test]
    fn test_isthmus_timestamps() {
        let superchains = Registry::from_chain_list();
        let op_mainnet_config = superchains.rollup_configs.get(&10).unwrap();
        assert_eq!(op_mainnet_config.hardforks.isthmus_time, Some(OP_MAINNET_ISTHMUS_TIMESTAMP));

        let op_sepolia_config = superchains.rollup_configs.get(&11155420).unwrap();
        assert_eq!(op_sepolia_config.hardforks.isthmus_time, Some(OP_SEPOLIA_ISTHMUS_TIMESTAMP));
    }

    #[test]
    fn test_jovian_timestamps() {
        let superchains = Registry::from_chain_list();
        let op_mainnet_config = superchains.rollup_configs.get(&10).unwrap();
        assert_eq!(op_mainnet_config.hardforks.jovian_time, Some(OP_MAINNET_JOVIAN_TIMESTAMP));

        let op_sepolia_config = superchains.rollup_configs.get(&11155420).unwrap();
        assert_eq!(op_sepolia_config.hardforks.jovian_time, Some(OP_SEPOLIA_JOVIAN_TIMESTAMP));
    }

    #[test]
    fn test_karst_timestamps() {
        let superchains = Registry::from_chain_list();
        let op_mainnet_config = superchains.rollup_configs.get(&10).unwrap();
        assert_eq!(op_mainnet_config.hardforks.karst_time, Some(OP_MAINNET_KARST_TIMESTAMP));

        let op_sepolia_config = superchains.rollup_configs.get(&11155420).unwrap();
        assert_eq!(op_sepolia_config.hardforks.karst_time, Some(OP_SEPOLIA_KARST_TIMESTAMP));
    }
}
