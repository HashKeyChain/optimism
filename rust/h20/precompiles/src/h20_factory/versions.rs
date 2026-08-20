//! Version manager for the H20 token factory precompile.
//!
//! This module is the single owner of both version mappings: which version is
//! active at a given hardfork ([`FactoryVersions::from_spec`]), and which
//! concrete implementation backs a version ([`FactoryVersion::implementation`]).
//! Centralizing fork routing here keeps hardfork logic auditable and off the
//! execution path, and lets the dispatcher route calls without ever matching on
//! the version itself.

use crate::H20Spec;

use crate::{Factory, FactoryV1};

/// An activated version of the H20 token factory precompile logic.
///
/// Each variant maps to an immutable implementation via [`Self::implementation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryVersion {
    /// Introduced at Beryl, the factory's activation fork.
    V1,
}

impl FactoryVersion {
    /// Returns the immutable logic implementation for this version.
    pub fn implementation<'l>(self) -> &'l dyn Factory {
        static V1: FactoryV1 = FactoryV1;
        match self {
            Self::V1 => &V1,
        }
    }
}

/// Resolver that selects the factory version active at a given hardfork.
///
/// The version is resolved once per call from the block's active upgrade; there
/// is only ever one active version at a time.
#[derive(Debug, Default, Clone, Copy)]
pub struct FactoryVersions;

impl FactoryVersions {
    /// Returns the version active at `upgrade`, or `None` before the introduction
    /// fork (Beryl), where the factory precompile is not installed at all.
    pub fn from_spec(upgrade: H20Spec) -> Option<FactoryVersion> {
        (upgrade >= H20Spec::Beryl).then_some(FactoryVersion::V1)
    }
}

#[cfg(test)]
mod tests {
    use crate::H20Spec;

    use crate::{FactoryVersion, FactoryVersions};

    #[test]
    fn resolves_none_before_beryl() {
        assert_eq!(FactoryVersions::from_spec(H20Spec::Disabled), None);
    }

    #[test]
    fn resolves_v1_from_beryl() {
        assert_eq!(FactoryVersions::from_spec(H20Spec::Beryl), Some(FactoryVersion::V1));
    }
}
