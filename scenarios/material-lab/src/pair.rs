//! Display model of a free pair: each body's motion and its own material layer.
use crate::{MaterialLabState, scene};
use serde::Serialize;

/// One body and its material layer. Motion is along the +x contact normal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairBody {
    pub material: &'static str,
    pub mass: f32,
    pub position: f32,
    pub velocity: f32,
    pub momentum: f64,
    pub kinetic_energy: f64,
    /// Present compression of the layer: its permanent set plus any elastic
    /// part. A separated layer keeps only the permanent set; collapsed layers
    /// retain their boundary deformation even though their elastic store is gone.
    pub deformation: f64,
    pub permanent_set: f64,
    pub elastic_energy: f64,
    pub damage_work: f64,
    pub viscous_loss: f64,
    pub remaining: f64,
    pub capacity: f64,
    pub failed: bool,
}

/// Both bodies, incoming body first, and the energy ledger they share.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pair {
    pub bodies: [PairBody; 2],
    /// Kinetic energy of both bodies before the first step.
    pub initial_energy: f64,
    /// The solver's retained energy and capacity-charge defects. Never damage.
    pub numerical_defect: f64,
    /// Both elastic releases and any excess plastic work at contact collapse.
    pub collapse_loss: f64,
}
impl Pair {
    pub fn total(&self, value: impl Fn(&PairBody) -> f64) -> f64 {
        self.bodies.iter().map(value).sum()
    }
    /// Motion, stored and spent energy with the defect: the ledger's total,
    /// which stays at `initial_energy`.
    pub fn accounted_energy(&self) -> f64 {
        self.total(|b| b.kinetic_energy + b.elastic_energy + b.damage_work + b.viscous_loss)
            + self.numerical_defect
            + self.collapse_loss
    }
}

fn kinetic(mass: f32, velocity: f32) -> f64 {
    0.5 * f64::from(mass) * f64::from(velocity).powi(2)
}

impl MaterialLabState {
    /// The free pair of a two-material case, read from the physical state.
    pub fn pair(&self) -> Option<Pair> {
        let setup = self.case.free_pair()?;
        // The history exists from the first step; an untouched pair is undeformed.
        let layers = self.world.material_paired_compression_summary();
        let failure = self.paired_failure();
        let bodies = [0, 1].map(|i| {
            let id = scene::body(i as u64);
            let mass = self.world.body_mass(id).unwrap();
            let motion = self.world.motion(id).unwrap();
            let velocity = motion.linear_velocity.x;
            let layer = layers.as_ref().map(|l| &l[i]);
            PairBody {
                material: setup[i].material.name,
                mass,
                position: motion.position.x,
                velocity,
                momentum: f64::from(mass) * f64::from(velocity),
                kinetic_energy: kinetic(mass, velocity),
                deformation: layer.map_or(0.0, |l| l.indentation.max(l.plastic_indentation)),
                permanent_set: layer.map_or(0.0, |l| l.plastic_indentation),
                elastic_energy: layer.map_or(0.0, |l| l.elastic_energy),
                damage_work: layer.map_or(0.0, |l| l.damage_work),
                viscous_loss: layer.map_or(0.0, |l| l.viscous_loss),
                remaining: self
                    .world
                    .loading_material_remaining(self.materials[i].unwrap())
                    .unwrap(),
                capacity: f64::from(scene::pair_region(setup[i]).capacity()),
                failed: failure.as_ref().is_some_and(|f| f.layers[i].failed),
            }
        });
        Some(Pair {
            bodies,
            initial_energy: setup.iter().map(|b| kinetic(b.mass, b.velocity)).sum(),
            numerical_defect: self.summary().charge_error
                + self
                    .world
                    .material_moving_support_summary()
                    .map_or(0.0, |m| m.errors[0]),
            collapse_loss: failure.as_ref().map_or(0.0, |f| f.dissipated()),
        })
    }
}
