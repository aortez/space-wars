//! Bounded material strength, deformation and accumulated-failure model shared
//! by physical actors, wood, tissue and structures. This crate is pure
//! arithmetic: it owns no rigid-body engine, scenario state, rendering or
//! clock. Hosts measure loads and keep regions; this crate decides what a
//! measured load does to a material and accounts each event's work once.
//!
//! Units are world length `wu`, mass `mu` and seconds. Stress and energy per
//! volume share `mu / (wu s^2)`. Geometry is 2D with a represented thickness,
//! as in `engine-water` and `engine-soil`. See `docs/design/material-damage.md`.

// Evidence scaffolding for the tests and the example; its helpers panic, so it
// is not host API.
#[doc(hidden)]
pub mod fixtures;
mod geometry;
mod impact;
mod ledger;
mod material;
mod reference;
mod region;
mod sustained;

#[cfg(test)]
mod tests;

pub use geometry::{
    Beam, ContactBody, ContactGeometry, DEPTH_WIDTH_FRACTION, PATCH_WIDTH_FRACTION, Section,
    closing_speed, deformation_depth, effective_mass, lever, patch_width_from_radii,
    point_velocity,
};
pub use impact::{Impact, impact, yield_speed};
pub use ledger::{LEDGER_TOLERANCE, Outcome, WorkLedger};
pub use material::{FailureMode, Material};
pub use reference::{ALL_REFERENCE, ARMOR, HULL, ROCK, SHELL, SOFT_TISSUE, WOOD};
pub use region::{Connection, Region};
pub use sustained::{CRUSH_TIME_CONSTANT, ConnectionLoad, SustainedLoad, connection, sustained};

/// Identity of the law implemented here. Records carry it beside each
/// material's parameter identity so evidence names its configuration.
pub const LAW_VERSION: &str = "material-law-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialError {
    InvalidMaterial,
    InvalidGeometry,
    InvalidLoad,
    InvalidRegion,
    /// Every input is in range, but their magnitudes together overflow `f32`.
    Overflow,
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidMaterial => "material parameter outside its documented range",
            Self::InvalidGeometry => "contact or beam geometry is not finite and positive",
            Self::InvalidLoad => "load is not finite, is negative, or has no movable body",
            Self::InvalidRegion => "region volume, work or repair fraction is invalid",
            Self::Overflow => "inputs are in range but together overflow f32 arithmetic",
        })
    }
}

impl std::error::Error for MaterialError {}
