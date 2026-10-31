//! C2 headless flight foundation: real physics, explicit actions, independent
//! outcomes and replayable diagnostic tasks. No learned/native pilot is called.
//! Exposes both raw inspection data and a separate versioned policy adapter.
use super::*;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderShape,
    ColliderSpec,
    polar::{PolarConfig, PolarSensor},
};
use engine_terrain::TerrainEdit;
use flight::FlightControlMode;
use flight_sensing::FlightSurroundings;

mod task;
pub use task::*;
pub mod generation;
pub mod motor;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod tests;

fn finite(v: Vec2) -> bool {
    v.x.is_finite() && v.y.is_finite()
}

impl SurfaceSortieScenario {
    /// A practice world under the learned flight envelope: the material planet
    /// stationary and without gravity, with every listed pilot starting in
    /// open-wing flight under learned flight control. Recovery, claims and
    /// outposts stay as the material world provides them; the headless
    /// environment removes those separately.
    pub fn init_learned_flight(
        seed: u64,
        players: usize,
        starts: &[(PlayerId, pilot::MaterialFlightStart)],
    ) -> SurfaceSortieState {
        let mut state = Self::init_material_flight(seed, players, starts);
        state.world.planets[0].wrapper_omega = 0.0;
        state.world.planets[0].mass = 0.0;
        for (owner, _) in starts {
            state.set_flight_control_mode(owner.index(), FlightControlMode::Learned);
        }
        state
    }
}

pub fn flight_sensor_config() -> PolarConfig {
    PolarConfig {
        sectors: 64,
        radial_edges: vec![
            0., 2., 4., 6., 8., 11., 14., 18., 23., 29., 36., 44., 54., 66., 80., 96., 114., 134.,
            156., 180., 206., 234., 266., 300.,
        ],
        angular_samples: 2,
        radial_samples: 2,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightFrame {
    pub world_tick: u64,
    pub origin: Vec2,
    pub angle: f32,
    pub center_of_mass: Vec2,
    pub velocity: Vec2,
    pub spin: f32,
    pub gravity: Vec2,
    pub health: f32,
    pub target_position: Vec2,
    pub target_velocity: Vec2,
    pub errors: ArrivalErrors,
    /// Diagnostic: the standard mode's potential assist, not applied force.
    pub potential_landing_assist: f32,
    pub near_contact: bool,
}

#[derive(Debug, Serialize)]
pub struct FlightObservation {
    pub version: u32,
    pub frame: FlightFrame,
    pub progress: FlightProgress,
    pub arrival: ArrivalCriteria,
    pub remaining_ticks: u32,
    pub previous_action: FlightAction,
    pub control_mode: FlightControlMode,
    /// Raw diagnostic observations. Do not feed this entire record to a policy:
    /// it includes legacy landing telemetry; use policy_observation for inputs.
    pub surroundings: FlightSurroundings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightTraceStep {
    /// None records a rejected controller action, never a substituted action.
    pub action: Option<FlightAction>,
    pub terrain_edits: Vec<TerrainEdit>,
    pub frame: Option<FlightFrame>,
    pub progress: FlightProgress,
    pub evidence: FlightEvidence,
    pub error: Option<String>,
}

/// Compact evidence: observations can be regenerated from setup and actions.
/// This is replay provenance, not an exact PPO/environment restart checkpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightTrace {
    pub version: u32,
    pub task: FlightTask,
    pub initial: FlightFrame,
    pub steps: Vec<FlightTraceStep>,
}

pub struct FlightEnvironment {
    state: SurfaceSortieState,
    sensor: PolarSensor,
    goal_start: Vec2,
    /// Set when the goal rests in a spinning planet frame.
    frame_goal: Option<FrameGoal>,
    frame: FlightFrame,
    progress: FlightProgress,
    previous_action: FlightAction,
    pending_edits: Vec<TerrainEdit>,
    trace: FlightTrace,
}

impl FlightEnvironment {
    pub fn new(task: FlightTask) -> Result<Self, &'static str> {
        task.validate()?;
        let start = task.start;
        let mut state = match task.transfer {
            Some(transfer) => transfer_state(start, transfer)?,
            None => SurfaceSortieScenario::init_material_flight(
                start.seed,
                1,
                &[(
                    PlayerId::PLAYER_1,
                    pilot::MaterialFlightStart {
                        bearing: start.bearing,
                        altitude: start.altitude,
                        radial_speed: 0.0,
                        lateral_speed: 0.0,
                        heading_offset: start.heading_offset,
                    },
                )],
            ),
        };
        state.set_flight_control_mode(0, FlightControlMode::Learned);
        // No recovery/ejection/rebuild or repair can obscure a failed flight.
        state.pilots[0].recovery = None;
        let frame_planet = task.transfer.map_or(0, |t| t.start_planet);
        if task.transfer.is_none() {
            state.claims.clear();
            state.outposts.clear();
            state.world.planets[0].wrapper_omega = start.planet_spin;
            if !start.gravity {
                state.world.planets[0].mass = 0.0;
            }
        }
        let body = state.world.physics.ship_body(0);
        let origin = state
            .world
            .physics
            .world
            .motion(body)
            .ok_or("missing ship")?
            .position;
        let com = state
            .world
            .physics
            .world
            .center_of_mass(body)
            .ok_or("missing COM")?;
        let planet = state.world.planets[frame_planet];
        let r = com - planet.position;
        // An arena planet orbits; its physics body carries no velocity before
        // the first step, so the frame velocity comes from the orbit itself.
        let velocity = if task.transfer.is_some() {
            crate::planet_surface_velocity(&planet, com) + start.relative_velocity
        } else {
            state.planet_motion(0).linear_velocity
                + Vec2::new(-r.y, r.x) * planet.wrapper_omega
                + start.relative_velocity
        };
        if !state
            .world
            .physics
            .world
            .set_velocity(body, velocity, start.spin, true)
        {
            return Err("cannot set initial ship motion");
        }
        state.world.ships[0].velocity = velocity;
        state.world.ships[0].omega =
            physics::control_angular_velocity(&state.world.ships[0], start.spin);
        for (index, obstacle) in task.obstacles.iter().enumerate() {
            let entity = PhysicsId::new(9_100_000 + index as u64);
            let collider = ColliderSpec {
                shape: ColliderShape::Cuboid {
                    half_width: obstacle.half_extents.x,
                    half_height: obstacle.half_extents.y,
                },
                ..ColliderSpec::ball(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 1.0)
            };
            if !state.world.physics.world.insert_body(
                PhysicsBodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: origin + obstacle.offset,
                    angle: obstacle.angle,
                    ..Default::default()
                },
                &[collider],
            ) {
                return Err("cannot insert diagnostic obstacle");
            }
        }
        for edit in &task.terrain_edits {
            state
                .world
                .queue_planet_edit(0, *edit)
                .map_err(|_| "invalid terrain edit")?;
        }
        // A real neutral tick arms the ordinary controls and synchronizes the
        // material query index. Do not teleport after this completed step.
        SurfaceSortieScenario::step(&mut state, &[], FLIGHT_DT);
        let initial_com = state
            .world
            .physics
            .world
            .center_of_mass(body)
            .ok_or("missing initial COM")?;
        // A transfer goal is the task's offset from its planet's center; a
        // practice goal above a spinning planet is placed from the ship, then
        // rides the surface. A practice goal above a still planet keeps the
        // original arithmetic so archived traces replay exactly.
        let frame_goal = match task.transfer {
            Some(transfer) => {
                let frame = motion::SurfaceFrame::read(&state.world.physics, transfer.goal_planet);
                Some(FrameGoal {
                    planet: transfer.goal_planet,
                    start_angle: frame.angle,
                    offset: task.goal_offset,
                    velocity: Vec2::ZERO,
                })
            }
            None if start.planet_spin != 0.0 => {
                let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
                Some(FrameGoal {
                    planet: 0,
                    start_angle: frame.angle,
                    offset: initial_com + task.goal_offset - frame.position,
                    velocity: task.goal_velocity,
                })
            }
            None => None,
        };
        let (goal_start, goal_velocity) = match frame_goal {
            Some(goal) => goal.target(&state, 0.0),
            None => (initial_com + task.goal_offset, task.goal_velocity),
        };
        let frame = read_frame(&state, goal_start, goal_velocity)?;
        let evidence = read_evidence(&state);
        let flight = state
            .free_flight_observation(0)
            .ok_or("missing initial flight")?;
        if evidence.collision
            || evidence.damage
            || evidence.ship_lost
            || !flight.controls_armed
            || !flight.flight.enabled
            || flight.flight.sweep != 0.0
        {
            return Err("initial state is not undamaged open-wing free flight");
        }
        for spec in state
            .world
            .physics
            .surface_preview_geometry(0, &state.world.ships[0])
        {
            let position = frame.origin + spec.local_position.rotate_radians(frame.angle);
            if state.world.physics.world.collider_fits_at(
                spec.id,
                position,
                frame.angle + spec.local_angle,
            ) != Some(true)
            {
                return Err("initial hull or feet overlap geometry");
            }
        }
        let sensor = PolarSensor::new(flight_sensor_config())?;
        let env = Self {
            state,
            sensor,
            goal_start,
            frame_goal,
            frame: frame.clone(),
            progress: FlightProgress::default(),
            previous_action: FlightAction::default(),
            pending_edits: Vec::new(),
            trace: FlightTrace {
                version: task.trace_version(),
                task,
                initial: frame,
                steps: Vec::new(),
            },
        };
        env.observation()?;
        Ok(env)
    }

    /// Transactional reset: a rejected task leaves the current attempt intact.
    pub fn reset(&mut self, task: FlightTask) -> Result<(), &'static str> {
        *self = Self::new(task)?;
        Ok(())
    }

    pub fn progress(&self) -> FlightProgress {
        self.progress
    }
    pub fn frame(&self) -> &FlightFrame {
        &self.frame
    }
    pub fn trace(&self) -> &FlightTrace {
        &self.trace
    }
    /// The live scenario state behind the attempt, for runtime controllers
    /// that read the same world a planner would hand them. Stepping stays
    /// with the environment.
    pub fn state(&self) -> &SurfaceSortieState {
        &self.state
    }

    /// Read-only completed-step sensing. The policy calls this once per action.
    pub fn observation(&self) -> Result<FlightObservation, &'static str> {
        if self.progress.outcome == Some(FlightOutcome::EnvironmentError) {
            return Err("environment failed; last frame is stale");
        }
        let surroundings = self.state.flight_surroundings(0, &self.sensor)?;
        if surroundings.tick != self.frame.world_tick {
            return Err("inconsistent observation tick");
        }
        Ok(FlightObservation {
            version: self.trace.version,
            frame: self.frame.clone(),
            progress: self.progress,
            arrival: self.trace.task.arrival,
            remaining_ticks: self
                .trace
                .task
                .deadline_ticks
                .saturating_sub(self.progress.elapsed_ticks),
            previous_action: self.previous_action,
            control_mode: FlightControlMode::Learned,
            surroundings,
        })
    }

    /// The versioned network inputs, using the same adapter available to a
    /// runtime planner. Diagnostic observations and outcomes are not encoded.
    pub fn policy_observation(
        &self,
    ) -> Result<flight_policy_observation::FlightPolicyObservation, &'static str> {
        if self.progress.outcome == Some(FlightOutcome::EnvironmentError)
            || self.state.world.tick != self.frame.world_tick
        {
            return Err("policy observation unavailable after environment error");
        }
        self.state.flight_policy_observation(
            0,
            &self.sensor,
            flight_policy_observation::FlightPolicyContext {
                target_position: self.frame.target_position,
                target_velocity: self.frame.target_velocity,
                arrival: self.trace.task.arrival,
                elapsed_ticks: self.progress.elapsed_ticks,
                dwell_ticks: self.progress.dwell_ticks,
                deadline_ticks: self.trace.task.deadline_ticks,
                previous_action: self.previous_action,
            },
        )
    }

    /// Diagnostic world mutation through the production queued-edit boundary.
    /// It becomes visible after the next ordinary step and is retained in replay.
    pub fn queue_terrain_edit(&mut self, edit: TerrainEdit) -> Result<(), &'static str> {
        if self.progress.outcome.is_some() {
            return Err("flight already ended");
        }
        if self.pending_edits.len() >= 64 {
            return Err("too many pending diagnostic edits");
        }
        self.state
            .world
            .queue_planet_edit(0, edit)
            .map_err(|_| "invalid terrain edit")?;
        self.pending_edits.push(edit);
        Ok(())
    }

    /// Exactly one ordinary physics tick per valid action. Terminal attempts
    /// cannot advance; invalid actions fail without being clamped/substituted.
    pub fn step(&mut self, action: FlightAction) -> Result<&FlightTraceStep, &'static str> {
        if self.progress.outcome.is_some() {
            return Err("flight already ended");
        }
        let edits = std::mem::take(&mut self.pending_edits);
        if !action.valid() {
            self.progress.outcome = Some(FlightOutcome::ControllerError);
            self.trace.steps.push(FlightTraceStep {
                action: None,
                terrain_edits: edits,
                frame: Some(self.frame.clone()),
                progress: self.progress,
                evidence: FlightEvidence::default(),
                error: Some("invalid flight action".into()),
            });
            return Ok(self.trace.steps.last().unwrap());
        }
        let before = self.frame.errors;
        let tick_before = self.state.world.tick;
        SurfaceSortieScenario::step(
            &mut self.state,
            &[
                SurfaceSortieAction {
                    horizontal: action.steering,
                    primary_held: false,
                    brake_held: false,
                    interact_held: false,
                }
                .encode(PlayerId::PLAYER_1),
                SurfaceFlightAction {
                    thrust: action.thrust,
                    brake: action.brake,
                }
                .encode(PlayerId::PLAYER_1),
                SurfaceWingAction { closed: false }.encode(PlayerId::PLAYER_1),
            ],
            FLIGHT_DT,
        );
        self.previous_action = action;
        let evidence = read_evidence(&self.state);
        let time = (self.progress.elapsed_ticks + 1) as f32 / FLIGHT_HZ as f32;
        // A frame goal rides its planet's orbit and spin; a goal above a still
        // planet moves at its constant velocity.
        let (target, target_velocity) = match self.frame_goal {
            Some(goal) => goal.target(&self.state, time),
            None => (
                self.goal_start + self.trace.task.goal_velocity * time,
                self.trace.task.goal_velocity,
            ),
        };
        let result = if self.state.world.tick == tick_before + 1 {
            read_frame(&self.state, target, target_velocity)
        } else {
            Err("simulation did not advance exactly one tick")
        };
        let (frame, error) = match result {
            Ok(frame) => {
                self.progress
                    .advance(before, frame.errors, evidence, &self.trace.task);
                self.frame = frame.clone();
                (Some(frame), None)
            }
            Err(error) => {
                self.progress.elapsed_ticks +=
                    self.state.world.tick.saturating_sub(tick_before) as u32;
                self.progress.outcome = Some(if evidence.ship_lost {
                    FlightOutcome::ShipLost
                } else {
                    FlightOutcome::EnvironmentError
                });
                (None, Some(error.to_string()))
            }
        };
        self.trace.steps.push(FlightTraceStep {
            action: Some(action),
            terrain_edits: edits,
            frame,
            progress: self.progress,
            evidence,
            error,
        });
        Ok(self.trace.steps.last().unwrap())
    }

    /// Rebuild from the task and verify every recorded frame/outcome/evidence.
    pub fn replay(trace: &FlightTrace) -> Result<Self, &'static str> {
        if trace.version != trace.task.trace_version() {
            return Err("unsupported flight trace version");
        }
        let mut env = Self::new(trace.task.clone())?;
        if env.frame != trace.initial {
            return Err("initial flight replay mismatch");
        }
        for expected in &trace.steps {
            for edit in &expected.terrain_edits {
                env.queue_terrain_edit(*edit)?;
            }
            let action = expected.action.unwrap_or(FlightAction {
                steering: f32::NAN,
                ..Default::default()
            });
            if env.step(action)? != expected {
                return Err("flight step replay mismatch");
            }
        }
        Ok(env)
    }
}

fn read_frame(
    state: &SurfaceSortieState,
    target_position: Vec2,
    target_velocity: Vec2,
) -> Result<FlightFrame, &'static str> {
    state.flight_frame(0, target_position, target_velocity)
}

/// A goal resting in a planet's spinning frame, the frame the brake and the
/// planet-frame motor measure against: its offset from the planet's center
/// and its velocity, both in the frame's axes at the start.
#[derive(Debug, Clone, Copy)]
struct FrameGoal {
    planet: usize,
    start_angle: f32,
    offset: Vec2,
    velocity: Vec2,
}

impl FrameGoal {
    /// The goal's world position and velocity after `time` seconds: the
    /// offset it has reached in the frame, turned by the planet's spin since
    /// the start and carried by the planet's orbit and spin.
    fn target(self, state: &SurfaceSortieState, time: f32) -> (Vec2, Vec2) {
        let frame = motion::SurfaceFrame::read(&state.world.physics, self.planet);
        let turn = frame.angle - self.start_angle;
        let offset = (self.offset + self.velocity * time).rotate_radians(turn);
        let carried = Vec2::new(-offset.y, offset.x) * frame.angular_velocity;
        (
            frame.position + offset,
            frame.linear_velocity + self.velocity.rotate_radians(turn) + carried,
        )
    }
}

/// A generated match arena with our ship placed in open-wing flight above the
/// start planet, as the generated flag trial places its seats. The other seat
/// stays where the arena puts it; the arena's gravity, spin, sun and boundary
/// are the task's conditions.
fn transfer_state(
    start: FlightStart,
    transfer: TransferWorld,
) -> Result<SurfaceSortieState, &'static str> {
    let mut state = SurfaceSortieScenario::init_material_arena_layout(start.seed, transfer.layout);
    if transfer.start_planet >= state.world.planets.len()
        || transfer.goal_planet >= state.world.planets.len()
    {
        return Err("transfer planet outside the generated layout");
    }
    state.pilots[0].planet = transfer.start_planet;
    let planet = state.world.planets[transfer.start_planet];
    let frame = motion::SurfaceFrame::read(&state.world.physics, transfer.start_planet);
    let up = Vec2::Y.rotate_radians(start.bearing);
    let position =
        frame.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + start.altitude + 5.45);
    let velocity = crate::planet_surface_velocity(&planet, position);
    let angle = rotation_for_direction(up) + start.heading_offset;
    let ship = &mut state.world.ships[0];
    ship.position = position - SHIP_PIVOT;
    ship.velocity = velocity;
    ship.rotation_radians = angle;
    ship.direction = Vec2::Y.rotate_radians(angle);
    let body = state.world.physics.ship_body(0);
    if !state
        .world
        .physics
        .world
        .set_pose(body, position, angle, true)
        || !state
            .world
            .physics
            .world
            .set_velocity(body, velocity, frame.angular_velocity, true)
    {
        return Err("cannot place the transfer start");
    }
    state.world.physics.material_queries_dirty = true;
    Ok(state)
}

impl SurfaceSortieState {
    /// The completed-step flight frame of a player's ship against a target:
    /// center-of-mass position and velocity from physics, and the arrival
    /// errors the environment judges. Shared by the headless environment and
    /// runtime controllers, so arrival means one thing in training and play.
    pub fn flight_frame(
        &self,
        player: usize,
        target_position: Vec2,
        target_velocity: Vec2,
    ) -> Result<FlightFrame, &'static str> {
        read_player_frame(self, player, target_position, target_velocity)
    }
}

fn read_player_frame(
    state: &SurfaceSortieState,
    player: usize,
    target_position: Vec2,
    target_velocity: Vec2,
) -> Result<FlightFrame, &'static str> {
    let flight = state
        .free_flight_observation(player)
        .ok_or("flight state unavailable")?;
    let body = state.world.physics.ship_body(flight.vehicle.0);
    let com = state
        .world
        .physics
        .world
        .center_of_mass(body)
        .ok_or("COM unavailable")?;
    let velocity = state
        .world
        .physics
        .world
        .motion(body)
        .ok_or("motion unavailable")?
        .linear_velocity;
    if ![
        com,
        velocity,
        flight.ship.position,
        flight.gravity,
        target_position,
        target_velocity,
    ]
    .into_iter()
    .all(finite)
        || ![flight.ship.angle, flight.ship.spin, flight.health]
            .into_iter()
            .all(f32::is_finite)
    {
        return Err("non-finite physical flight state");
    }
    Ok(FlightFrame {
        world_tick: flight.tick,
        origin: flight.ship.position,
        angle: flight.ship.angle,
        center_of_mass: com,
        velocity,
        spin: flight.ship.spin,
        gravity: flight.gravity,
        health: flight.health,
        target_position,
        target_velocity,
        errors: ArrivalErrors {
            position: com.distance_to(target_position),
            velocity: (velocity - target_velocity).length(),
            spin: flight.ship.spin.abs(),
        },
        potential_landing_assist: flight.landing.assist_strength,
        near_contact: flight.contact,
    })
}

fn read_evidence(state: &SurfaceSortieState) -> FlightEvidence {
    let entity = state.world.physics.ship_body(0).entity;
    let mut evidence = FlightEvidence::default();
    for event in state
        .world
        .physics
        .world
        .contact_events()
        .iter()
        .filter(|e| (e.collider_a.entity == entity) != (e.collider_b.entity == entity))
    {
        evidence.contact_pairs += 1;
        evidence.maximum_impulse = evidence.maximum_impulse.max(event.impulse_magnitude);
        evidence.minimum_separation = evidence
            .minimum_separation
            .into_iter()
            .chain(event.minimum_separation)
            .reduce(f32::min);
        // A solver impulse is physical interaction even when CCD/speculation
        // prevents penetration. Separated, zero-impulse pairs are only near misses.
        evidence.collision |=
            event.impulse_magnitude > 0.0 || event.minimum_separation.is_some_and(|s| s <= 0.0);
    }
    evidence.collision |= (0..3).any(|part| {
        state
            .world
            .physics
            .surface_vehicle_contacts(0, part)
            .any(|c| c.separation <= 0.0)
    });
    let damage = state.damage_observation(0);
    evidence.damage = damage.last_damage_tick == Some(state.world.tick)
        || damage.last_solar_damage_tick == Some(state.world.tick)
        || state
            .world
            .body_impacts
            .iter()
            .any(|hit| hit.ship == 0 && hit.damage > 0.0)
        || state
            .world
            .laser_hits
            .iter()
            .any(|hit| hit.target == LaserTarget::Ship(0) && hit.damage > 0.0);
    let ship = &state.world.ships[0];
    evidence.ship_lost = ship.dead
        || ship.form != ShipForm::Ship
        || !state.vehicle_available(0)
        || state.location(0) != PilotLocation::Aboard(VehicleId(0));
    let radius = state.world.config.universe_radius as f32;
    evidence.boundary_escape = state
        .world
        .physics
        .world
        .center_of_mass(state.world.physics.ship_body(0))
        .is_some_and(|com| com.distance_to(Vec2::splat(radius)) > radius);
    evidence
}
