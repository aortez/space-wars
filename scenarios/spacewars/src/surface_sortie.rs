//! Opt-in shared-world pilot/vehicle experiment. Not the ordinary capture game.

use super::*;
use engine_rapier::{
    spaceling::{
        SpacelingAssembly, SpacelingBalance, SpacelingControl, SpacelingSnapshot, SpacelingSpec,
    },
    world::PhysicsId,
};

pub mod asteroids;
mod claim;
pub mod claim_footing;
pub mod combat;
pub mod compatibility;
pub mod flight;
pub mod ground_navigation;
pub mod ground_posture;
pub mod impact;
pub mod jetpack;
mod landing;
mod landing_diagnostics;
pub mod landing_objective;
mod material;
pub mod mission;
mod motion;
mod outpost;
pub mod pilot;
pub mod pod_righting;
mod profiles;
pub mod rebuild_placement;
mod recovery;
pub mod recovery_sensors;
mod render;
pub mod return_trial;
pub(crate) mod solar;
mod travel;
pub use claim::{
    PlanetClaimObservation, PlanetClaimPhase, PlanetClaimStatus, PlanetFlagObservation,
};
pub use flight::SurfaceWingAction;
pub use landing::{LandingPhase, LandingTelemetry};
pub use material::{SurfaceMiningAction, SurfaceMiningObservation};
pub use motion::{SurfaceMotionMetrics, SurfaceMotionObservation, SurfaceMotionPreset};
pub use outpost::{CaptureStatus, OutpostId, OutpostObservation, RepairStatus};
pub use profiles::GeneratedSurfaceProfile;
pub use recovery::{SurfaceRecoveryObservation, SurfaceRecoveryStatus};
pub use solar::{SolarExposure, SolarHazard};
#[cfg(test)]
mod tests;

const CONTROL_V2: u32 = 0x5355_0002;
fn pilot_physics_id(player: PlayerId) -> PhysicsId {
    PhysicsId::new(40_000 + player.index() as u64)
}
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
    pub fn encode(self, player: PlayerId) -> Action {
        let mut payload = self.horizontal.to_le_bytes().to_vec();
        payload.extend([
            self.primary_held as u8,
            self.interact_held as u8,
            self.brake_held as u8,
        ]);
        payload.push(player.index() as u8);
        Action::scenario(CONTROL_V2, payload)
    }

    pub fn decode(action: &Action) -> Option<(PlayerId, Self)> {
        let Action::Scenario {
            kind: CONTROL_V2,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 8 || payload[4..7].iter().any(|&value| value > 1) {
            return None;
        }
        let horizontal = f32::from_le_bytes(payload[..4].try_into().ok()?);
        let player = PlayerId::from_index(payload[7] as usize)?;
        horizontal.is_finite().then_some((
            player,
            Self {
                horizontal: horizontal.clamp(-1.0, 1.0),
                primary_held: payload[4] != 0,
                interact_held: payload[5] != 0,
                brake_held: payload[6] != 0,
            },
        ))
    }
}

#[derive(Clone)]
pub struct SurfaceSortieState {
    world: SpacewarsState,
    motion_preset: SurfaceMotionPreset,
    generated_case: Option<compatibility::GeneratedSurfaceCase>,
    pilots: Vec<SurfacePilot>,
    outposts: Vec<outpost::SurfaceOutpost>,
    claims: Vec<claim::SurfacePlanetClaim>,
    mining: Option<material::SurfaceMining>,
    damage: impact::SurfaceDamageState,
    asteroids: asteroids::AsteroidPressure,
}

#[derive(Clone)]
pub(super) struct SurfacePilot {
    motion_metrics: SurfaceMotionMetrics,
    idle_anchor: Option<(bool, Vec2)>,
    input: SurfaceSortieAction,
    interact_was_held: bool,
    controls_armed: bool,
    wing_input: bool,
    flight_enabled: bool,
    pub(super) combat: Option<combat::CombatSeat>,
    damage: impact::SurfaceDamageObservation,
    transfers: u64,
    last_transfer: TransferResult,
    landing: LandingTelemetry,
    pod_righting: pod_righting::PodRightingState,
    recovery: Option<recovery::SurfaceRecovery>,
    id: SpacelingId,
    pub(super) owner: PlayerId,
    vehicle: VehicleId,
    /// Approach frame, not proof of contact or permission to board/repair.
    planet: usize,
    travel_enabled: bool,
    body: Option<SpacelingAssembly>,
    control: SpacelingControl,
    gravity: Vec2,
    facing: f32,
    jetpack_charge: Option<f32>,
    gait_phase: f32,
    pub(super) ship_gravity_delta: Vec2,
}

impl SurfacePilot {
    fn new(owner: PlayerId, planet: usize, travel_enabled: bool) -> Self {
        Self {
            id: SpacelingId(owner.index() as u64 + 1),
            owner,
            vehicle: VehicleId(owner.index()),
            planet,
            travel_enabled,
            body: None,
            control: SpacelingControl::default(),
            gravity: Vec2::ZERO,
            facing: 1.0,
            jetpack_charge: None,
            gait_phase: 0.0,
            ship_gravity_delta: Vec2::ZERO,
            motion_metrics: SurfaceMotionMetrics::default(),
            idle_anchor: None,
            input: SurfaceSortieAction::default(),
            interact_was_held: false,
            controls_armed: false,
            wing_input: false,
            flight_enabled: false,
            combat: None,
            damage: impact::SurfaceDamageObservation::default(),
            transfers: 0,
            last_transfer: TransferResult::Ready,
            landing: LandingTelemetry::default(),
            pod_righting: pod_righting::PodRightingState::default(),
            recovery: None,
        }
    }

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
            body.apply_control_with_queries(
                &mut physics.world,
                self.control,
                self.gravity,
                dt,
                !physics.material_queries_dirty,
            );
            physics
                .world
                .apply_velocity_delta(body.body(), velocity_delta, true);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceSessionObservation {
    pub version: u32,
    pub players: Vec<SurfaceSortieObservation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terrain: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceSortieObservation {
    pub version: u32,
    pub generated_case: Option<compatibility::GeneratedSurfaceCase>,
    pub travel_enabled: bool,
    pub ship_support_planet: Option<usize>,
    pub pilot_support_planet: Option<usize>,
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
    pub solar: Option<SolarExposure>,
    pub ship_available: bool,
    pub vehicle_form: ShipForm,
    pub recovery: Option<SurfaceRecoveryObservation>,
    pub access_position: Vec2,
    pub transfers: u64,
    pub last_transfer: TransferResult,
    pub controls_armed: bool,
    pub physical_bodies: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mining: Option<SurfaceMiningObservation>,
    pub landing: LandingTelemetry,
    pub outpost: Option<OutpostObservation>,
    pub outposts: Vec<OutpostObservation>,
    pub planet_claim: Option<PlanetClaimObservation>,
    pub planet_claims: Vec<PlanetClaimObservation>,
    pub motion: SurfaceMotionObservation,
    pub motion_metrics: SurfaceMotionMetrics,
}

impl SurfaceSortieState {
    pub fn player_count(&self) -> usize {
        self.pilots.len()
    }

    pub fn spaceling_snapshot(&self, player: usize) -> Option<SpacelingSnapshot> {
        self.pilots[player].snapshot(&self.world.physics)
    }

    pub fn location(&self, player: usize) -> PilotLocation {
        if self.pilots[player].body.is_some() {
            PilotLocation::OnFoot
        } else {
            PilotLocation::Aboard(self.pilots[player].vehicle)
        }
    }

    pub fn observation(&self, player: usize) -> SurfaceSortieObservation {
        let ship = &self.world.ships[self.pilots[player].vehicle.0];
        let snapshot = self.spaceling_snapshot(player);
        SurfaceSortieObservation {
            version: if self.has_material_ground() { 11 } else { 10 },
            generated_case: self.generated_case,
            travel_enabled: self.travel_enabled(),
            ship_support_planet: self.ship_support_planet(player),
            pilot_support_planet: self.pilot_support_planet(player),
            tick: self.world.tick,
            spaceling: self.pilots[player].id,
            owner: self.pilots[player].owner,
            vehicle: self.pilots[player].vehicle,
            location: self.location(player),
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
            solar: self.solar_exposure(player),
            ship_available: self.vehicle_available(player),
            vehicle_form: ship.form,
            recovery: self.pilots[player]
                .recovery
                .as_ref()
                .map(|r| r.observation()),
            access_position: self.access_position(player),
            transfers: self.pilots[player].transfers,
            last_transfer: self.pilots[player].last_transfer,
            controls_armed: self.pilots[player].controls_armed,
            physical_bodies: self.world.physics.world.body_count(),
            mining: self.mining_observation(player),
            landing: self.pilots[player].landing,
            outpost: self
                .focused_outpost(player)
                .map(|post| post.observation(&self.world.planets[post.planet], player)),
            outposts: self
                .outposts
                .iter()
                .map(|post| post.observation(&self.world.planets[post.planet], player))
                .collect(),
            planet_claim: self.claim_observation(self.motion_planet_index(player), player),
            planet_claims: (0..self.claims.len())
                .filter_map(|planet| self.claim_observation(planet, player))
                .collect(),
            motion: self.motion_observation(player),
            motion_metrics: self.pilots[player].motion_metrics,
        }
    }

    fn access_up(&self, player: usize) -> Vec2 {
        if self
            .world
            .terrain
            .planets
            .contains_key(&self.pilots[player].planet)
        {
            if let Some(hit) = self.material_access(player) {
                return hit.normal;
            }
        }
        let ship = &self.world.ships[self.pilots[player].vehicle.0];
        // A surface access point beside the *actual* ship. No elevated berth
        // or fixed planet marker; the physical landing gate is checked first.
        let local = if ship.form == ShipForm::Ship {
            Vec2::new(8.0, -5.0)
        } else {
            Vec2::new(2.8, -0.65)
        };
        let hatch = ship.position
            + physics::ship_pivot(ship.form)
            + local.rotate_radians(ship.rotation_radians);
        (hatch - self.world.planets[self.pilots[player].planet].position).normalized()
    }

    fn access_position(&self, player: usize) -> Vec2 {
        if self
            .world
            .terrain
            .planets
            .contains_key(&self.pilots[player].planet)
        {
            return self.material_access(player).map_or_else(
                || self.world.ships[self.pilots[player].vehicle.0].position,
                |hit| hit.point,
            );
        }
        let planet = &self.world.planets[self.pilots[player].planet];
        planet.position + self.access_up(player) * (planet.radius * BODY_BOUNDS_RADIUS_SCALE)
    }

    fn spec() -> SpacelingSpec {
        SpacelingSpec {
            collision_groups: physics::spaceling_collision_groups(),
            ..SpacelingSpec::default()
        }
    }

    fn vehicle_available(&self, player: usize) -> bool {
        let ship = &self.world.ships[self.pilots[player].vehicle.0];
        !ship.dead
            && ship.form == ShipForm::Ship
            && ship.owner_id == self.pilots[player].owner.index()
    }

    fn vehicle_accessible(&self, player: usize) -> bool {
        let ship = &self.world.ships[self.pilots[player].vehicle.0];
        self.vehicle_available(player)
            || (self.pilots[player].recovery.is_some()
                && !ship.dead
                && ship.owner_id == self.pilots[player].owner.index())
    }

    fn vehicle_settled(&self, player: usize) -> bool {
        self.pilots[player].landing.phase == LandingPhase::Landed
    }

    /// Shared by the action gate and read-only controller sensors.
    fn transfer_readiness(&self, player: usize) -> TransferResult {
        if !self.vehicle_accessible(player) {
            return TransferResult::VehicleUnavailable;
        }
        if !self.vehicle_settled(player) {
            return TransferResult::ShipNotSettled;
        }
        if self
            .world
            .terrain
            .planets
            .contains_key(&self.pilots[player].planet)
            && self.material_access(player).is_none()
        {
            return TransferResult::ExitBlocked;
        }
        if let Some(snapshot) = self.spaceling_snapshot(player) {
            if snapshot
                .motion
                .position
                .distance_to(self.access_position(player))
                > BOARDING_RANGE
            {
                return TransferResult::TooFar;
            }
            let relative = snapshot.motion.linear_velocity
                - motion::point_velocity(self.planet_motion(player), snapshot.motion.position);
            if !snapshot.grounded()
                || !snapshot.support.is_some_and(|support| {
                    physics::is_planet_surface_support(support.collider, self.pilots[player].planet)
                })
                || snapshot.balance != SpacelingBalance::Balanced
                || relative.length() > SETTLED_SPEED
            {
                return TransferResult::MustBeSupported;
            }
        } else {
            let spec = Self::spec();
            let up = self.access_up(player);
            let position = self.access_position(player) + up * (spec.half_height() + 0.12);
            let angle = rotation_for_direction(up);
            // Rapier's broad-phase queries describe the completed step. A capsule
            // inserted by another seat's transfer this tick is not indexed yet.
            // Reserve a conservative capsule-sized clearance using the bounded
            // pilot list and authoritative body poses, without another physics step.
            if self.pilots.iter().any(|pilot| {
                pilot.snapshot(&self.world.physics).is_some_and(|snapshot| {
                    snapshot.motion.position.distance_to(position) < spec.half_height() * 2.0 + 0.04
                })
            }) {
                return TransferResult::ExitBlocked;
            }
            if !self.world.physics.world.capsule_is_clear(
                position,
                angle,
                spec.half_segment,
                spec.radius + 0.04,
                spec.collision_groups,
            ) {
                return TransferResult::ExitBlocked;
            }
        }
        TransferResult::Ready
    }

    fn try_transfer(&mut self, player: usize) -> TransferResult {
        let readiness = self.transfer_readiness(player);
        if readiness != TransferResult::Ready {
            return readiness;
        }
        if self.pilots[player].body.is_some() {
            // The creature remains alive/owned; only its external physical
            // representation disappears while it occupies the existing ship.
            if let Some(pack) = self.pilots[player]
                .body
                .as_ref()
                .and_then(|body| body.jetpack())
            {
                self.pilots[player].jetpack_charge = Some(pack.charge);
            }
            self.world
                .physics
                .world
                .remove_entity(pilot_physics_id(self.pilots[player].owner));
            self.pilots[player].body = None;
            TransferResult::Boarded
        } else {
            let spec = Self::spec();
            let up = self.access_up(player);
            let position = self.access_position(player) + up * (spec.half_height() + 0.12);
            let angle = rotation_for_direction(up);
            let Some(mut body) = SpacelingAssembly::insert(
                &mut self.world.physics.world,
                pilot_physics_id(self.pilots[player].owner),
                position,
                angle,
                spec,
            ) else {
                return TransferResult::ExitBlocked;
            };
            if let Some(charge) = self.pilots[player].jetpack_charge {
                assert!(body.equip_jetpack(charge));
            }
            let surface = self.planet_motion(player);
            self.world.physics.world.set_velocity(
                body.body(),
                motion::point_velocity(surface, position),
                surface.angular_velocity,
                true,
            );
            self.pilots[player].body = Some(body);
            TransferResult::Exited
        }
    }
}

impl Scenario for SurfaceSortieScenario {
    type State = SurfaceSortieState;
    type Config = SurfaceMotionPreset;

    fn init(motion_preset: SurfaceMotionPreset, seed: u64) -> SurfaceSortieState {
        if matches!(
            motion_preset,
            SurfaceMotionPreset::Generated | SurfaceMotionPreset::GeneratedSurfaceV1
        ) {
            return compatibility::GeneratedSurfaceCase::new(seed, 0, 0)
                .with_profile(if motion_preset == SurfaceMotionPreset::Generated {
                    GeneratedSurfaceProfile::Raw
                } else {
                    GeneratedSurfaceProfile::SurfaceV1
                })
                .init()
                .expect("default generated world has a planet");
        }
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
        let mut planet = PlanetState {
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
        world.sun = motion_preset.configure(&mut planet);
        world.planets = vec![planet];
        world.rover_builds = vec![RoverBuildState::default()];
        Self::on_surface(world, motion_preset, 0, Vec2::Y, Some(-0.34))
    }

    fn step(state: &mut SurfaceSortieState, actions: &[Action], dt: Duration) -> StepResult {
        // This fixture uses the same 60 Hz fixed-step contract as Spacewars.
        if dt.is_zero() {
            return StepResult::default();
        }
        state
            .asteroids
            .begin_step(&mut state.world, dt.as_secs_f32());
        // Invalidate edited support before either seat can transfer or use it.
        let footings = state.material_footings();
        let prepared = SpacewarsScenario::prepare_terrain(&mut state.world, &[]);
        state.reconcile_material_support(footings);
        state.read_mining_actions(actions);
        state.read_wing_actions(actions);
        state.read_impact_actions(actions);
        state.read_weapon_actions(actions);
        for (player, input) in actions.iter().filter_map(SurfaceSortieAction::decode) {
            if let Some(pilot) = state.pilots.get_mut(player.index()) {
                pilot.input = input;
            }
        }
        // Bounded seat-local samples; no per-tick actor collection allocation.
        let samples: [_; SPACEWARS_PLAYER_COUNT] = std::array::from_fn(|player| {
            (player < state.player_count()).then(|| motion::StepSample::read(state, player))
        });
        let mut effective_inputs = [SurfaceSortieAction::default(); SPACEWARS_PLAYER_COUNT];
        for (player, effective) in effective_inputs
            .iter_mut()
            .enumerate()
            .take(state.player_count())
        {
            let input = state.pilots[player].input;
            if !state.pilots[player].controls_armed {
                state.pilots[player].controls_armed = input == SurfaceSortieAction::default()
                    && !state.pilots[player].wing_input
                    && !state.damage.held[player]
                    && state.pilots[player]
                        .combat
                        .as_ref()
                        .is_none_or(|c| c.input == combat::SurfaceWeaponAction::default());
            } else {
                *effective = input;
                if state.update_scuttle_input(player, input, dt) {
                    *effective = SurfaceSortieAction::default();
                } else if input.interact_held && !state.pilots[player].interact_was_held {
                    let result = state.try_transfer(player);
                    state.pilots[player].last_transfer = result;
                    if matches!(result, TransferResult::Exited | TransferResult::Boarded) {
                        state.pilots[player].transfers += 1;
                        state.pilots[player].controls_armed = false;
                        *effective = SurfaceSortieAction::default();
                    }
                }
            }
            let pilot = &mut state.pilots[player];
            pilot.interact_was_held = input.interact_held;
            let on_foot = pilot.body.is_some();
            pilot.control = if on_foot {
                SpacelingControl {
                    walk: effective.horizontal,
                    jump_held: effective.primary_held,
                }
            } else {
                SpacelingControl::default()
            };
            if effective.horizontal.abs() > 0.01 {
                pilot.facing = effective.horizontal.signum();
            }
            let ship = &mut state.world.ships[pilot.vehicle.0];
            ship.set_thrust(if !on_foot && !ship.dead && effective.primary_held {
                1.0
            } else {
                0.0
            });
            ship.set_turn(if on_foot || ship.dead {
                0.0
            } else {
                effective.horizontal
            });
            ship.set_brake(if !on_foot && !ship.dead && effective.brake_held {
                1.0
            } else {
                0.0
            });
            let weapons = pilot
                .combat
                .as_ref()
                .map_or(combat::SurfaceWeaponAction::default(), |c| c.input);
            let armed =
                pilot.controls_armed && !on_foot && !ship.dead && ship.form == ShipForm::Ship;
            ship.set_laser(armed && weapons.laser);
            ship.set_cannon(armed && weapons.cannon);
        }
        // Schedule terrain, solve gravity, and step Rapier exactly once for all seats.
        let dt = dt.as_secs_f32();
        state.update_surface_wings(dt);
        let damage_before = state.damage_sample();
        for planet in &mut state.world.planets {
            state.motion_preset.advance(planet, state.world.sun, dt);
        }
        let result = SpacewarsScenario::step_with_surface_pilots(
            &mut state.world,
            &[],
            Duration::from_secs_f32(dt),
            &mut state.pilots,
            Some(prepared),
            Some(&mut state.asteroids),
        );
        state.reconcile_recovery_vehicles();
        state.record_surface_damage(damage_before);
        for (player, (before, effective)) in samples.into_iter().zip(effective_inputs).enumerate() {
            let Some(before) = before else { continue };
            let pilot = &mut state.pilots[player];
            pilot.select_approach_planet(&state.world.physics, &state.world.planets);
            pilot.landing.update(
                &state.world.physics,
                pilot.vehicle.0,
                pilot.planet,
                &state.world.planets[pilot.planet],
                &state.world.ships[pilot.vehicle.0],
                dt,
            );
            if let Some(snapshot) = pilot.snapshot(&state.world.physics) {
                pilot.gait_phase = (pilot.gait_phase + snapshot.relative_speed.abs() * dt * 5.0)
                    .rem_euclid(std::f32::consts::TAU);
            }
            state.record_motion_step(player, before, effective);
        }
        // Resolve all claimants together, then service each eligible vehicle once.
        state.update_outpost(Duration::from_secs_f32(dt));
        state.update_planet_claims(Duration::from_secs_f32(dt));
        state.update_recovery(Duration::from_secs_f32(dt));
        state.update_mining();
        result
    }

    fn observe(state: &SurfaceSortieState) -> Observation {
        Observation {
            payload: serde_json::to_vec(&SurfaceSessionObservation {
                version: if state.has_material_ground() { 11 } else { 10 },
                players: (0..state.player_count())
                    .map(|player| state.observation(player))
                    .collect(),
                terrain: terrain::observation(&state.world).payload,
            })
            .expect("finite sortie observation"),
        }
    }

    fn render_frame(state: &SurfaceSortieState) -> RenderFrame {
        render::frame(state, 0)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

impl SurfaceSortieScenario {
    fn on_surface(
        mut world: SpacewarsState,
        motion_preset: SurfaceMotionPreset,
        planet_index: usize,
        up: Vec2,
        terminal_angle: Option<f32>,
    ) -> SurfaceSortieState {
        let planet = world.planets[planet_index];
        // Begin just above the rear feet. Only initial placement is prescribed;
        // all subsequent motion is dynamic, including the unoccupied ship.
        let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
        world.ships[0].position = center - SHIP_PIVOT;
        world.ships[0].rotation_radians = rotation_for_direction(up);
        world.ships[0].direction = up;
        world.ships[0].velocity = motion_preset.initial_velocity(&planet)
            + Vec2::new(-(center - planet.position).y, (center - planet.position).x)
                * planet.wrapper_omega;
        world.ships[0].life = world.ships[0].life_max * 0.75;
        world.physics = physics::SpacewarsPhysics::new(
            world.config.universe_radius as f32,
            &world.ships,
            world.sun,
            &world.planets,
        );
        world.physics.enable_surface_sortie(&[0], &world.ships);
        world.spaceport_contacts.clear();
        let outposts = terminal_angle
            .into_iter()
            .map(|angle| {
                assert!(
                    world
                        .physics
                        .insert_surface_terminal(planet_index, planet.radius, angle)
                );
                outpost::SurfaceOutpost::new(
                    OutpostId(planet_index as u64 + 1),
                    planet_index,
                    angle,
                )
            })
            .collect();
        SurfaceSortieState {
            world,
            motion_preset,
            generated_case: None,
            pilots: vec![SurfacePilot::new(PlayerId::PLAYER_1, planet_index, false)],
            outposts,
            claims: Vec::new(),
            mining: None,
            damage: impact::SurfaceDamageState::default(),
            asteroids: asteroids::AsteroidPressure::default(),
        }
    }

    pub fn player_frame(state: &SurfaceSortieState, player: usize) -> RenderFrame {
        render::frame(state, player)
    }

    /// Fixed-scale north-up world context; the client overlays this second frame.
    pub fn minimap_frame(
        state: &SurfaceSortieState,
        player: usize,
        viewport_aspect: f32,
    ) -> RenderFrame {
        render::minimap(state, player, viewport_aspect)
    }
}
