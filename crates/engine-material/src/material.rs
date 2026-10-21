//! The material law: a small set of named engineering parameters with
//! validated ranges and a stable identity for records.

use crate::MaterialError;
use serde::Serialize;

/// How a material behaves once stress exceeds its strength. The discriminants
/// are part of `Material::identity`, so they stay fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// Fractures: in an impact the stored elastic energy is also lost to
    /// fracture work and nothing springs back.
    Brittle = 0,
    /// Yields at a constant plateau force: elastic energy springs back and
    /// plastic work accumulates against the remaining capacity.
    Ductile = 1,
}

/// Material parameters in world units. Values are simulation calibration,
/// not measured engineering, biological or botanical properties.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Material {
    /// Label for records and tables; not part of the law or its identity.
    pub name: &'static str,
    /// Mass per represented volume, `mu / wu^3`.
    pub density: f32,
    /// Elastic modulus: stress per unit strain, `mu / (wu s^2)`.
    pub stiffness: f32,
    /// Safe stress limit. Below it, loading is fully recoverable.
    pub strength: f32,
    /// Irreversible work the material absorbs per unit volume before it
    /// fails, `mu / (wu s^2)`.
    pub toughness: f32,
    /// Fraction of stored elastic energy lost harmlessly per loading cycle.
    pub damping: f32,
    pub mode: FailureMode,
    /// Fraction of `strength` that remains at zero integrity. At least 0.01,
    /// so exhausted material still carries load.
    pub residual_strength: f32,
}

impl Material {
    pub fn validate(self) -> Result<(), MaterialError> {
        let values = [
            self.density,
            self.stiffness,
            self.strength,
            self.toughness,
            self.damping,
            self.residual_strength,
        ];
        let valid = values.iter().all(|value| value.is_finite())
            && (1.0e-6..=1.0e9).contains(&self.density)
            && (1.0..=1.0e12).contains(&self.stiffness)
            && (1.0e-6..=1.0e12).contains(&self.strength)
            && self.strength <= self.stiffness
            && (1.0e-9..=1.0e15).contains(&self.toughness)
            && (0.0..=0.95).contains(&self.damping)
            && (0.01..=1.0).contains(&self.residual_strength);
        if valid {
            Ok(())
        } else {
            Err(MaterialError::InvalidMaterial)
        }
    }

    /// Strain at the safe limit.
    pub fn yield_strain(self) -> f32 {
        self.strength / self.stiffness
    }

    /// Elastic energy per unit volume at the safe limit,
    /// `strength^2 / (2 stiffness)`. A brittle material's toughness is near it.
    pub fn resilience(self) -> f32 {
        self.strength * self.strength / (2.0 * self.stiffness)
    }

    /// Safe stress of material with the given remaining integrity in 0..1.
    /// Damage softens strength linearly down to the residual fraction.
    pub fn effective_strength(self, integrity: f32) -> f32 {
        let integrity = integrity.clamp(0.0, 1.0);
        self.strength * (self.residual_strength + (1.0 - self.residual_strength) * integrity)
    }

    /// Stable FNV-1a hash of the law parameters, excluding the name.
    pub fn identity(self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for value in [
            self.density,
            self.stiffness,
            self.strength,
            self.toughness,
            self.damping,
            self.residual_strength,
        ] {
            hash = fnv1a(hash, &value.to_bits().to_le_bytes());
        }
        fnv1a(hash, &[self.mode as u8])
    }

    pub fn identity_hex(self) -> String {
        format!("{:016x}", self.identity())
    }
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
