//! H20 core activation spec.

/// H20 execution state used by the standalone core crate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum H20Spec {
    /// H20 is not active.
    #[default]
    Disabled,
    /// HSK H20 v1 is active.
    Beryl,
}
