//! B20 core activation spec.

/// B20 execution state used by the standalone core crate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum B20Spec {
    /// B20 is not active.
    #[default]
    Disabled,
    /// Base Beryl B20 v1 is active.
    Beryl,
}
