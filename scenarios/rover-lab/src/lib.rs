//! Interactive feasibility scenario for the Rapier-backed rover.

use std::time::Duration;

use engine_common::{
    Action, Camera2, Fill, Observation, RenderCircle, RenderColor, RenderFrame, RenderLine,
    RenderPoint, RenderPolygon, RenderPrimitive, RenderText, Scenario, StepResult, Stroke,
    TextAnchor, TickModel,
};
use engine_core::Vec2;
use engine_gravity::{
    GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver,
    GravitySourcePolicy, GravityStepMetrics,
};
use engine_rapier::{
    rover::{
        BumpSpec, PlanetAssembly, PlanetSpec, RoverAssembly, RoverControl, RoverSnapshot, RoverSpec,
    },
    world::{BodyMotion, PhysicsId, PhysicsWorld, PhysicsWorldConfig},
};

const FIXED_HZ: u32 = 60;
const DRIVE_ACTION_KIND: u32 = 1;
const PLANET_ID: PhysicsId = PhysicsId::new(1);
const ROVER_ID: PhysicsId = PhysicsId::new(2);
const PLANET_GRAVITY_ID: u64 = 1;
const GRAVITY_SOURCE_TAG: u64 = 1 << 60;
const GRAVITY_ROVER_BODY_TAG: u64 = 2 << 60;
const GRAVITY_ID_MASK: u64 = (1 << 60) - 1;
const PLANET_RADIUS: f32 = 20.0;
const CAMERA_HEIGHT: f32 = 54.0;
const GRAVITY_ACCELERATION: f32 = 18.0;
const DEFAULT_PLANET_ANGULAR_VELOCITY: f32 = 0.015;
const LAB_BUMP: BumpSpec = BumpSpec {
    surface_angle: 1.08,
    half_width: 1.25,
    half_height: 0.32,
};

const PLANET_FILL: RenderColor = RenderColor::rgb(0.07, 0.13, 0.24);
const PLANET_STROKE: RenderColor = RenderColor::rgb(0.28, 0.56, 0.82);
const BUMP_COLOR: RenderColor = RenderColor::rgb(0.76, 0.46, 0.16);
const CHASSIS_COLOR: RenderColor = RenderColor::rgb(0.91, 0.32, 0.22);
const CHASSIS_STROKE: RenderColor = RenderColor::rgb(1.0, 0.76, 0.55);
const WHEEL_COLOR: RenderColor = RenderColor::rgb(0.13, 0.15, 0.19);
const WHEEL_STROKE: RenderColor = RenderColor::rgb(0.75, 0.82, 0.9);
const SUSPENSION_COLOR: RenderColor = RenderColor::rgb(0.38, 0.95, 0.84);
const CONTACT_COLOR: RenderColor = RenderColor::rgb(1.0, 0.2, 0.85);

pub struct RoverLabScenario;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoverLabConfig {
    pub planet_angular_velocity: f32,
    pub gravity_acceleration: f32,
}

impl Default for RoverLabConfig {
    fn default() -> Self {
        Self {
            planet_angular_velocity: DEFAULT_PLANET_ANGULAR_VELOCITY,
            gravity_acceleration: GRAVITY_ACCELERATION,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoverGravitySource {
    pub id: u64,
    pub position: Vec2,
    /// `G * mass` in Rover Lab units.
    pub gravitational_parameter: f32,
    pub policy: GravitySourcePolicy,
}

pub struct RoverLabState {
    pub config: RoverLabConfig,
    pub tick: u64,
    pub planet: PlanetSpec,
    pub rover_spec: RoverSpec,
    pub control: RoverControl,
    pub gravity_sources: Vec<RoverGravitySource>,
    pub last_gravity_metrics: GravityStepMetrics,
    planet_assembly: PlanetAssembly,
    rover: RoverAssembly,
    physics: PhysicsWorld,
    gravity_solver: GravitySolver,
    gravity_participants: Vec<GravityParticipant>,
}

impl RoverLabState {
    pub fn rover_snapshot(&self) -> RoverSnapshot {
        self.rover
            .snapshot(&self.physics)
            .expect("rover lab must retain its rover")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoverLabAction {
    pub throttle: f32,
    pub brake: bool,
}

impl RoverLabAction {
    pub fn drive(throttle: f32, brake: bool) -> Action {
        let mut payload = throttle.to_le_bytes().to_vec();
        payload.push(u8::from(brake));
        Action::scenario(DRIVE_ACTION_KIND, payload)
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        if *kind != DRIVE_ACTION_KIND || payload.len() != 5 {
            return None;
        }
        Some(Self {
            throttle: f32::from_le_bytes(payload[0..4].try_into().ok()?),
            brake: payload[4] != 0,
        })
    }
}

impl Scenario for RoverLabScenario {
    type State = RoverLabState;
    type Config = RoverLabConfig;

    fn init(config: Self::Config, _seed: u64) -> Self::State {
        let config = normalized_config(config);
        let planet = PlanetSpec {
            center: Vec2::ZERO,
            radius: PLANET_RADIUS,
            angle: 0.0,
        };
        let rover_spec = RoverSpec::default();
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            length_unit: 1.0,
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 2,
            collect_events: true,
            ..PhysicsWorldConfig::default()
        });
        let planet_assembly = PlanetAssembly::insert(&mut physics, PLANET_ID, planet, &[LAB_BUMP])
            .expect("rover lab planet specification must be valid");
        let rover =
            RoverAssembly::insert(&mut physics, ROVER_ID, &planet_assembly, planet, rover_spec)
                .expect("rover lab rover specification must be valid");
        let gravity_reference_distance = planet.radius + rover_spec.wheel_radius;
        let gravity_sources = vec![RoverGravitySource {
            id: PLANET_GRAVITY_ID,
            position: planet.center,
            gravitational_parameter: config.gravity_acceleration
                * gravity_reference_distance
                * gravity_reference_distance,
            policy: GravitySourcePolicy::Direct,
        }];

        RoverLabState {
            config,
            tick: 0,
            planet,
            rover_spec,
            control: RoverControl::default(),
            gravity_sources,
            last_gravity_metrics: GravityStepMetrics::default(),
            planet_assembly,
            rover,
            physics,
            gravity_solver: GravitySolver::new(),
            gravity_participants: Vec::new(),
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        if let Some(action) = actions
            .iter()
            .filter_map(RoverLabAction::decode)
            .next_back()
        {
            state.control = RoverControl {
                throttle: action.throttle.clamp(-1.0, 1.0),
                brake: action.brake,
            };
        }

        let dt_seconds = dt.as_secs_f32();
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return StepResult::default();
        }

        state.planet.angle = (state.planet.angle
            + state.config.planet_angular_velocity * dt_seconds)
            .rem_euclid(std::f32::consts::TAU);
        state.planet_assembly.set_next_pose(
            &mut state.physics,
            state.planet.center,
            state.planet.angle,
        );
        state.rover.set_control(&mut state.physics, state.control);

        if let Some(planet_source) = state
            .gravity_sources
            .iter_mut()
            .find(|source| source.id == PLANET_GRAVITY_ID)
        {
            planet_source.position = state.planet.center;
        }
        state.physics.clear_forces();
        state.last_gravity_metrics = apply_rover_gravity(state, dt_seconds);
        state.physics.step(dt_seconds);
        state.tick += 1;
        StepResult::default()
    }

    fn observe(state: &Self::State) -> Observation {
        let rover = state.rover_snapshot();
        let mut payload = state.tick.to_le_bytes().to_vec();
        for value in [
            rover.chassis.position.x,
            rover.chassis.position.y,
            rover.chassis.angle,
            rover.chassis.linear_velocity.x,
            rover.chassis.linear_velocity.y,
            rover.wheels[0].angle,
            rover.wheels[1].angle,
        ] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        Observation { payload }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render_lab(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: FIXED_HZ }
    }
}

fn apply_rover_gravity(state: &mut RoverLabState, dt_seconds: f32) -> GravityStepMetrics {
    let RoverLabState {
        gravity_sources,
        rover,
        physics,
        gravity_solver,
        gravity_participants,
        ..
    } = state;
    gravity_participants.clear();
    gravity_participants.extend(gravity_sources.iter().map(|source| GravityParticipant {
        id: tagged_gravity_id(GRAVITY_SOURCE_TAG, source.id),
        position: source.position,
        source_mass: source.gravitational_parameter,
        response_scale: 0.0,
        source_policy: source.policy,
    }));

    let bodies = rover.bodies();
    for body in bodies {
        let motion = physics
            .motion(body)
            .expect("rover gravity bodies remain in the canonical world");
        gravity_participants.push(GravityParticipant::target(
            tagged_gravity_id(GRAVITY_ROVER_BODY_TAG, u64::from(body.role.value())),
            motion.position,
            1.0,
        ));
    }

    let target_offset = gravity_sources.len();
    let outputs = gravity_solver
        .solve(
            gravity_participants,
            GravityConfig {
                backend: GravityBackend::BarnesHut { theta: 0.7 },
                softening: 0.05,
                interaction_scale: dt_seconds,
            },
        )
        .expect("Rover Lab gravity sources are valid");
    for (body, output) in bodies.into_iter().zip(&outputs[target_offset..]) {
        assert!(physics.apply_velocity_delta(body, output.velocity_delta, true));
    }
    gravity_solver.metrics()
}

fn tagged_gravity_id(tag: u64, payload: u64) -> GravityId {
    debug_assert_eq!(payload & !GRAVITY_ID_MASK, 0);
    GravityId::new(tag | payload)
}

fn normalized_config(config: RoverLabConfig) -> RoverLabConfig {
    RoverLabConfig {
        planet_angular_velocity: if config.planet_angular_velocity.is_finite() {
            config.planet_angular_velocity
        } else {
            DEFAULT_PLANET_ANGULAR_VELOCITY
        },
        gravity_acceleration: if config.gravity_acceleration.is_finite()
            && config.gravity_acceleration > 0.0
        {
            config.gravity_acceleration
        } else {
            GRAVITY_ACCELERATION
        },
    }
}

fn render_lab(state: &RoverLabState) -> RenderFrame {
    let rover = state.rover_snapshot();
    let mut frame = RenderFrame::new(Camera2::new(
        render_point(state.planet.center),
        CAMERA_HEIGHT,
    ));

    frame.push_primitive(
        -20,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(state.planet.center),
            radius: state.planet.radius,
            fill: Some(Fill::new(PLANET_FILL)),
            stroke: Some(Stroke::new(PLANET_STROKE, 0.16)),
        }),
    );
    frame.push_primitive(
        -19,
        RenderPrimitive::Polygon(RenderPolygon {
            points: bump_points(state.planet).map(render_point).to_vec(),
            fill: Some(Fill::new(BUMP_COLOR)),
            stroke: Some(Stroke::new(CHASSIS_STROKE, 0.12)),
        }),
    );

    let planet_marker_angle = state.planet.angle + std::f32::consts::FRAC_PI_2;
    let marker_start =
        state.planet.center + Vec2::from_radians(planet_marker_angle) * (state.planet.radius - 1.4);
    let marker_end =
        state.planet.center + Vec2::from_radians(planet_marker_angle) * state.planet.radius;
    frame.push_primitive(
        -18,
        RenderPrimitive::Line(RenderLine::new(
            render_point(marker_start),
            render_point(marker_end),
            Stroke::new(SUSPENSION_COLOR, 0.18),
        )),
    );

    for index in 0..2 {
        let wheel = rover.wheels[index];
        frame.push_primitive(
            1,
            RenderPrimitive::Line(RenderLine::new(
                render_point(rover.suspension_anchors[index]),
                render_point(wheel.position),
                Stroke::new(SUSPENSION_COLOR, 0.16),
            )),
        );
        render_wheel(&mut frame, wheel, state.rover_spec.wheel_radius);
    }

    frame.push_primitive(
        3,
        RenderPrimitive::Polygon(RenderPolygon {
            points: body_box_points(
                rover.chassis,
                state.rover_spec.chassis_half_width,
                state.rover_spec.chassis_half_height,
            )
            .map(render_point)
            .to_vec(),
            fill: Some(Fill::new(CHASSIS_COLOR)),
            stroke: Some(Stroke::new(CHASSIS_STROKE, 0.14)),
        }),
    );

    for contact in rover.contacts {
        frame.push_primitive(
            5,
            RenderPrimitive::Circle(RenderCircle::filled(
                render_point(contact.position),
                0.13,
                CONTACT_COLOR,
            )),
        );
        frame.push_primitive(
            5,
            RenderPrimitive::Line(RenderLine::new(
                render_point(contact.position),
                render_point(contact.position + contact.normal * 0.7),
                Stroke::new(CONTACT_COLOR, 0.09),
            )),
        );
    }

    let control_label = if state.control.brake {
        "brake"
    } else if state.control.throttle > 0.0 {
        "forward"
    } else if state.control.throttle < 0.0 {
        "reverse"
    } else {
        "coast"
    };
    let mut text = RenderText::new(
        RenderPoint::new(0.0, 25.3),
        format!("Rover lab | W forward  S brake  X reverse  R reset | {control_label}"),
    );
    text.color = RenderColor::rgb(0.88, 0.94, 1.0);
    text.size = 16.0;
    text.anchor = TextAnchor::Center;
    frame.push_primitive(10, RenderPrimitive::Text(text));

    frame
}

fn render_wheel(frame: &mut RenderFrame, wheel: BodyMotion, radius: f32) {
    frame.push_primitive(
        2,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(wheel.position),
            radius,
            fill: Some(Fill::new(WHEEL_COLOR)),
            stroke: Some(Stroke::new(WHEEL_STROKE, 0.14)),
        }),
    );
    let spoke = Vec2::from_radians(wheel.angle) * (radius * 0.78);
    frame.push_primitive(
        3,
        RenderPrimitive::Line(RenderLine::new(
            render_point(wheel.position - spoke),
            render_point(wheel.position + spoke),
            Stroke::new(WHEEL_STROKE, 0.11),
        )),
    );
}

fn body_box_points(body: BodyMotion, half_width: f32, half_height: f32) -> [Vec2; 4] {
    [
        Vec2::new(-half_width, -half_height),
        Vec2::new(half_width, -half_height),
        Vec2::new(half_width, half_height),
        Vec2::new(-half_width, half_height),
    ]
    .map(|point| body.position + point.rotate_radians(body.angle))
}

fn bump_points(planet: PlanetSpec) -> [Vec2; 4] {
    let surface_angle = planet.angle + LAB_BUMP.surface_angle;
    let normal = Vec2::from_radians(surface_angle);
    let tangent = Vec2::from_radians(surface_angle - std::f32::consts::FRAC_PI_2);
    let center = planet.center + normal * LAB_BUMP.radial_center_distance(planet.radius);
    LAB_BUMP
        .local_vertices()
        .map(|point| center + tangent * point.x + normal * point.y)
}

fn render_point(point: Vec2) -> RenderPoint {
    RenderPoint::new(point.x, point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_action_round_trips() {
        let action = RoverLabAction::drive(-0.75, true);
        assert_eq!(
            RoverLabAction::decode(&action),
            Some(RoverLabAction {
                throttle: -0.75,
                brake: true,
            })
        );
    }

    #[test]
    fn scenario_steps_and_renders_authoritative_rapier_state() {
        let mut state = RoverLabScenario::init(RoverLabConfig::default(), 4);
        let initial = state.rover_snapshot().chassis.position;
        let action = RoverLabAction::drive(0.7, false);
        for _ in 0..600 {
            RoverLabScenario::step(
                &mut state,
                std::slice::from_ref(&action),
                Duration::from_secs_f32(1.0 / 60.0),
            );
        }

        let current = state.rover_snapshot().chassis.position;
        assert!(current.distance_to(initial) > 2.0);
        assert_eq!(state.tick, 600);
        assert!(
            RoverLabScenario::render_frame(&state)
                .layers
                .iter()
                .any(|layer| !layer.primitives.is_empty())
        );
        assert!(!RoverLabScenario::observe(&state).payload.is_empty());
    }

    #[test]
    fn rover_field_accepts_multiple_sources_or_no_sources() {
        let mut state = RoverLabScenario::init(RoverLabConfig::default(), 9);
        state.gravity_sources.push(RoverGravitySource {
            id: 2,
            position: Vec2::new(80.0, 30.0),
            gravitational_parameter: 250.0,
            policy: GravitySourcePolicy::Hierarchical,
        });

        RoverLabScenario::step(&mut state, &[], Duration::from_secs_f32(1.0 / 60.0));
        assert_eq!(state.last_gravity_metrics.source_count, 2);
        assert_eq!(state.last_gravity_metrics.direct_source_count, 1);
        assert_eq!(state.last_gravity_metrics.hierarchical_source_count, 1);
        assert_eq!(state.last_gravity_metrics.target_count, 3);

        state.gravity_sources.clear();
        RoverLabScenario::step(&mut state, &[], Duration::from_secs_f32(1.0 / 60.0));
        assert_eq!(state.last_gravity_metrics.source_count, 0);
        assert_eq!(state.last_gravity_metrics.target_count, 3);
        assert_eq!(state.last_gravity_metrics.applied_sources, 0);
    }
}
