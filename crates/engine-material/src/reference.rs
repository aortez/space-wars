//! Reference materials in world units. These are calibration defaults for
//! fixtures and tuning, not measured engineering or biological properties.
//! The generated report lists toughness beside resilience: brittle materials
//! stay under four times their resilience, ductile ones sit well above it.

use crate::{FailureMode, Material};

/// Thick vehicle plate: stiff, strong and very tough.
pub const ARMOR: Material = Material {
    name: "armor",
    density: 8.0,
    stiffness: 2.0e6,
    strength: 2.0e4,
    toughness: 1.0e4,
    damping: 0.1,
    mode: FailureMode::Ductile,
    residual_strength: 0.3,
};

/// Light ship skin: weaker than rock, so crashes hurt the ship.
pub const HULL: Material = Material {
    name: "hull",
    density: 2.7,
    stiffness: 7.0e5,
    strength: 6.0e3,
    toughness: 2.0e3,
    damping: 0.15,
    mode: FailureMode::Ductile,
    residual_strength: 0.25,
};

/// Hardened projectile: stronger than plate, so it is what damages plate.
pub const SHELL: Material = Material {
    name: "shell",
    density: 8.0,
    stiffness: 2.0e6,
    strength: 3.0e4,
    toughness: 3.0e3,
    damping: 0.1,
    mode: FailureMode::Ductile,
    residual_strength: 0.5,
};

/// Living or dead wood: brittle, breaks before it dents plate.
pub const WOOD: Material = Material {
    name: "wood",
    density: 0.6,
    stiffness: 2.0e5,
    strength: 3.0e3,
    toughness: 60.0,
    damping: 0.2,
    mode: FailureMode::Brittle,
    residual_strength: 0.2,
};

/// Soft tissue: compliant, weak and heavily damped.
pub const SOFT_TISSUE: Material = Material {
    name: "soft_tissue",
    density: 1.0,
    stiffness: 2.0e3,
    strength: 2.0e2,
    toughness: 60.0,
    damping: 0.5,
    mode: FailureMode::Ductile,
    residual_strength: 0.1,
};

/// Planet and asteroid rock: stiff and brittle, chips rather than dents.
pub const ROCK: Material = Material {
    name: "rock",
    density: 2.5,
    stiffness: 5.0e6,
    strength: 1.0e4,
    toughness: 15.0,
    damping: 0.05,
    mode: FailureMode::Brittle,
    residual_strength: 0.1,
};

pub const ALL_REFERENCE: [Material; 6] = [ARMOR, HULL, ROCK, SHELL, SOFT_TISSUE, WOOD];
