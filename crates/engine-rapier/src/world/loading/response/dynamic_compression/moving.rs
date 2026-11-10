//! Two free translating bodies sharing one unilateral material history.
use super::*;

/// Laboratory-frame balances for a moving support, in platen/support order.
/// The ordinary compression summary reports relative motion and reduced-mass
/// kinetic energy. These totals additionally account for common translation.
#[derive(Debug, Clone, PartialEq)]
pub struct MovingSupportSummary {
    pub masses: [f32; 2],
    pub positions: [f64; 2],
    pub velocities: [f32; 2],
    pub kinetic_energy: [f64; 2],
    pub momentum: [f64; 2],
    /// Total energy, platen/support impulse, platen/support position defects.
    pub errors: [f64; 5],
    pub absolute_errors: [f64; 5],
    /// Fixed scales; each absolute error is bounded by 1e-5 of its scale.
    pub scales: [f64; 5],
}

/// Retained motion at collapse, in platen/support order along the contact normal.
/// A public step may continue past this boundary; these values do not advance.
#[derive(Debug, Clone, PartialEq)]
pub struct MovingSupportFailure {
    pub failure: CompressionFailure,
    pub positions: [f64; 2],
    pub velocities: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Motion {
    pub positions: [f64; 2],
    pub velocities: [f32; 2],
}
impl Motion {
    pub fn read(world: &PhysicsWorld, scene: &Scene) -> Result<Self, R> {
        let n = Vector::new(scene.plates.normal[0], scene.plates.normal[1]);
        let mut result = Self {
            positions: [0.0; 2],
            velocities: [0.0; 2],
        };
        for (i, id) in scene.plates.bodies.into_iter().enumerate() {
            let b = &world.raw.bodies[world.body_handle(id).ok_or(R::UnsupportedScene)?];
            result.positions[i] = f64::from(n.x) * f64::from(b.translation().x)
                + f64::from(n.y) * f64::from(b.translation().y);
            result.velocities[i] = n.dot(b.linvel());
        }
        Ok(result)
    }
    pub fn relative_velocity(&self) -> f64 {
        f64::from(self.velocities[0]) - f64::from(self.velocities[1])
    }
    fn indentation(&self) -> f64 {
        self.positions[0] - self.positions[1] + 1.0
    }
    fn valid(&self) -> bool {
        self.positions
            .iter()
            .all(|x| x.is_finite() && x.abs() <= 18.0 && f64::from(*x as f32) == *x)
            && self
                .velocities
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 64.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct MovingHistory {
    masses: [f32; 2],
    pub initial: Motion,
    pub current: Motion,
    integrated_velocity: [f64; 2],
    errors: [f64; 5],
    absolute_errors: [f64; 5],
}
impl MovingHistory {
    pub fn new(world: &PhysicsWorld, scene: &Scene) -> Result<Option<Self>, R> {
        if world.body_kind(scene.plates.bodies[1]) != Some(BodyKind::Dynamic) {
            return Ok(None);
        }
        let motion = Motion::read(world, scene)?;
        Ok(Some(Self {
            masses: [scene.mass, Scene::mass(world, scene.plates.bodies[1])?],
            initial: motion.clone(),
            current: motion,
            integrated_velocity: [0.0; 2],
            errors: [0.0; 5],
            absolute_errors: [0.0; 5],
        }))
    }
    pub fn effective_mass(&self) -> f64 {
        let [a, b] = self.masses.map(f64::from);
        a * b / (a + b)
    }
    fn weighted(&self, values: [f64; 2]) -> f64 {
        let [a, b] = self.masses.map(f64::from);
        (a * values[0] + b * values[1]) / (a + b)
    }
    fn energy(&self, motion: &Motion) -> [f64; 2] {
        std::array::from_fn(|i| {
            0.5 * f64::from(self.masses[i]) * f64::from(motion.velocities[i]).powi(2)
        })
    }
    fn scales(&self, curve: &MaterialCurve) -> [f64; 5] {
        let e = curve.y * curve.depth;
        [
            e,
            libm::sqrt(2.0 * f64::from(self.masses[0]) * e),
            libm::sqrt(2.0 * f64::from(self.masses[1]) * e),
            curve.depth,
            curve.depth,
        ]
    }
    pub fn midpoint(&self, relative: f64) -> [f32; 2] {
        let common = self.weighted(self.current.velocities.map(f64::from));
        let [a, b] = self.masses.map(f64::from);
        [
            (common + b / (a + b) * relative) as f32,
            (common - a / (a + b) * relative) as f32,
        ]
    }
    pub fn endpoint(&self, impulse: f64) -> [f32; 2] {
        std::array::from_fn(|i| {
            (f64::from(self.current.velocities[i])
                + if i == 0 { -impulse } else { impulse } / f64::from(self.masses[i]))
                as f32
        })
    }
    pub fn finish(&mut self, next: Motion, work: f64, impulse: f64, h: f64) -> Result<(), R> {
        let before: f64 = self.energy(&self.current).iter().sum();
        let after: f64 = self.energy(&next).iter().sum();
        let integrated: [f64; 2] = std::array::from_fn(|i| {
            0.5 * (f64::from(self.current.velocities[i]) + f64::from(next.velocities[i])) * h
        });
        let errors = [
            before - after - work,
            -impulse
                - f64::from(self.masses[0])
                    * (f64::from(next.velocities[0]) - f64::from(self.current.velocities[0])),
            impulse
                - f64::from(self.masses[1])
                    * (f64::from(next.velocities[1]) - f64::from(self.current.velocities[1])),
            next.positions[0] - self.current.positions[0] - integrated[0],
            next.positions[1] - self.current.positions[1] - integrated[1],
        ];
        for (i, delta) in integrated.into_iter().enumerate() {
            self.integrated_velocity[i] = model::add(self.integrated_velocity[i], delta)?;
        }
        for (i, error) in errors.into_iter().enumerate() {
            self.errors[i] = model::add(self.errors[i], error)?;
            self.absolute_errors[i] = model::add(self.absolute_errors[i], error.abs())?;
        }
        self.current = next;
        Ok(())
    }
    pub fn live(&self, world: &PhysicsWorld, scene: &Scene) -> bool {
        world.body_kind(scene.plates.bodies[1]) == Some(BodyKind::Dynamic)
            && Scene::mass(world, scene.plates.bodies[1]) == Ok(self.masses[1])
            && Motion::read(world, scene) == Ok(self.current.clone())
    }
    pub fn valid(&self, history: &History) -> bool {
        let c = history.curve();
        let s = &history.summary;
        let initial_energy: f64 = self.energy(&self.initial).iter().sum();
        let energy: f64 = self.energy(&self.current).iter().sum();
        let expected = [
            initial_energy
                - energy
                - s.elastic_energy
                - history.plastic_work()
                - s.viscous_loss
                - history.released_elastic(),
            -s.reaction_impulse
                - f64::from(self.masses[0])
                    * (f64::from(self.current.velocities[0])
                        - f64::from(self.initial.velocities[0])),
            s.reaction_impulse
                - f64::from(self.masses[1])
                    * (f64::from(self.current.velocities[1])
                        - f64::from(self.initial.velocities[1])),
            self.current.positions[0] - self.initial.positions[0] - self.integrated_velocity[0],
            self.current.positions[1] - self.initial.positions[1] - self.integrated_velocity[1],
        ];
        self.masses.iter().all(|m| (5.0..=80.0).contains(m))
            && self.masses[0] == history.mass
            && self.initial.valid()
            && self.current.valid()
            && history.acceleration == 0.0
            && history.loads.is_none()
            && history.incoming.is_none()
            && self.initial.indentation() == history.initial_x
            && self.current.indentation() == s.indentation
            && self.current.relative_velocity() == s.velocity
            && (self.integrated_velocity[0] - self.integrated_velocity[1] - s.integrated_velocity)
                .abs()
                <= 1e-10 * c.depth
            && self
                .errors
                .iter()
                .zip(self.absolute_errors)
                .zip(expected)
                .zip(self.scales(&c))
                .all(|(((error, absolute), expected), scale)| {
                    error.is_finite()
                        && absolute.is_finite()
                        && (0.0..=1e-5 * scale).contains(&absolute)
                        && error.abs() <= absolute + 1e-10 * scale
                        && (*error - expected).abs() <= 1e-10 * scale
                })
            && (s.solves != 0
                || (self.current == self.initial
                    && self.integrated_velocity == [0.0; 2]
                    && self.errors == [0.0; 5]
                    && self.absolute_errors == [0.0; 5]))
    }
    pub fn valid_failure(
        &self,
        boundary: &Motion,
        indentation: f64,
        velocity: f64,
        flight: f64,
        depth: f64,
    ) -> bool {
        boundary.valid()
            && boundary.relative_velocity() == velocity
            && boundary.indentation() == indentation
            && boundary.velocities == self.current.velocities
            && (0..2).all(|i| {
                (self.current.positions[i]
                    - boundary.positions[i]
                    - f64::from(boundary.velocities[i]) * flight)
                    .abs()
                    <= self.absolute_errors[3 + i] + 1e-10 * depth
            })
    }
    fn summary(&self, curve: &MaterialCurve) -> MovingSupportSummary {
        MovingSupportSummary {
            masses: self.masses,
            positions: self.current.positions,
            velocities: self.current.velocities,
            kinetic_energy: self.energy(&self.current),
            momentum: std::array::from_fn(|i| {
                f64::from(self.masses[i]) * f64::from(self.current.velocities[i])
            }),
            errors: self.errors,
            absolute_errors: self.absolute_errors,
            scales: self.scales(curve),
        }
    }
}

impl PhysicsWorld {
    /// Motion and fixed numerical balances of both bodies in a moving-support
    /// history. Retained after invalidation, like the relative-motion summary.
    pub fn material_moving_support_summary(&self) -> Option<MovingSupportSummary> {
        let h = &self.loading.as_ref()?.dynamic_compression.as_ref()?.history;
        Some(h.moving.as_ref()?.summary(&h.curve()))
    }

    /// Both bodies' motion at the one committed collapse boundary, including
    /// after invalidation or removal. Reading does not consume the observation.
    pub fn material_moving_support_failure(&self) -> Option<MovingSupportFailure> {
        let f = self
            .loading
            .as_ref()?
            .dynamic_compression
            .as_ref()?
            .history
            .failure
            .as_ref()?;
        let motion = f.motion.as_ref()?;
        Some(MovingSupportFailure {
            failure: f.event.clone()?,
            positions: motion.positions,
            velocities: motion.velocities,
        })
    }
}

#[cfg(test)]
#[path = "tests/moving.rs"]
mod tests;
