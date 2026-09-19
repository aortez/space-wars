//! Known kinematic trajectories in inertial axes fixed at the measurement epoch.
use super::*;

const MAX_SOURCES: usize = 32;
pub(super) const LAUNCH_WINDOW_TICKS: u64 = live_planning::MAX_SURVEY_AGE_TICKS;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Orbit {
    center: Vec2,
    radius: f32,
    phase: f32,
    rate: f32,
}
impl Orbit {
    fn position(self) -> Vec2 {
        self.center + Vec2::from_radians(self.phase) * self.radius
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Trajectory {
    position: Vec2,
    velocity: Vec2,
    orbit: Option<Orbit>,
}
impl Trajectory {
    fn advance(&mut self, ticks: u64, origin: Vec2, angle: f32) {
        if ticks == 0 {
            return;
        }
        if let Some(orbit) = &mut self.orbit {
            // Match PlanetState::update_orbit's f32 recurrence. A closed-form
            // rotation drifts from the engine once the accumulated phase grows.
            for _ in 1..ticks {
                orbit.phase += orbit.rate * DT;
            }
            let previous = orbit.position();
            orbit.phase += orbit.rate * DT;
            let position = orbit.position();
            self.position = (position - origin).rotate_radians(-angle);
            self.velocity = ((position - previous) / DT).rotate_radians(-angle);
        } else {
            self.position += self.velocity * (ticks as f32 * DT);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Source {
    path: Trajectory,
    scale: f32,
    radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlightEnvironment {
    pub(super) tick: u64,
    origin: Vec2,
    angle: f32,
    planet: usize,
    sources: Vec<Source>,
    pub(super) spin: f32,
}
impl FlightEnvironment {
    pub(crate) fn read(state: &SurfaceSortieState, p: &pilot::PilotObservationV1) -> Option<Self> {
        if state.world.planets.len() + usize::from(state.world.sun.is_some()) > MAX_SOURCES {
            return None;
        }
        let frame = p.planet.motion;
        let local = |point: Vec2| (point - frame.position).rotate_radians(-frame.angle);
        let orbital = matches!(
            state.motion_preset,
            SurfaceMotionPreset::Orbit
                | SurfaceMotionPreset::Generated
                | SurfaceMotionPreset::GeneratedSurfaceV1
        );
        let mut sources = Vec::new();
        for (i, body) in state.world.planets.iter().enumerate() {
            let m = motion::SurfaceFrame::read(&state.world.physics, i);
            sources.push(Source {
                path: Trajectory {
                    position: local(m.position),
                    velocity: m.linear_velocity.rotate_radians(-frame.angle),
                    orbit: if orbital {
                        Some(Orbit {
                            center: state.world.sun?.position,
                            radius: body.orbit_radius,
                            phase: body.orbit_angle,
                            rate: body.orbit_omega,
                        })
                    } else {
                        None
                    },
                },
                scale: 60.0 * GRAVITY * body.mass,
                radius: if state.world.terrain.planets.contains_key(&i) {
                    body.radius
                } else {
                    0.0
                },
            });
        }
        if let Some(sun) = state.world.sun {
            sources.push(Source {
                path: Trajectory {
                    position: local(sun.position),
                    velocity: Vec2::ZERO,
                    orbit: None,
                },
                scale: 60.0 * GRAVITY * sun.mass,
                radius: 0.0,
            });
        }
        Some(Self {
            tick: p.tick,
            origin: frame.position,
            angle: frame.angle,
            planet: p.planet.index,
            sources,
            spin: frame.spin,
        })
    }

    /// Compare the observed completed frame with the predicted frame *at this
    /// tick*. Admission also requires forecasts across the launch window; mere
    /// source identity never extends an old instantaneous flight prediction.
    pub(crate) fn compatible(&self, new: &Self) -> bool {
        let Some(ticks) = new
            .tick
            .checked_sub(self.tick)
            .filter(|t| *t <= LAUNCH_WINDOW_TICKS)
        else {
            return false;
        };
        let seconds = ticks as f32 * DT;
        let angle = new.angle - self.angle;
        let shift = (new.origin - self.origin).rotate_radians(-self.angle);
        let point = |p: Vec2| shift + p.rotate_radians(angle);
        // Rapier derives kinematic velocity from two f32 world poses. Comparing
        // velocities from two ticks includes rounding from four poses; near
        // world coordinate 1,000 that alone can exceed 0.01 units/second.
        // Keep the physical tolerance and add a coordinate-scaled rounding
        // envelope, rather than treating this quantization as an impulse.
        let pose_precision = |p: Vec2| p.x.abs().max(p.y.abs()).max(1.0) * f32::EPSILON;
        // Validation is bounded by MAX_SOURCES * LAUNCH_WINDOW_TICKS scalar
        // phase additions; each source needs only the final two world poses.
        let mut predicted = self.clone();
        for source in &mut predicted.sources {
            source.path.advance(ticks, self.origin, self.angle);
        }
        self.planet == new.planet
            && predicted.center().distance_to(shift) <= 0.01
            && (self.spin - new.spin).abs() <= 0.0001
            && ((angle - self.spin * seconds) * 0.5).sin().abs() <= 0.0001
            && self.sources.len() == new.sources.len()
            && predicted.sources.iter().zip(&new.sources).all(|(a, b)| {
                let velocity_roundoff = 2.0
                    * (pose_precision(self.origin + a.path.position.rotate_radians(self.angle))
                        + pose_precision(new.origin + b.path.position.rotate_radians(new.angle)))
                    / DT;
                a.path.position.distance_to(point(b.path.position)) <= 0.01
                    && a.path
                        .velocity
                        .distance_to(b.path.velocity.rotate_radians(angle))
                        <= 0.01 + velocity_roundoff
                    && match (a.path.orbit, b.path.orbit) {
                        (None, None) => true,
                        (Some(a), Some(b)) => {
                            a.rate == b.rate
                                && a.radius == b.radius
                                && a.center.distance_to(b.center) <= 0.01
                        }
                        _ => false,
                    }
                    && a.scale == b.scale
                    && a.radius == b.radius
            })
    }

    /// One fixed simulation tick; charged with the forecast's integration step.
    pub(super) fn advance(&mut self) {
        for source in &mut self.sources {
            source.path.advance(1, self.origin, self.angle);
        }
    }
    pub(super) fn center(&self) -> Vec2 {
        self.sources[self.planet].path.position
    }
    pub(super) fn time_dependent(&self) -> bool {
        self.sources.len() > 1 || self.sources[self.planet].path.orbit.is_some()
    }
    pub(super) fn surface_velocity(&self, offset: Vec2) -> Vec2 {
        self.sources[self.planet].path.velocity + Vec2::new(-offset.y, offset.x) * self.spin
    }
    pub(super) fn gravity(&self, point: Vec2) -> Vec2 {
        self.sources.iter().fold(Vec2::ZERO, |g, source| {
            let d = source.path.position - point;
            let r2 = d.length_squared().max(source.radius.powi(2)).max(0.01);
            g + d * (source.scale / (r2 * r2.sqrt()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicted_orbits_match_completed_frames_and_reject_changed_motion_or_sources() {
        for (radius, spin, orbit) in [
            (60.0, 0.015, 0.065),
            (60.0, -0.015, -0.065),
            (100.0, 0.02, 0.04),
        ] {
            let mut state = SurfaceSortieScenario::init_material_moving_crossing_trial(
                42, 0, radius, spin, orbit,
            );
            let before =
                FlightEnvironment::read(&state, &state.pilot_observation(0, None)).unwrap();
            for tick in 1..=LAUNCH_WINDOW_TICKS {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                let after =
                    FlightEnvironment::read(&state, &state.pilot_observation(0, None)).unwrap();
                assert!(
                    before.compatible(&after),
                    "tick={tick} orbit={orbit} spin={spin}: {before:?} -> {after:?}"
                );
            }
            let now = FlightEnvironment::read(&state, &state.pilot_observation(0, None)).unwrap();
            for mutation in 0..9 {
                let mut changed = now.clone();
                match mutation {
                    0 => changed.spin += 0.001,
                    1 => changed.sources[0].path.velocity.x += 0.1,
                    2 => changed.sources[0].path.position.x += 0.1,
                    3 => changed.sources[0].scale *= 1.01,
                    4 => changed.sources[0].path.orbit.as_mut().unwrap().rate *= 1.01,
                    5 => changed.sources[0].path.orbit.as_mut().unwrap().radius += 0.1,
                    6 => changed.sources[0].path.orbit.as_mut().unwrap().center.x += 0.1,
                    7 => changed.sources[0].radius += 0.1,
                    _ => changed.tick += 1,
                }
                assert!(!before.compatible(&changed), "mutation={mutation}");
            }
        }
    }

    #[test]
    fn generated_orbits_preserve_the_measured_environment_over_the_launch_window() {
        for phase_offset in [0.0, 10.0 * std::f32::consts::TAU] {
            let mut state = SurfaceSortieScenario::init_material_match(7725194555774358125);
            for planet in &mut state.world.planets {
                planet.orbit_angle += phase_offset;
            }
            for _ in 0..120 {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            let p = state.pilot_observation(0, None);
            let before = FlightEnvironment::read(&state, &p).unwrap();
            for tick in 1..=LAUNCH_WINDOW_TICKS {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                let mut p = state.pilot_observation(0, None);
                // Measure the same planet even if the idle ship changes its approach.
                let m = motion::SurfaceFrame::read(&state.world.physics, before.planet);
                p.planet.index = before.planet;
                p.planet.motion.position = m.position;
                p.planet.motion.angle = m.angle;
                p.planet.motion.velocity = m.linear_velocity;
                p.planet.motion.spin = m.angular_velocity;
                let after = FlightEnvironment::read(&state, &p).unwrap();
                assert!(
                    before.compatible(&after),
                    "tick={tick} phase_offset={phase_offset}: {before:?} -> {after:?}"
                );
            }
        }
    }
}
