//! Version manager for the asset H20 precompile.
//!
//! This module is the single owner of both version mappings: which version is
//! active at a given hardfork ([`AssetVersions::from_spec`]), and which
//! concrete implementation backs a version ([`AssetVersion::implementation`]).
//! Centralizing fork routing here keeps hardfork logic auditable and off the
//! execution path, and lets the dispatcher route calls without ever matching on
//! the version itself.

use crate::H20Spec;

use crate::{Asset, AssetAccounting, AssetV1, PolicyAccounting};

/// An activated version of the asset H20 precompile logic.
///
/// Each variant maps to an immutable implementation via [`Self::implementation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetVersion {
    /// Introduced at Beryl, the asset's activation fork.
    V1,
}

impl AssetVersion {
    /// Returns the immutable logic implementation for this version.
    pub fn implementation<'l, S, A>(self) -> &'l dyn Asset<S, A>
    where
        S: AssetAccounting + 'l,
        A: PolicyAccounting + 'l,
    {
        static V1: AssetV1 = AssetV1;
        match self {
            Self::V1 => &V1,
        }
    }
}

/// Resolver that selects the asset version active at a given hardfork.
///
/// The version is resolved once per call from the block's active upgrade; there
/// is only ever one active version at a time.
#[derive(Debug, Default, Clone, Copy)]
pub struct AssetVersions;

impl AssetVersions {
    /// Returns the version active at `upgrade`, or `None` before the introduction
    /// fork (Beryl), where the asset precompile is not installed at all.
    pub fn from_spec(upgrade: H20Spec) -> Option<AssetVersion> {
        (upgrade >= H20Spec::Beryl).then_some(AssetVersion::V1)
    }
}

#[cfg(test)]
mod tests {
    use crate::H20Spec;

    use crate::{AssetVersion, AssetVersions};

    #[test]
    fn resolves_none_before_beryl() {
        assert_eq!(AssetVersions::from_spec(H20Spec::Disabled), None);
    }

    #[test]
    fn resolves_v1_from_beryl() {
        assert_eq!(AssetVersions::from_spec(H20Spec::Beryl), Some(AssetVersion::V1));
    }
}
