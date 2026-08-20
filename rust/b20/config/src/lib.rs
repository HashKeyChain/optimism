#![doc = "Consensus configuration for HSK Base Beryl B20 v1."]
#![cfg_attr(not(feature = "std"), no_std)]

use alloy_primitives::Address;

/// Consensus configuration for Base Beryl B20 v1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct H20Config {
    /// First L2 block timestamp at which B20 is active.
    activation_time: Option<u64>,
    /// Static Beryl `ActivationRegistry` administrator.
    activation_admin: Option<Address>,
}

impl H20Config {
    /// Disabled B20 configuration.
    pub const DISABLED: Self = Self { activation_time: None, activation_admin: None };

    /// Creates and validates a B20 configuration.
    pub fn new(
        activation_time: Option<u64>,
        activation_admin: Option<Address>,
    ) -> Result<Self, H20ConfigError> {
        match (activation_time, activation_admin) {
            (None, None) => Ok(Self::DISABLED),
            (Some(activation_time), Some(activation_admin)) => {
                if activation_admin.is_zero() {
                    return Err(H20ConfigError::ZeroAdmin);
                }
                Ok(Self {
                    activation_time: Some(activation_time),
                    activation_admin: Some(activation_admin),
                })
            }
            (Some(_), None) => Err(H20ConfigError::MissingAdmin),
            (None, Some(_)) => Err(H20ConfigError::MissingActivationTime),
        }
    }

    /// Returns the configured activation timestamp.
    pub const fn activation_time(self) -> Option<u64> {
        self.activation_time
    }

    /// Returns the static `ActivationRegistry` administrator.
    pub const fn activation_admin(self) -> Option<Address> {
        self.activation_admin
    }

    /// Returns true if B20 is configured.
    pub const fn is_enabled(self) -> bool {
        self.activation_time.is_some()
    }

    /// Returns true if B20 is active at `timestamp`.
    pub const fn is_active_at(self, timestamp: u64) -> bool {
        match self.activation_time {
            Some(activation_time) => timestamp >= activation_time,
            None => false,
        }
    }
}

/// Invalid B20 consensus configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H20ConfigError {
    /// An activation timestamp was configured without an administrator.
    MissingAdmin,
    /// An administrator was configured without an activation timestamp.
    MissingActivationTime,
    /// The configured administrator is the zero address.
    ZeroAdmin,
}

impl core::fmt::Display for H20ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::MissingAdmin => "h20Time requires h20ActivationAdmin",
            Self::MissingActivationTime => "h20ActivationAdmin requires h20Time",
            Self::ZeroAdmin => "h20ActivationAdmin must not be the zero address",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for H20ConfigError {}

#[cfg(test)]
mod tests {
    use alloy_primitives::Address;

    use super::{H20Config, H20ConfigError};

    #[test]
    fn disabled_configuration_is_never_active() {
        assert!(!H20Config::DISABLED.is_active_at(u64::MAX));
    }

    #[test]
    fn activation_is_inclusive() {
        let config = H20Config::new(Some(100), Some(Address::repeat_byte(0x11))).unwrap();
        assert!(!config.is_active_at(99));
        assert!(config.is_active_at(100));
        assert!(config.is_active_at(101));
    }

    #[test]
    fn incomplete_or_zero_admin_configuration_is_rejected() {
        assert_eq!(H20Config::new(Some(1), None), Err(H20ConfigError::MissingAdmin));
        assert_eq!(
            H20Config::new(None, Some(Address::repeat_byte(0x11))),
            Err(H20ConfigError::MissingActivationTime)
        );
        assert_eq!(H20Config::new(Some(1), Some(Address::ZERO)), Err(H20ConfigError::ZeroAdmin));
    }
}
