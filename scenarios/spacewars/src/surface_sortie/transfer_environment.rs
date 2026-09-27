//! Opt-in source snapshot for free-flight prediction. No queries or world clone.
//! Known scripted motion and gravity are frozen here; future impacts and material
//! edits are not. This is separate from ordinary controller observations.
use super::*;
use pilot::{PilotMotion, PilotPlanetObservation};

pub const MAX_TRANSFER_PLANETS: usize = 8;
const DT: f32 = 1.0 / 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Orbit {
    center: Vec2,
    radius: f32,
    phase: f32,
    rate: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Source {
    motion: PilotMotion,
    orbit: Option<Orbit>,
    translation_velocity: Vec2,
    radius: f32,
    wrapper_angle: f32,
    wrapper_rate: f32,
    gravity_scale: f32,
    gravity_radius: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferEnvironment {
    pub tick: u64,
    planets: Vec<Source>,
    sun: Option<(mission::MissionObstacle, f32)>,
    ship_gravity_offset: Vec2,
}

impl SurfaceSortieState {
    /// Bounded, read-only capture of known environmental dynamics. Does not
    /// prepare terrain, flush queries or advance a hidden copy of the world.
    pub fn transfer_environment(&self) -> Result<TransferEnvironment, &'static str> {
        if self.world.planets.is_empty() || self.world.planets.len() > MAX_TRANSFER_PLANETS {
            return Err("transfer environment planet capacity");
        }
        let orbital = matches!(
            self.motion_preset,
            SurfaceMotionPreset::Orbit
                | SurfaceMotionPreset::Generated
                | SurfaceMotionPreset::GeneratedSurfaceV1
        );
        if orbital && self.world.sun.is_none() {
            return Err("transfer orbit source missing");
        }
        let planets = self
            .world
            .planets
            .iter()
            .enumerate()
            .map(|(index, body)| {
                let frame = motion::SurfaceFrame::read(&self.world.physics, index);
                Source {
                    motion: PilotMotion {
                        position: frame.position,
                        velocity: frame.linear_velocity,
                        angle: frame.angle,
                        spin: frame.angular_velocity,
                    },
                    orbit: orbital.then(|| Orbit {
                        center: self.world.sun.unwrap().position,
                        radius: body.orbit_radius,
                        phase: body.orbit_angle,
                        rate: body.orbit_omega,
                    }),
                    translation_velocity: if self.motion_preset == SurfaceMotionPreset::Translating
                    {
                        self.motion_preset.initial_velocity(body)
                    } else {
                        Vec2::ZERO
                    },
                    radius: body.radius,
                    wrapper_angle: body.wrapper_angle,
                    wrapper_rate: body.wrapper_omega,
                    gravity_scale: 60.0 * GRAVITY * body.mass,
                    gravity_radius: if self.world.terrain.planets.contains_key(&index) {
                        body.radius
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        Ok(TransferEnvironment {
            tick: self.tick(),
            planets,
            ship_gravity_offset: physics::ship_pivot(ShipForm::Ship),
            sun: self.world.sun.map(|sun| {
                (
                    mission::MissionObstacle {
                        position: sun.position,
                        radius: sun.radius * BODY_BOUNDS_RADIUS_SCALE,
                    },
                    60.0 * GRAVITY * sun.mass,
                )
            }),
        })
    }
}

impl TransferEnvironment {
    /// Compare an independently advanced source snapshot to a current capture.
    /// Never rebase the prediction onto later observations. Fixed parameters and
    /// scripted phases are exact; completed-body readings allow only the small
    /// numeric tolerances already checked against native motion/gravity tests.
    pub fn matches_advanced_environment(&self, observed: &Self) -> bool {
        let finite_vector = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        let finite = |e: &Self| {
            finite_vector(e.ship_gravity_offset)
                && e.sun.is_none_or(|(sun, scale)| {
                    finite_vector(sun.position)
                        && sun.radius.is_finite()
                        && sun.radius > 0.0
                        && scale.is_finite()
                        && scale >= 0.0
                })
                && !e.planets.is_empty()
                && e.planets.len() <= MAX_TRANSFER_PLANETS
                && e.planets.iter().all(|p| {
                    finite_vector(p.translation_velocity)
                        && p.radius.is_finite()
                        && p.radius > 0.0
                        && p.wrapper_angle.is_finite()
                        && p.wrapper_rate.is_finite()
                        && p.gravity_scale.is_finite()
                        && p.gravity_scale >= 0.0
                        && p.gravity_radius.is_finite()
                        && p.gravity_radius >= 0.0
                        && p.orbit.is_none_or(|o| {
                            finite_vector(o.center)
                                && o.radius.is_finite()
                                && o.radius > 0.0
                                && o.phase.is_finite()
                                && o.rate.is_finite()
                        })
                })
        };
        finite(self)
            && finite(observed)
            && self.tick == observed.tick
            && self.sun == observed.sun
            && self.ship_gravity_offset == observed.ship_gravity_offset
            && self.planets.len() == observed.planets.len()
            && self.planets.iter().zip(&observed.planets).all(|(a, b)| {
                a.orbit == b.orbit
                    && a.translation_velocity == b.translation_velocity
                    && a.radius == b.radius
                    && a.wrapper_angle == b.wrapper_angle
                    && a.wrapper_rate == b.wrapper_rate
                    && a.gravity_scale == b.gravity_scale
                    && a.gravity_radius == b.gravity_radius
                    && a.motion.position.distance_to(b.motion.position) < 0.002
                    && a.motion.velocity.distance_to(b.motion.velocity) < 0.02
                    && (a.motion.angle - b.motion.angle)
                        .sin()
                        .atan2((a.motion.angle - b.motion.angle).cos())
                        .abs()
                        < 0.002
                    && (a.motion.spin - b.motion.spin).abs() < 0.002
            })
    }

    /// Ship gravity retains the engine's legacy unrotated render-position
    /// offset; observations and motor guidance use the rigid-body origin.
    pub fn ship_gravity(&self, body_origin: Vec2) -> Vec2 {
        self.gravity(body_origin - self.ship_gravity_offset)
    }

    pub fn matches_source(&self, o: &mission::MissionObservationV1) -> bool {
        let frame = &o.local.combat.recovery.flight.pilot.planet;
        self.tick == o.local.combat.recovery.flight.pilot.tick
            && self.planets.len() == o.planets.len()
            && o.planets
                .get(frame.index)
                .is_some_and(|planet| planet == frame)
            && self
                .planets
                .iter()
                .zip(&o.planets)
                .enumerate()
                .all(|(index, (source, planet))| {
                    planet.index == index
                        && planet.motion == source.motion
                        && planet.radius == source.radius
                })
            && self.sun.map(|(sun, _)| sun) == o.sun
    }

    /// Advance exactly one known-motion tick, with the engine's f32 orbital
    /// phase recurrence. It deliberately cannot observe later world changes.
    pub fn advance(&mut self, planets: &mut [PilotPlanetObservation]) {
        assert_eq!(planets.len(), self.planets.len());
        for (source, planet) in self.planets.iter_mut().zip(planets) {
            let before = source.motion;
            let position = if let Some(orbit) = &mut source.orbit {
                orbit.phase += orbit.rate * DT;
                orbit.center + Vec2::from_radians(orbit.phase) * orbit.radius
            } else {
                before.position + source.translation_velocity * DT
            };
            source.wrapper_angle =
                (source.wrapper_angle + source.wrapper_rate * DT).rem_euclid(std::f32::consts::TAU);
            let angle = source.wrapper_angle.sin().atan2(source.wrapper_angle.cos());
            let turn = (angle - before.angle)
                .sin()
                .atan2((angle - before.angle).cos());
            source.motion = PilotMotion {
                position,
                velocity: (position - before.position) / DT,
                angle,
                spin: turn / DT,
            };
            planet.motion = source.motion;
        }
        self.tick += 1;
    }

    /// Native zero-contact approach recurrence; contacts/support are outside
    /// this model. Retain the old frame across a two-unit free-space bisector.
    pub fn approach_frame(&self, ship: Vec2, previous: usize) -> usize {
        let height = |index: usize| {
            ship.distance_to(self.planets[index].motion.position)
                - self.planets[index].radius * BODY_BOUNDS_RADIUS_SCALE
        };
        let mut selected = previous;
        let mut distance = height(selected);
        for index in 0..self.planets.len() {
            let candidate = height(index);
            if candidate + 2.0 < distance {
                selected = index;
                distance = candidate;
            }
        }
        selected
    }

    /// Same celestial point/spherical source law as the shared gravity solve.
    /// This is a local numeric model, not an authoritative physics operation.
    pub fn gravity(&self, position: Vec2) -> Vec2 {
        let contribution = |center: Vec2, scale: f32, radius: f32| {
            let delta = center - position;
            let r2 = delta.length_squared().max(radius * radius).max(0.01);
            delta * (scale / (r2 * r2.sqrt()))
        };
        let mut result = self.sun.map_or(Vec2::ZERO, |(sun, scale)| {
            contribution(sun.position, scale, 0.0)
        });
        for source in &self.planets {
            result += contribution(
                source.motion.position,
                source.gravity_scale,
                source.gravity_radius,
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_pins_dynamics_and_rejects_accumulated_drift_and_nonfinite_sources() {
        let state =
            SurfaceSortieScenario::init_material_moving_crossing_trial(42, 0, 60.0, 0.08, 0.065);
        let e = state.transfer_environment().unwrap();
        let changes: &[fn(&mut TransferEnvironment)] = &[
            |e| e.planets[0].orbit.as_mut().unwrap().rate += 0.001,
            |e| e.planets[0].orbit.as_mut().unwrap().phase += 0.001,
            |e| e.planets[0].orbit.as_mut().unwrap().radius += 0.001,
            |e| e.planets[0].orbit.as_mut().unwrap().center.x += 0.001,
            |e| e.planets[0].translation_velocity.x += 0.001,
            |e| e.planets[0].wrapper_rate += 0.001,
            |e| e.planets[0].wrapper_angle += 0.001,
            |e| e.planets[0].gravity_scale += 1.0,
            |e| e.planets[0].gravity_radius += 0.001,
            |e| e.ship_gravity_offset.x += 0.001,
            |e| e.sun.as_mut().unwrap().1 += 1.0,
            |e| e.planets[0].motion.velocity.x += 0.03,
            |e| e.planets[0].motion.angle += 0.003,
            |e| e.planets[0].motion.spin += 0.003,
            |e| e.planets[0].motion.position.x = f32::NAN,
            |e| e.planets[0].gravity_scale = f32::INFINITY,
        ];
        for change in changes {
            let mut changed = e.clone();
            change(&mut changed);
            assert!(!e.matches_advanced_environment(&changed));
        }
        let mut changed = e.clone();
        changed.planets[0].motion.position.x += 0.001;
        assert!(e.matches_advanced_environment(&changed));
        changed.planets[0].motion.position.x += 0.002;
        assert!(!e.matches_advanced_environment(&changed)); // Compare to source, never rebase.
        let mut invalid = e;
        invalid.planets[0].gravity_scale = f32::INFINITY;
        assert!(!invalid.matches_advanced_environment(&invalid));
    }

    #[test]
    fn snapshot_tracks_scripted_frames_and_shared_gravity_without_mutating_world() {
        for (spin, orbit) in [(0.08, 0.065), (-0.02, -0.065), (0.0, 0.0)] {
            let mut state = SurfaceSortieScenario::init_material_moving_crossing_trial(
                42, 0, 60.0, spin, orbit,
            );
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            let observed = state.mission_observation(0, None);
            let mut environment = state.transfer_environment().unwrap();
            assert!(environment.matches_source(&observed));
            assert_eq!(observed, state.mission_observation(0, None));
            let mut planets = observed.planets.clone();
            for _ in 0..3600 {
                let before = state
                    .world
                    .physics
                    .world
                    .motion(state.world.physics.ship_body(state.pilots[0].vehicle.0))
                    .unwrap();
                let gravity = environment.ship_gravity(before.position);
                environment.advance(&mut planets);
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                assert_eq!(environment.tick, state.tick());
                assert!(
                    environment
                        .matches_advanced_environment(&state.transfer_environment().unwrap())
                );
                for a in &planets {
                    let b = motion::SurfaceFrame::read(&state.world.physics, a.index);
                    assert!(a.motion.position.distance_to(b.position) < 0.002);
                    assert!(a.motion.velocity.distance_to(b.linear_velocity) < 0.02);
                    assert!((a.motion.spin - b.angular_velocity).abs() < 0.002);
                }
                assert!(
                    gravity.distance_to(state.pilots[0].ship_gravity_delta * 60.0) < 0.01,
                    "tick={} spin={spin} orbit={orbit} expected={gravity:?} actual={:?} ship={:?}",
                    environment.tick,
                    state.pilots[0].ship_gravity_delta * 60.0,
                    before,
                );
            }
            assert!(!environment.matches_source(&observed));
        }
    }
}
