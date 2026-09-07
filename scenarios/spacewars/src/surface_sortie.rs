//! Opt-in shared-world pilot/vehicle experiment. Not the ordinary capture game.

use super::*;
use engine_rapier::{
    spaceling::{
        SpacelingAssembly, SpacelingBalance, SpacelingControl, SpacelingSnapshot, SpacelingSpec,
    },
    world::PhysicsId,
};

mod landing;
mod outpost;
mod render;
pub use landing::{LandingPhase, LandingTelemetry};
pub use outpost::{CaptureStatus, OutpostId, OutpostObservation, RepairStatus};
#[cfg(test)]
mod tests;

const CONTROL_V1: u32 = 0x5355_0001;
const PILOT_PHYSICS_ID: PhysicsId = PhysicsId::new(40_000);
const SURFACE_RADIUS: f32 = 60.0;
const BOARDING_RANGE: f32 = 3.0;
const SETTLED_SPEED: f32 = 2.0;

pub struct SurfaceSortieScenario;

/// Persistent creature identity, including while there is no external body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpacelingId(pub u64);

/// Vehicle identity is independent of the owning player and occupying creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VehicleId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotLocation {
    Aboard(VehicleId),
    OnFoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferResult {
    Ready,
    Exited,
    Boarded,
    ShipNotSettled,
    ExitBlocked,
    TooFar,
    MustBeSupported,
    VehicleUnavailable,
}

impl TransferResult {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready for a surface sortie",
            Self::Exited => "Disembarked; B / X boards at the access marker",
            Self::Boarded => "Aboard the same ship; ready to pilot",
            Self::ShipNotSettled => "Land rear-first and settle before entering or exiting",
            Self::ExitBlocked => "Surface access blocked; no safe exit",
            Self::TooFar => "Return to the cyan hatch beside the landed ship",
            Self::MustBeSupported => "Stand and settle at the access marker to board",
            Self::VehicleUnavailable => "Vehicle unavailable; restart this experiment",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SurfaceSortieAction {
    pub horizontal: f32,
    /// Thrust aboard; fresh-press jump on foot.
    pub primary_held: bool,
    pub interact_held: bool,
    pub brake_held: bool,
}

impl SurfaceSortieAction {
    pub fn encode(self) -> Action {
        let mut payload = self.horizontal.to_le_bytes().to_vec();
        payload.extend([
            self.primary_held as u8,
            self.interact_held as u8,
            self.brake_held as u8,
        ]);
        Action::scenario(CONTROL_V1, payload)
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario {
            kind: CONTROL_V1,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 7 || payload[4..].iter().any(|&value| value > 1) {
            return None;
        }
        let horizontal = f32::from_le_bytes(payload[..4].try_into().ok()?);
        horizontal.is_finite().then_some(Self {
            horizontal: horizontal.clamp(-1.0, 1.0),
            primary_held: payload[4] != 0,
            interact_held: payload[5] != 0,
            brake_held: payload[6] != 0,
        })
    }
}

pub struct SurfaceSortieState {
    world: SpacewarsState,
    pilot: SurfacePilot,
    input: SurfaceSortieAction,
    interact_was_held: bool,
    controls_armed: bool,
    transfers: u64,
    last_transfer: TransferResult,
    landing: LandingTelemetry,
    outpost: outpost::SurfaceOutpost,
}

pub(super) struct SurfacePilot {
    id: SpacelingId,
    owner: PlayerId,
    vehicle: VehicleId,
    body: Option<SpacelingAssembly>,
    control: SpacelingControl,
    gravity: Vec2,
    facing: f32,
    gait_phase: f32,
    pub(super) ship_gravity_delta: Vec2,
}

impl SurfacePilot {
    pub(super) fn snapshot(
        &self,
        physics: &physics::SpacewarsPhysics,
    ) -> Option<SpacelingSnapshot> {
        self.body.as_ref()?.snapshot(&physics.world)
    }

    pub(super) fn apply_gravity_and_control(
        &mut self,
        physics: &mut physics::SpacewarsPhysics,
        velocity_delta: Vec2,
        dt: f32,
    ) {
        if let Some(body) = self.body.as_mut() {
            self.gravity = velocity_delta / dt;
            body.apply_control(&mut physics.world, self.control, self.gravity, dt);
            physics
                .world
                .apply_velocity_delta(body.body(), velocity_delta, true);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceSortieObservation {
    pub version: u32,
    pub tick: u64,
    pub spaceling: SpacelingId,
    pub owner: PlayerId,
    pub vehicle: VehicleId,
    pub location: PilotLocation,
    pub position: Vec2,
    pub velocity: Vec2,
    pub angle: f32,
    pub grounded: bool,
    pub balance: &'static str,
    pub jumps: u64,
    pub ship_position: Vec2,
    pub ship_health: f32,
    pub ship_available: bool,
    pub access_position: Vec2,
    pub transfers: u64,
    pub last_transfer: TransferResult,
    pub controls_armed: bool,
    pub physical_bodies: usize,
    pub landing: LandingTelemetry,
    pub outpost: OutpostObservation,
}

impl SurfaceSortieState {
    pub fn spaceling_snapshot(&self) -> Option<SpacelingSnapshot> {
        self.pilot.snapshot(&self.world.physics)
    }

    pub fn location(&self) -> PilotLocation {
        if self.pilot.body.is_some() {
            PilotLocation::OnFoot
        } else {
            PilotLocation::Aboard(self.pilot.vehicle)
        }
    }

    pub fn observation(&self) -> SurfaceSortieObservation {
        let ship = &self.world.ships[self.pilot.vehicle.0];
        let snapshot = self.spaceling_snapshot();
        SurfaceSortieObservation {
            version: 3,
            tick: self.world.tick,
            spaceling: self.pilot.id,
            owner: self.pilot.owner,
            vehicle: self.pilot.vehicle,
            location: self.location(),
            position: snapshot.map_or(ship.position, |s| s.motion.position),
            velocity: snapshot.map_or(ship.velocity, |s| s.motion.linear_velocity),
            angle: snapshot.map_or(ship.rotation_radians, |s| s.motion.angle),
            grounded: snapshot.is_some_and(SpacelingSnapshot::grounded),
            balance: snapshot.map_or("ABOARD", |s| match s.balance {
                SpacelingBalance::Balanced => "BALANCED",
                SpacelingBalance::KnockedDown => "KNOCKED DOWN",
                SpacelingBalance::Recovering => "RECOVERING",
            }),
            jumps: snapshot.map_or(0, |s| s.jumps),
            ship_position: ship.position,
            ship_health: ship.life,
            ship_available: self.vehicle_available(),
            access_position: self.access_position(),
            transfers: self.transfers,
            last_transfer: self.last_transfer,
            controls_armed: self.controls_armed,
            physical_bodies: self.world.physics.world.body_count(),
            landing: self.landing,
            outpost: self
                .outpost
                .observation(&self.world.planets[self.outpost.planet]),
        }
    }

    fn access_up(&self) -> Vec2 {
        let ship = &self.world.ships[self.pilot.vehicle.0];
        // A surface access point beside the *actual* ship. No elevated berth
        // or fixed planet marker; the physical landing gate is checked first.
        let hatch =
            ship.position + SHIP_PIVOT + Vec2::new(8.0, -5.0).rotate_radians(ship.rotation_radians);
        (hatch - self.world.planets[0].position).normalized()
    }

    fn access_position(&self) -> Vec2 {
        let planet = &self.world.planets[0];
        planet.position + self.access_up() * (planet.radius * BODY_BOUNDS_RADIUS_SCALE)
    }

    fn spec() -> SpacelingSpec {
        SpacelingSpec {
            collision_groups: physics::spaceling_collision_groups(),
            ..SpacelingSpec::default()
        }
    }

    fn vehicle_available(&self) -> bool {
        let ship = &self.world.ships[self.pilot.vehicle.0];
        !ship.dead && ship.form == ShipForm::Ship && ship.owner_id == self.pilot.owner.index()
    }

    fn vehicle_settled(&self) -> bool {
        self.landing.phase == LandingPhase::Landed
    }

    fn try_transfer(&mut self) -> TransferResult {
        if !self.vehicle_available() {
            return TransferResult::VehicleUnavailable;
        }
        if !self.vehicle_settled() {
            return TransferResult::ShipNotSettled;
        }
        if let Some(snapshot) = self.spaceling_snapshot() {
            if snapshot.motion.position.distance_to(self.access_position()) > BOARDING_RANGE {
                return TransferResult::TooFar;
            }
            let relative = snapshot.motion.linear_velocity
                - planet_surface_velocity(&self.world.planets[0], snapshot.motion.position);
            if !snapshot.grounded()
                || snapshot.balance != SpacelingBalance::Balanced
                || relative.length() > SETTLED_SPEED
            {
                return TransferResult::MustBeSupported;
            }
            // The creature remains alive/owned; only its external physical
            // representation disappears while it occupies the existing ship.
            self.world.physics.world.remove_entity(PILOT_PHYSICS_ID);
            self.pilot.body = None;
            TransferResult::Boarded
        } else {
            let spec = Self::spec();
            let up = self.access_up();
            let position = self.access_position() + up * (spec.half_height() + 0.12);
            let angle = rotation_for_direction(up);
            if !self.world.physics.world.capsule_is_clear(
                position,
                angle,
                spec.half_segment,
                spec.radius + 0.04,
                spec.collision_groups,
            ) {
                return TransferResult::ExitBlocked;
            }
            let Some(body) = SpacelingAssembly::insert(
                &mut self.world.physics.world,
                PILOT_PHYSICS_ID,
                position,
                angle,
                spec,
            ) else {
                return TransferResult::ExitBlocked;
            };
            self.world.physics.world.set_velocity(
                body.body(),
                planet_surface_velocity(&self.world.planets[0], position),
                self.world.planets[0].wrapper_omega,
                true,
            );
            self.pilot.body = Some(body);
            TransferResult::Exited
        }
    }
}

impl Scenario for SurfaceSortieScenario {
    type State = SurfaceSortieState;
    type Config = ();

    fn init(_config: (), seed: u64) -> SurfaceSortieState {
        let mut world = SpacewarsScenario::init(
            SpacewarsConfig {
                universe_radius: 500,
                use_planets: false,
                use_starfield: false,
                asteroid_probability_per_sec: 0.0,
                ..SpacewarsConfig::default()
            },
            seed,
        );
        let planet = PlanetState {
            position: Vec2::splat(500.0),
            radius: SURFACE_RADIUS,
            // Match 18 units/s² at the feet using the existing fixed-tick
            // Spacewars gravity scale, without changing the normal game.
            mass: 18.0 / 60.0 / GRAVITY * (SURFACE_RADIUS * BODY_BOUNDS_RADIUS_SCALE + 0.9).powi(2),
            color: Color::scale_255(30.0, 55.0, 65.0),
            owner_id: None,
            capturing_player_id: None,
            previous_docked_ship: None,
            dock_contest_time: 0.0,
            taking_ownership_time: 0.0,
            building_new_ship_time: 0.0,
            orbit_radius: 0.0,
            orbit_angle: 0.0,
            orbit_omega: 0.0,
            wrapper_angle: std::f32::consts::FRAC_PI_2,
            wrapper_omega: 0.015,
        };
        // Begin just above the ground on the rear feet; Rapier settles the
        // vehicle during the first few ticks, just as after a flown landing.
        let center = planet.position + Vec2::Y * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
        world.ships[0].position = center - SHIP_PIVOT;
        world.ships[0].rotation_radians = 0.0;
        world.ships[0].direction = Vec2::Y;
        world.ships[0].velocity = planet_surface_velocity(&planet, center);
        world.ships[0].life = world.ships[0].life_max * 0.75;
        // Keep the second vehicle far from the single-player fixture. Its
        // existence does not imply that this pilot may occupy another owner's ship.
        world.ships[1].position = Vec2::new(850.0, 850.0);
        world.ships[1].velocity = Vec2::new(-6.0, 6.0);
        world.planets = vec![planet];
        world.rover_builds = vec![RoverBuildState::default()];
        world.physics = physics::SpacewarsPhysics::new(500.0, &world.ships, None, &world.planets);
        world.physics.enable_surface_landing(0, &world.ships[0]);
        world.spaceport_contacts.clear();
        let outpost = outpost::SurfaceOutpost::new(OutpostId(1), 0, -0.34);
        assert!(world.physics.insert_surface_terminal(
            outpost.planet,
            planet.radius,
            outpost.local_angle
        ));
        SurfaceSortieState {
            world,
            pilot: SurfacePilot {
                id: SpacelingId(1),
                owner: PlayerId::PLAYER_1,
                vehicle: VehicleId(0),
                body: None,
                control: SpacelingControl::default(),
                gravity: Vec2::ZERO,
                facing: 1.0,
                gait_phase: 0.0,
                ship_gravity_delta: Vec2::ZERO,
            },
            input: SurfaceSortieAction::default(),
            interact_was_held: false,
            controls_armed: false,
            transfers: 0,
            last_transfer: TransferResult::Ready,
            landing: LandingTelemetry::default(),
            outpost,
        }
    }

    fn step(state: &mut SurfaceSortieState, actions: &[Action], dt: Duration) -> StepResult {
        // This fixture uses the same 60 Hz fixed-step contract as Spacewars.
        if dt.is_zero() {
            return StepResult::default();
        }
        if let Some(input) = actions
            .iter()
            .filter_map(SurfaceSortieAction::decode)
            .next_back()
        {
            state.input = input;
        }
        let input = state.input;
        let mut effective = SurfaceSortieAction::default();
        if !state.controls_armed {
            state.controls_armed = input == SurfaceSortieAction::default();
        } else {
            effective = input;
            if input.interact_held && !state.interact_was_held {
                state.last_transfer = state.try_transfer();
                if matches!(
                    state.last_transfer,
                    TransferResult::Exited | TransferResult::Boarded
                ) {
                    state.transfers += 1;
                    state.controls_armed = false;
                    effective = SurfaceSortieAction::default();
                }
            }
        }
        state.interact_was_held = input.interact_held;
        let on_foot = state.pilot.body.is_some();
        state.pilot.control = if on_foot {
            SpacelingControl {
                walk: effective.horizontal,
                jump_held: effective.primary_held,
            }
        } else {
            SpacelingControl::default()
        };
        if effective.horizontal.abs() > 0.01 {
            state.pilot.facing = effective.horizontal.signum();
        }
        let ship = &mut state.world.ships[state.pilot.vehicle.0];
        ship.set_thrust(if !on_foot && effective.primary_held {
            1.0
        } else {
            0.0
        });
        ship.set_turn(if on_foot { 0.0 } else { effective.horizontal });
        ship.set_brake(if !on_foot && effective.brake_held {
            1.0
        } else {
            0.0
        });
        ship.set_laser(false);
        ship.set_cannon(false);
        // Rotation is kinematic terrain motion. No spaceling pose/velocity
        // transport is performed here or in the controller.
        let dt = dt.as_secs_f32();
        state.world.planets[0].wrapper_angle = (state.world.planets[0].wrapper_angle
            + state.world.planets[0].wrapper_omega * dt)
            .rem_euclid(std::f32::consts::TAU);
        let result = SpacewarsScenario::step_with_surface_pilot(
            &mut state.world,
            &[],
            Duration::from_secs_f32(dt),
            Some(&mut state.pilot),
        );
        state.landing.update(
            &state.world.physics,
            state.pilot.vehicle.0,
            &state.world.planets[0],
            &state.world.ships[state.pilot.vehicle.0],
            dt,
        );
        if let Some(snapshot) = state.spaceling_snapshot() {
            state.pilot.gait_phase = (state.pilot.gait_phase
                + snapshot.relative_speed.abs() * dt * 5.0)
                .rem_euclid(std::f32::consts::TAU);
        }
        // Services consume completed physical support/landing, never create it.
        state.update_outpost(Duration::from_secs_f32(dt));
        result
    }

    fn observe(state: &SurfaceSortieState) -> Observation {
        Observation {
            payload: serde_json::to_vec(&state.observation()).expect("finite sortie observation"),
        }
    }

    fn render_frame(state: &SurfaceSortieState) -> RenderFrame {
        render::frame(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

impl SurfaceSortieScenario {
    /// Fixed-scale north-up world context; the client overlays this second frame.
    pub fn minimap_frame(state: &SurfaceSortieState, viewport_aspect: f32) -> RenderFrame {
        render::minimap(state, viewport_aspect)
    }
}
