//! Material quantity with persistent integrity, and beam connections.

use crate::{Beam, Material, MaterialError};
use serde::Serialize;

/// A quantity of one material with its remaining irreversible-work capacity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Region {
    material: Material,
    volume: f32,
    capacity: f32,
    remaining: f32,
}

impl Region {
    /// Full capacity is `toughness * volume`.
    pub fn new(material: Material, volume: f32) -> Result<Self, MaterialError> {
        material.validate()?;
        if !(volume.is_finite() && (1.0e-9..=1.0e12).contains(&volume)) {
            return Err(MaterialError::InvalidRegion);
        }
        let capacity = material.toughness * volume;
        if !(capacity.is_finite() && capacity > 0.0) {
            return Err(MaterialError::InvalidRegion);
        }
        Ok(Self {
            material,
            volume,
            capacity,
            remaining: capacity,
        })
    }

    /// Restore a saved region: full capacity from `material` and `volume`,
    /// with `remaining` of it left, for hosts that persist integrity.
    pub fn with_remaining(
        material: Material,
        volume: f32,
        remaining: f32,
    ) -> Result<Self, MaterialError> {
        let mut region = Self::new(material, volume)?;
        if !(remaining.is_finite() && (0.0..=region.capacity).contains(&remaining)) {
            return Err(MaterialError::InvalidRegion);
        }
        region.remaining = remaining;
        Ok(region)
    }

    pub fn material(&self) -> Material {
        self.material
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Represented mass of the material, `density * volume`.
    pub fn mass(&self) -> f32 {
        self.material.density * self.volume
    }

    pub fn capacity(&self) -> f32 {
        self.capacity
    }

    pub fn remaining(&self) -> f32 {
        self.remaining
    }

    pub fn integrity(&self) -> f32 {
        (self.remaining / self.capacity).clamp(0.0, 1.0)
    }

    /// Remaining integrity on the 0 to 100 scale hosts display. It describes
    /// remaining capacity; it does not set resistance by itself.
    pub fn normalized_health(&self) -> f32 {
        100.0 * self.integrity()
    }

    pub fn failed(&self) -> bool {
        self.remaining <= 0.0
    }

    /// Safe stress at the current integrity.
    pub fn effective_strength(&self) -> f32 {
        self.material.effective_strength(self.integrity())
    }

    /// Charge irreversible work against the remaining capacity; the only path
    /// that reduces integrity. Returns the work that could not be absorbed.
    pub fn absorb(&mut self, work: f32) -> Result<f32, MaterialError> {
        if !(work.is_finite() && work >= 0.0) {
            return Err(MaterialError::InvalidRegion);
        }
        let taken = work.min(self.remaining);
        self.remaining -= taken;
        Ok(work - taken)
    }

    /// Explicit, host-authorized repair of a fraction of the full capacity.
    pub fn repair(&mut self, fraction: f32) -> Result<(), MaterialError> {
        if !(fraction.is_finite() && (0.0..=1.0).contains(&fraction)) {
            return Err(MaterialError::InvalidRegion);
        }
        self.remaining = (self.remaining + fraction * self.capacity).min(self.capacity);
        Ok(())
    }

    /// Partition the material into two regions holding `fraction` and the rest
    /// of the volume, each at the current integrity. No capacity is created.
    pub fn split(self, fraction: f32) -> Result<(Self, Self), MaterialError> {
        if !(fraction.is_finite() && fraction > 0.0 && fraction < 1.0) {
            return Err(MaterialError::InvalidRegion);
        }
        let integrity = self.integrity();
        let part = |volume: f32| -> Result<Self, MaterialError> {
            let mut region = Self::new(self.material, volume)?;
            region.remaining = region.capacity * integrity;
            Ok(region)
        };
        Ok((
            part(self.volume * fraction)?,
            part(self.volume * (1.0 - fraction))?,
        ))
    }
}

/// A structural connection: a beam of one material with its own integrity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Connection {
    beam: Beam,
    region: Region,
}

impl Connection {
    pub fn new(material: Material, beam: Beam) -> Result<Self, MaterialError> {
        beam.validate()?;
        Ok(Self {
            beam,
            region: Region::new(material, beam.volume())?,
        })
    }

    pub fn beam(&self) -> Beam {
        self.beam
    }

    pub fn region(&self) -> &Region {
        &self.region
    }

    pub fn integrity(&self) -> f32 {
        self.region.integrity()
    }

    pub fn failed(&self) -> bool {
        self.region.failed()
    }

    pub fn absorb(&mut self, work: f32) -> Result<f32, MaterialError> {
        self.region.absorb(work)
    }

    pub fn repair(&mut self, fraction: f32) -> Result<(), MaterialError> {
        self.region.repair(fraction)
    }
}
