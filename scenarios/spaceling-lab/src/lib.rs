//! Playable single-body character testbed, independent of Spacewars services.

use std::time::Duration;

use engine_common::{Action, Observation, RenderFrame, Scenario, StepResult, TickModel};
use engine_core::Vec2;
use engine_gravity::{
    GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver,
    GravitySourcePolicy,
};
use engine_rapier::{
    rover::{BumpSpec, PlanetAssembly, PlanetSpec},
    spaceling::{SpacelingAssembly, SpacelingControl, SpacelingSnapshot, SpacelingSpec},
    world::{PhysicsId, PhysicsWorld, PhysicsWorldConfig},
};

mod render;

pub const FIXED_HZ: u32 = 60;
const CONTROL_V1: u32 = 1;
const CONTROL_V2: u32 = 2;
const PLANET_ID: PhysicsId = PhysicsId::new(1);
const SPACELING_ID: PhysicsId = PhysicsId::new(2);
const PLANET_RADIUS: f32 = 20.0;
const BUMPS: [BumpSpec; 2] = [
    BumpSpec {
        surface_angle: 1.08,
        half_width: 2.0,
        half_height: 0.45,
    },
    BumpSpec {
        surface_angle: 2.1,
        half_width: 1.6,
        half_height: 0.3,
    },
];

pub struct SpacelingLabScenario;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingLabConfig {
    pub planet_angular_velocity: f32,
    pub gravity_acceleration: f32,
}

impl Default for SpacelingLabConfig {
    fn default() -> Self {
        Self {
            planet_angular_velocity: 0.025,
            gravity_acceleration: 18.0,
        }
    }
}

pub struct SpacelingLabState {
    pub config: SpacelingLabConfig,
    pub tick: u64,
    pub planet: PlanetSpec,
    pub spaceling_spec: SpacelingSpec,
    pub control: SpacelingControl,
    pub gravity: Vec2,
    pub airborne_ticks: u64,
    pub last_airtime_ticks: u64,
    pub landings: u64,
    pub shove_held: bool,
    pub shoves: u64,
    pub last_shove: Option<ShoveDiagnostic>,
    shove_was_held: bool,
    gait_phase: f32,
    facing: f32,
    was_grounded: bool,
    planet_assembly: PlanetAssembly,
    spaceling: SpacelingAssembly,
    physics: PhysicsWorld,
    gravity_solver: GravitySolver,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShoveDiagnostic {
    pub tick: u64,
    pub point: Vec2,
    pub velocity_delta: Vec2,
}

impl SpacelingLabState {
    pub fn spaceling_snapshot(&self) -> SpacelingSnapshot {
        self.spaceling
            .snapshot(&self.physics)
            .expect("lab retains its spaceling")
    }
}

/// Versioned, held-state controls. The last valid action in a tick wins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingLabAction {
    pub walk: f32,
    pub jump_held: bool,
    pub shove_held: bool,
}

impl SpacelingLabAction {
    pub fn control(walk: f32, jump_held: bool) -> Action {
        Self::with_shove(walk, jump_held, false)
    }

    pub fn with_shove(walk: f32, jump_held: bool, shove_held: bool) -> Action {
        let mut payload = walk.to_le_bytes().to_vec();
        payload.push(u8::from(jump_held));
        payload.push(u8::from(shove_held));
        Action::scenario(CONTROL_V2, payload)
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        let expected_length = match *kind {
            CONTROL_V1 => 5,
            CONTROL_V2 => 6,
            _ => return None,
        };
        if payload.len() != expected_length || payload[4..].iter().any(|value| *value > 1) {
            return None;
        }
        let walk = f32::from_le_bytes(payload[..4].try_into().ok()?);
        walk.is_finite().then_some(Self {
            walk: walk.clamp(-1.0, 1.0),
            jump_held: payload[4] != 0,
            shove_held: payload.get(5).is_some_and(|value| *value != 0),
        })
    }
}

impl Scenario for SpacelingLabScenario {
    type State = SpacelingLabState;
    type Config = SpacelingLabConfig;

    fn init(config: Self::Config, _seed: u64) -> Self::State {
        // This fixture is intentionally unrandomized; seed remains a host/replay input.
        let defaults = SpacelingLabConfig::default();
        let config = SpacelingLabConfig {
            planet_angular_velocity: if config.planet_angular_velocity.is_finite() {
                config.planet_angular_velocity.clamp(-0.5, 0.5)
            } else {
                defaults.planet_angular_velocity
            },
            gravity_acceleration: if config.gravity_acceleration.is_finite() {
                config.gravity_acceleration.clamp(0.0, 100.0)
            } else {
                defaults.gravity_acceleration
            },
        };
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 2,
            // Character support queries the local narrow phase directly.
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        let planet = PlanetSpec {
            center: Vec2::ZERO,
            radius: PLANET_RADIUS,
            angle: 0.0,
        };
        let planet_assembly = PlanetAssembly::insert(&mut physics, PLANET_ID, planet, &BUMPS)
            .expect("valid lab planet");
        let spaceling_spec = SpacelingSpec::default();
        let spaceling = SpacelingAssembly::insert(
            &mut physics,
            SPACELING_ID,
            Vec2::new(0.0, PLANET_RADIUS + spaceling_spec.half_height() + 0.15),
            0.0,
            spaceling_spec,
        )
        .expect("valid lab spaceling");
        SpacelingLabState {
            config,
            tick: 0,
            planet,
            spaceling_spec,
            control: SpacelingControl::default(),
            gravity: Vec2::ZERO,
            airborne_ticks: 0,
            last_airtime_ticks: 0,
            landings: 0,
            shove_held: false,
            shove_was_held: false,
            shoves: 0,
            last_shove: None,
            gait_phase: 0.0,
            facing: 1.0,
            was_grounded: false,
            planet_assembly,
            spaceling,
            physics,
            gravity_solver: GravitySolver::new(),
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        let dt = dt.as_secs_f32();
        if dt <= 0.0 || !dt.is_finite() {
            return StepResult::default();
        }
        if let Some(action) = actions
            .iter()
            .filter_map(SpacelingLabAction::decode)
            .next_back()
        {
            state.control = SpacelingControl {
                walk: action.walk,
                jump_held: action.jump_held,
            };
            state.shove_held = action.shove_held;
        }
        if state.control.walk.abs() > 0.01 {
            state.facing = state.control.walk.signum();
        }
        state.planet.angle = (state.planet.angle + state.config.planet_angular_velocity * dt)
            .rem_euclid(std::f32::consts::TAU);
        state.planet_assembly.set_next_pose(
            &mut state.physics,
            state.planet.center,
            state.planet.angle,
        );
        state.physics.clear_forces();
        let position = state.spaceling_snapshot().motion.position;
        let reference_radius = state.planet.radius + state.spaceling_spec.half_height();
        let participants = [
            GravityParticipant {
                id: GravityId::new(1),
                position: state.planet.center,
                source_mass: state.config.gravity_acceleration
                    * reference_radius
                    * reference_radius,
                response_scale: 0.0,
                source_policy: GravitySourcePolicy::Direct,
            },
            GravityParticipant::target(GravityId::new(2), position, 1.0),
        ];
        let delta = state
            .gravity_solver
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::Exact,
                    softening: 0.05,
                    interaction_scale: dt,
                },
            )
            .expect("valid lab gravity")[1]
            .velocity_delta;
        state.gravity = delta / dt;
        if state.shove_held && !state.shove_was_held {
            let snapshot = state.spaceling_snapshot();
            let up = if state.gravity.length_squared() > 1e-6 {
                state.gravity.normalized() * -1.0
            } else {
                snapshot.up
            };
            let velocity_delta = Vec2::new(up.y, -up.x) * (4.0 * state.facing) + up * 6.0;
            let point = snapshot.motion.position
                + Vec2::new(0.0, 0.6).rotate_radians(snapshot.motion.angle);
            let mass = state
                .physics
                .body_mass(state.spaceling.body())
                .expect("lab body exists");
            if state.physics.apply_impulse_at_point(
                state.spaceling.body(),
                velocity_delta * mass,
                point,
                true,
            ) {
                state.shoves += 1;
                state.last_shove = Some(ShoveDiagnostic {
                    tick: state.tick,
                    point,
                    velocity_delta,
                });
            }
        }
        state.shove_was_held = state.shove_held;
        state
            .spaceling
            .apply_control(&mut state.physics, state.control, state.gravity, dt);
        state
            .physics
            .apply_velocity_delta(state.spaceling.body(), delta, true);
        state.physics.step(dt);
        state.tick += 1;

        let spaceling = state.spaceling_snapshot();
        if spaceling.grounded() {
            if !state.was_grounded {
                state.last_airtime_ticks = state.airborne_ticks;
                state.landings += 1;
            }
            state.airborne_ticks = 0;
            state.gait_phase = (state.gait_phase + spaceling.relative_speed.abs() * dt * 5.0)
                .rem_euclid(std::f32::consts::TAU);
        } else {
            state.airborne_ticks += 1;
        }
        state.was_grounded = spaceling.grounded();
        StepResult::default()
    }

    fn observe(state: &Self::State) -> Observation {
        let spaceling = state.spaceling_snapshot();
        let mut payload = vec![3]; // Observation version: explicit get-up diagnostics.
        for value in [
            state.tick,
            spaceling.jumps,
            state.landings,
            state.airborne_ticks,
            state.shoves,
            spaceling.knockdowns,
            spaceling.recoveries,
            spaceling.get_up_attempts,
        ] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            spaceling.motion.position.x,
            spaceling.motion.position.y,
            spaceling.motion.angle,
            spaceling.motion.linear_velocity.x,
            spaceling.motion.linear_velocity.y,
            spaceling.motion.angular_velocity,
            spaceling.up.x,
            spaceling.up.y,
            spaceling.relative_speed,
            state.gravity.x,
            state.gravity.y,
            state.planet.angle,
            state.control.walk,
            spaceling.recovery_progress,
            spaceling.settled_seconds,
        ] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        payload.push(u8::from(state.control.jump_held));
        payload.push(u8::from(state.shove_held));
        payload.push(spaceling.get_up_result as u8);
        payload.push(match spaceling.balance {
            engine_rapier::spaceling::SpacelingBalance::Balanced => 0,
            engine_rapier::spaceling::SpacelingBalance::KnockedDown => 1,
            engine_rapier::spaceling::SpacelingBalance::Recovering => 2,
        });
        payload.push(u8::from(spaceling.last_knockdown.is_some()));
        if let Some(disturbance) = spaceling.last_knockdown {
            payload.extend_from_slice(&disturbance.velocity_change.to_le_bytes());
            payload.extend_from_slice(&disturbance.angular_speed.to_le_bytes());
        }
        payload.push(u8::from(spaceling.grounded()));
        if let Some(support) = spaceling.support {
            payload.extend_from_slice(&support.collider.entity.value().to_le_bytes());
            payload.extend_from_slice(&support.collider.role.value().to_le_bytes());
            payload.extend_from_slice(&support.collider.part.to_le_bytes());
            for value in [
                support.normal.x,
                support.normal.y,
                support.velocity.x,
                support.velocity.y,
                support.angular_velocity,
            ] {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
        Observation { payload }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render::frame(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: FIXED_HZ }
    }
}

#[cfg(test)]
mod tests;
