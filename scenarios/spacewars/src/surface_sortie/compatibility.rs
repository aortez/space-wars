//! Reproducible compatibility evidence, isolated from ordinary gameplay.
//!
//! Keep every generated planet and the sun. Raw preserves their original masses
//! and paths; named profiles make explicit fixture-only changes for comparison.
//! Asteroids/starfield and the unused player are disabled.
//! Each probe starts afresh; on-foot probes require a real landing and transfer.

use super::*;

/// Default Spacewars world, zero-based planet, and quarter-turn bearing relative
/// to the sun: 0 = away, 1 = counterclockwise, 2 = toward, 3 = clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSurfaceCase {
    pub seed: u64,
    pub planet: usize,
    pub bearing: u8,
    #[serde(default)]
    pub profile: GeneratedSurfaceProfile,
}

fn config() -> SpacewarsConfig {
    SpacewarsConfig {
        use_starfield: false,
        asteroid_probability_per_sec: 0.0,
        ..SpacewarsConfig::default()
    }
}

impl GeneratedSurfaceCase {
    pub const fn new(seed: u64, planet: usize, bearing: u8) -> Self {
        Self {
            seed,
            planet,
            bearing,
            profile: GeneratedSurfaceProfile::Raw,
        }
    }

    pub const fn with_profile(mut self, profile: GeneratedSurfaceProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn planet_count(seed: u64) -> usize {
        build_world(&config(), seed).1.len()
    }

    pub fn init(self) -> Result<SurfaceSortieState, &'static str> {
        self.init_with_outpost(true)
    }

    pub(super) fn init_with_outpost(
        self,
        outpost: bool,
    ) -> Result<SurfaceSortieState, &'static str> {
        if self.bearing > 3 {
            return Err("bearing must be 0, 1, 2, or 3");
        }
        let mut world = SpacewarsScenario::init(config(), self.seed);
        self.profile.apply(&mut world);
        let planet = *world
            .planets
            .get(self.planet)
            .ok_or("planet index is outside the generated world")?;
        let up = (planet.position - world.sun.unwrap().position)
            .normalized()
            .rotate_radians(self.bearing as f32 * std::f32::consts::FRAC_PI_2);
        // Keep the terminal approximately the same walking distance away as
        // in the radius-60 fixture; do not rescale the ship or spaceling.
        let terminal_angle = up.y.atan2(up.x) - planet.wrapper_angle - 20.4 / planet.radius;
        world.ships[0].omega = planet.wrapper_omega;
        let mut state = SurfaceSortieScenario::on_surface(
            world,
            self.profile.preset(),
            self.planet,
            up,
            outpost.then_some(terminal_angle),
        );
        state.generated_case = Some(self);
        Ok(state)
    }

    pub fn run(self) -> Result<CompatibilityReport, &'static str> {
        let initial = self.init()?;
        let environment = SurfaceEnvironment::read(&initial);
        let probes = ProbeKind::ALL
            .into_iter()
            .map(|kind| run_probe(self.init().expect("validated case"), kind))
            .collect();
        Ok(CompatibilityReport {
            version: 2,
            case: self,
            environment,
            probes,
        })
    }
}

/// Acceleration in world units/s². The shared game's unsoftened inverse-square
/// field emits velocity deltas per 60 Hz tick. This is diagnostic only: it never
/// feeds another force into the physics step.
pub(super) fn source_acceleration(origin: Vec2, mass: f32, point: Vec2) -> Vec2 {
    let offset = origin - point;
    if offset.length_squared() == 0.0 {
        Vec2::ZERO
    } else {
        offset.normalized() * (60.0 * GRAVITY * mass / offset.length_squared())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceEnvironment {
    pub universe_radius: u32,
    pub planets: usize,
    pub radius: f32,
    pub mass: f32,
    pub orbit_radius: f32,
    pub orbit_omega: f32,
    pub spin: f32,
    /// Planet alone, at the standing capsule center (not a feet force).
    pub own_surface_gravity: f32,
    pub external_gravity_at_center: Vec2,
    pub orbit_acceleration: Vec2,
    pub frame_acceleration_mismatch: f32,
    /// Legacy ship gravity samples ShipState.position, which is offset from
    /// its Rapier body origin. Report the actual query point, not an idealized one.
    pub ship_gravity_sample_position: Vec2,
    pub ship_body_origin: Vec2,
    pub gravity_at_ship_spawn: Vec2,
    /// Required acceleration to remain on the prescribed rotating surface.
    pub surface_acceleration_at_ship_spawn: Vec2,
    /// Net gravity relative to that surface frame, projected inward.
    pub effective_inward_acceleration: f32,
    pub effective_lateral_acceleration: f32,
    pub ship_thrust_acceleration: f32,
    pub spaceling_jump_speed: f32,
}

impl SurfaceEnvironment {
    pub(super) fn read(state: &SurfaceSortieState) -> Self {
        let planet = &state.world.planets[state.pilots[0].planet];
        let center = state.world.ships[0].position + SHIP_PIVOT;
        let gravity_point = state.world.ships[0].position;
        let up = (center - planet.position).normalized();
        let mut external = Vec2::ZERO;
        let mut gravity = Vec2::ZERO;
        if let Some(sun) = state.world.sun {
            external += source_acceleration(sun.position, sun.mass, planet.position);
            gravity += source_acceleration(sun.position, sun.mass, gravity_point);
        }
        for (index, source) in state.world.planets.iter().enumerate() {
            if index != state.pilots[0].planet {
                external += source_acceleration(source.position, source.mass, planet.position);
            }
            gravity += source_acceleration(source.position, source.mass, gravity_point);
        }
        let orbit = state.world.sun.map_or(Vec2::ZERO, |sun| {
            (sun.position - planet.position) * planet.orbit_omega.powi(2)
        });
        let surface = orbit - (center - planet.position) * planet.wrapper_omega.powi(2);
        Self {
            universe_radius: state.world.config.universe_radius,
            planets: state.world.planets.len(),
            radius: planet.radius,
            mass: planet.mass,
            orbit_radius: planet.orbit_radius,
            orbit_omega: planet.orbit_omega,
            spin: planet.wrapper_omega,
            own_surface_gravity: 60.0 * GRAVITY * planet.mass
                / (planet.radius * BODY_BOUNDS_RADIUS_SCALE
                    + SurfaceSortieState::spec().half_height())
                .powi(2),
            external_gravity_at_center: external,
            orbit_acceleration: orbit,
            frame_acceleration_mismatch: (external - orbit).length(),
            ship_gravity_sample_position: gravity_point,
            ship_body_origin: center,
            gravity_at_ship_spawn: gravity,
            surface_acceleration_at_ship_spawn: surface,
            effective_inward_acceleration: -(gravity - surface).dot(up),
            effective_lateral_acceleration: (gravity - surface).dot(Vec2::new(up.y, -up.x)),
            ship_thrust_acceleration: landing::THRUST_ACCELERATION,
            spaceling_jump_speed: SurfaceSortieState::spec().jump_speed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeKind {
    Landing,
    Idle,
    Walk,
    Jump,
    Takeoff,
}

impl ProbeKind {
    pub const ALL: [Self; 5] = [
        Self::Landing,
        Self::Idle,
        Self::Walk,
        Self::Jump,
        Self::Takeoff,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeResult {
    pub kind: ProbeKind,
    pub outcome: ProbeOutcome,
    pub reason: &'static str,
    /// Measured ticks exclude prerequisite landing/transfer/settling.
    pub ticks: u64,
    /// Walk: signed surface arc distance. Jump: peak rise above initial capsule
    /// altitude. Takeoff: maximum rear-foot clearance. Other probes: None.
    pub distance: Option<f32>,
    /// Measured interval only; prerequisite damage is reported separately.
    pub metrics: SurfaceMotionMetrics,
    pub setup_metrics: SurfaceMotionMetrics,
    pub final_landing: LandingTelemetry,
    pub final_pilot: Option<PilotProbeObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotProbeObservation {
    pub grounded: bool,
    pub balanced: bool,
    pub relative_speed: f32,
    pub knockdowns: u64,
    pub last_disturbance_velocity: Option<f32>,
    pub gravity_acceleration: Vec2,
}

impl PilotProbeObservation {
    fn read(state: &SurfaceSortieState) -> Option<Self> {
        state.spaceling_snapshot(0).map(|snapshot| Self {
            grounded: snapshot.grounded(),
            balanced: snapshot.balance == SpacelingBalance::Balanced,
            relative_speed: snapshot.relative_speed,
            knockdowns: snapshot.knockdowns,
            last_disturbance_velocity: snapshot.last_knockdown.map(|event| event.velocity_change),
            gravity_acceleration: state.pilots[0].gravity,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompatibilityReport {
    pub version: u32,
    pub case: GeneratedSurfaceCase,
    pub environment: SurfaceEnvironment,
    pub probes: Vec<ProbeResult>,
}

fn step(state: &mut SurfaceSortieState, action: SurfaceSortieAction) {
    SurfaceSortieScenario::step(
        state,
        &[action.encode(PlayerId::PLAYER_1)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
}

fn settle(state: &mut SurfaceSortieState) -> bool {
    for _ in 0..600 {
        step(state, SurfaceSortieAction::default());
        if state.vehicle_settled(0) {
            return true;
        }
        if !state.vehicle_available(0) {
            return false;
        }
    }
    false
}

fn local_pilot_angle(state: &SurfaceSortieState) -> f32 {
    let frame = state.planet_motion(0);
    let offset = state.spaceling_snapshot(0).unwrap().motion.position - frame.position;
    offset.y.atan2(offset.x) - frame.angle
}

fn pilot_altitude(state: &SurfaceSortieState) -> f32 {
    state
        .spaceling_snapshot(0)
        .unwrap()
        .motion
        .position
        .distance_to(state.planet_motion(0).position)
        - state.world.planets[state.pilots[0].planet].radius * BODY_BOUNDS_RADIUS_SCALE
}

pub(super) fn run_probe(mut state: SurfaceSortieState, kind: ProbeKind) -> ProbeResult {
    let mut result = ProbeResult {
        kind,
        outcome: ProbeOutcome::Blocked,
        reason: "landing_prerequisite",
        ticks: 0,
        distance: None,
        metrics: SurfaceMotionMetrics::default(),
        setup_metrics: SurfaceMotionMetrics::default(),
        final_landing: LandingTelemetry::default(),
        final_pilot: None,
    };
    if kind == ProbeKind::Landing {
        let planet = state.world.planets[state.pilots[0].planet];
        let ship = &mut state.world.ships[0];
        let up = (ship.position + SHIP_PIVOT - planet.position).normalized();
        let center = ship.position + SHIP_PIVOT + up * 18.0;
        ship.position = center - SHIP_PIVOT;
        ship.rotation_radians += 5.0_f32.to_radians();
        ship.direction = Vec2::Y.rotate_radians(ship.rotation_radians);
        ship.velocity =
            planet_surface_velocity(&planet, center) - up * 5.0 + Vec2::new(up.y, -up.x) * 3.0;
        ship.omega = planet.wrapper_omega;
    } else {
        let landed = settle(&mut state);
        result.setup_metrics = state.pilots[0].motion_metrics;
        result.final_landing = state.pilots[0].landing;
        if !landed {
            return result;
        }
        if kind != ProbeKind::Takeoff {
            step(
                &mut state,
                SurfaceSortieAction {
                    interact_held: true,
                    ..SurfaceSortieAction::default()
                },
            );
            result.final_landing = state.pilots[0].landing;
            result.setup_metrics = state.pilots[0].motion_metrics;
            if state.location(0) != PilotLocation::OnFoot {
                result.reason = "transfer_prerequisite";
                return result;
            }
            for _ in 0..120 {
                step(&mut state, SurfaceSortieAction::default());
            }
            result.setup_metrics = state.pilots[0].motion_metrics;
            result.final_landing = state.pilots[0].landing;
            result.final_pilot = PilotProbeObservation::read(&state);
            let snapshot = state.spaceling_snapshot(0).unwrap();
            if !snapshot.grounded() || snapshot.balance != SpacelingBalance::Balanced {
                result.reason = "balanced_support_prerequisite";
                return result;
            }
        }
    }
    state.pilots[0].motion_metrics = SurfaceMotionMetrics::default();
    state.pilots[0].idle_anchor = None;
    let initial_angle = state
        .spaceling_snapshot(0)
        .map(|_| local_pilot_angle(&state));
    let initial_height = state.spaceling_snapshot(0).map(|_| pilot_altitude(&state));
    let initial_jumps = state
        .spaceling_snapshot(0)
        .map_or(0, |snapshot| snapshot.jumps);
    let mut arc = 0.0;
    let mut previous_angle = initial_angle;
    let mut distance = 0.0_f32;
    let max_ticks = match kind {
        ProbeKind::Landing => 900,
        ProbeKind::Idle | ProbeKind::Jump => 600,
        _ => 180,
    };
    for tick in 0..max_ticks {
        step(
            &mut state,
            SurfaceSortieAction {
                horizontal: if kind == ProbeKind::Walk { 1.0 } else { 0.0 },
                primary_held: kind == ProbeKind::Takeoff || (kind == ProbeKind::Jump && tick == 0),
                ..SurfaceSortieAction::default()
            },
        );
        result.ticks += 1;
        match kind {
            ProbeKind::Walk => {
                let angle = local_pilot_angle(&state);
                let delta = (angle - previous_angle.unwrap() + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                arc -= delta
                    * state.world.planets[state.pilots[0].planet].radius
                    * BODY_BOUNDS_RADIUS_SCALE;
                previous_angle = Some(angle);
                distance = arc;
            }
            ProbeKind::Jump => {
                distance = distance.max(pilot_altitude(&state) - initial_height.unwrap())
            }
            ProbeKind::Takeoff => distance = distance.max(state.pilots[0].landing.altitude),
            _ => {}
        }
        if kind == ProbeKind::Landing && state.vehicle_settled(0) {
            break;
        }
        if matches!(kind, ProbeKind::Landing | ProbeKind::Takeoff) && !state.vehicle_available(0) {
            break;
        }
    }
    result.metrics = state.pilots[0].motion_metrics;
    result.final_landing = state.pilots[0].landing;
    result.final_pilot = PilotProbeObservation::read(&state);
    result.distance =
        matches!(kind, ProbeKind::Walk | ProbeKind::Jump | ProbeKind::Takeoff).then_some(distance);
    let passed = match kind {
        ProbeKind::Landing => state.vehicle_settled(0) && result.metrics.ship_damage == 0.0,
        ProbeKind::Idle => {
            result.metrics.supported_ticks == result.ticks
                && result.metrics.knockdowns == 0
                && result.metrics.max_idle_drift < 1.0
        }
        ProbeKind::Walk => {
            distance > 5.0
                && result.metrics.supported_ticks * 10 >= result.ticks * 9
                && result.metrics.knockdowns == 0
        }
        ProbeKind::Jump => {
            distance > 0.5
                && state.spaceling_snapshot(0).unwrap().jumps == initial_jumps + 1
                && state.spaceling_snapshot(0).unwrap().grounded()
                && result.metrics.knockdowns == 0
        }
        ProbeKind::Takeoff => {
            distance > 10.0 && state.vehicle_available(0) && state.pilots[0].landing.altitude > 10.0
        }
    };
    result.outcome = if passed {
        ProbeOutcome::Passed
    } else {
        ProbeOutcome::Failed
    };
    result.reason = if passed {
        "criteria_met"
    } else {
        "criteria_not_met"
    };
    result
}
