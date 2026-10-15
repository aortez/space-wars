//! Fixed material-ship arrival cases for integration and calibration. The world
//! is only placed at initialization; all subsequent motion uses normal actions.

use crate::evolution::{EvaluationTask, EvolutionError, TaskManifest};
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieScenario, SurfaceSortieState,
        flight::FlightControlLimits, free_flight::FreeFlightObservation,
        pilot::MaterialFlightStart,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    guidance::{
        self, Binding, Controller, EncodedFeatures, Features, Movement, Policy, Request, Target,
    },
    model::{
        Activation, DenseGenome, DenseLayer, DenseModel, MODEL_VERSION, ModelContract, ModelMemory,
    },
};
use std::time::Duration;

pub const RECIPE: &str = "material_arrival_fixed_v1";
pub const DT: Duration = Duration::from_nanos(16_666_667);
pub const POSITION_TOLERANCE: f32 = 2.0;
pub const SPEED_TOLERANCE: f32 = 1.0;
pub const DWELL_TICKS: u32 = 30;
const MIN_ALTITUDE: f32 = 40.0;
const MAX_ALTITUDE: f32 = 220.0;
const MAX_TICKS: u32 = 1200;
const SETUP_TICKS: u32 = 30;

fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrivalCase {
    pub name: String,
    pub seed: u64,
    pub start: MaterialFlightStart,
    /// In the starting radial frame: +x tangential right, +y away from planet.
    pub target_offset: Vec2,
    /// Constant world velocity, specified in that same initial frame.
    pub target_velocity: Vec2,
    pub max_ticks: u32,
}
impl ArrivalCase {
    fn validate(&self) -> Result<(), EvolutionError> {
        let s = self.start;
        let values = [
            s.bearing,
            s.altitude,
            s.radial_speed,
            s.lateral_speed,
            s.heading_offset,
            self.target_offset.x,
            self.target_offset.y,
            self.target_velocity.x,
            self.target_velocity.y,
        ];
        if values.iter().any(|v| !v.is_finite())
            || !(15.0..=150.0).contains(&s.altitude)
            || s.bearing.abs() > std::f32::consts::TAU
            || s.heading_offset.abs() > std::f32::consts::PI
            || s.radial_speed.abs() > 30.0
            || s.lateral_speed.abs() > 30.0
            || self.target_offset.length() > 80.0
            || self.target_velocity.length() > 5.0
            || !(DWELL_TICKS..=MAX_TICKS).contains(&self.max_ticks)
        {
            return Err(error("invalid arrival fixture specification"));
        }
        Ok(())
    }
}

pub fn cases() -> Vec<ArrivalCase> {
    let base = ArrivalCase {
        name: "stationary_radial".into(),
        seed: 42,
        start: MaterialFlightStart {
            bearing: 0.0,
            altitude: 90.0,
            radial_speed: 0.0,
            lateral_speed: 0.0,
            heading_offset: 0.0,
        },
        target_offset: Vec2::new(0.0, 20.0),
        target_velocity: Vec2::ZERO,
        max_ticks: MAX_TICKS,
    };
    [
        (
            "stationary_radial",
            0.0,
            0.0,
            Vec2::new(0.0, 20.0),
            Vec2::ZERO,
        ),
        (
            "stationary_right",
            0.0,
            0.4,
            Vec2::new(20.0, 5.0),
            Vec2::ZERO,
        ),
        (
            "stationary_left",
            0.0,
            -0.4,
            Vec2::new(-20.0, 5.0),
            Vec2::ZERO,
        ),
        (
            "stationary_rotated",
            std::f32::consts::FRAC_PI_2,
            0.0,
            Vec2::new(20.0, 5.0),
            Vec2::ZERO,
        ),
        (
            "stationary_reversed",
            std::f32::consts::PI,
            std::f32::consts::PI,
            Vec2::new(0.0, 20.0),
            Vec2::ZERO,
        ),
        (
            "moving_right",
            0.0,
            0.4,
            Vec2::new(20.0, 5.0),
            Vec2::new(2.0, 0.0),
        ),
        (
            "moving_left",
            0.0,
            -0.4,
            Vec2::new(-20.0, 5.0),
            Vec2::new(-2.0, 0.0),
        ),
        (
            "moving_rotated",
            std::f32::consts::FRAC_PI_2,
            0.0,
            Vec2::new(20.0, 5.0),
            Vec2::new(2.0, 0.0),
        ),
    ]
    .into_iter()
    .map(
        |(name, bearing, heading, target_offset, target_velocity)| ArrivalCase {
            name: name.into(),
            start: MaterialFlightStart {
                bearing,
                heading_offset: heading,
                ..base.start
            },
            target_offset,
            target_velocity,
            ..base.clone()
        },
    )
    .collect()
}

pub fn neutral_genome() -> DenseGenome {
    DenseGenome {
        version: MODEL_VERSION,
        contract: guidance::contract(),
        memory: ModelMemory::None,
        layers: vec![DenseLayer {
            outputs: 3,
            activation: Activation::Linear,
            weights: vec![0.0; 27],
            biases: vec![0.0; 3],
        }],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalOutcome {
    Arrived,
    Timeout,
    InvalidOutput,
    LostControl,
    Assisted,
    Contact,
    Damaged,
    OutsideRegion,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrivalFitness {
    pub completed: u32,
    pub position_points: u32,
    pub velocity_points: u32,
    pub spare_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Measure {
    pub distance: f32,
    pub relative_speed: f32,
    pub dwell: u32,
}
impl Measure {
    fn read(o: &FreeFlightObservation, target: Target, previous_dwell: u32) -> Self {
        let distance = (target.position - o.ship.position).length();
        let relative_speed = (target.velocity - o.ship.velocity).length();
        Self {
            distance,
            relative_speed,
            dwell: if distance <= POSITION_TOLERANCE && relative_speed <= SPEED_TOLERANCE {
                previous_dwell + 1
            } else {
                0
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraceTick {
    pub before: FreeFlightObservation,
    pub target_before: Target,
    pub request: Request,
    pub encoded: EncodedFeatures,
    pub raw_outputs: Option<Vec<f32>>,
    pub movement: Option<Movement>,
    pub after: Option<FreeFlightObservation>,
    pub measure: Measure,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeBrain {
    pub version: u32,
    pub target_source: String,
    pub coordinator: String,
    pub contract: ModelContract,
    pub controller: String,
}

impl PracticeBrain {
    pub fn guidance(controller: &str) -> Self {
        Self {
            version: 1,
            target_source: "constant_world_velocity_v1".into(),
            coordinator: "arrival_single_owner_v1".into(),
            contract: guidance::contract(),
            controller: controller.into(),
        }
    }

    pub fn identity(&self) -> Result<String, EvolutionError> {
        let mut identity = Sha256::new();
        identity.update(b"spacewars.practice_brain.v1\0");
        identity.update(serde_json::to_vec(self).map_err(error)?);
        Ok(format!("{:x}", identity.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalEpisode {
    pub version: u32,
    pub runtime: String,
    pub recipe: String,
    pub case: ArrivalCase,
    pub brain: PracticeBrain,
    pub brain_id: String,
    pub setup_ticks: u32,
    pub initial: FreeFlightObservation,
    pub initial_target: Target,
    pub final_observation: Option<FreeFlightObservation>,
    pub elapsed_ticks: u32,
    pub outcome: ArrivalOutcome,
    /// Last available measurement; a missing final body is always LostControl.
    pub measure: Measure,
    pub fitness: ArrivalFitness,
    pub saturated_ticks: [u32; 9],
    /// One neutral cleanup tick after the scored result; never contributes score.
    pub released: Option<FreeFlightObservation>,
    pub trace: Vec<TraceTick>,
}

fn sample(initial: Target, ticks: u32) -> Target {
    Target {
        position: initial.position + initial.velocity * (ticks as f32 * DT.as_secs_f32()),
        ..initial
    }
}

fn observation(state: &SurfaceSortieState) -> Result<FreeFlightObservation, EvolutionError> {
    state
        .free_flight_observation(0)
        .ok_or_else(|| error("assigned arrival ship is missing"))
}

fn failure(o: &FreeFlightObservation, initial: &FreeFlightObservation) -> Option<ArrivalOutcome> {
    if o.version != 1
        || !o.alive
        || !o.available
        || o.form != ShipForm::Ship
        || o.owner != initial.owner
        || o.spaceling != initial.spaceling
        || o.vehicle != initial.vehicle
        || o.location != PilotLocation::Aboard(initial.vehicle)
        || !o.controls_armed
        || o.planet != initial.planet
        || !o.flight.enabled
        || o.flight.wings_closed
        || o.flight.sweep != 0.0
        || o.flight.limits != FlightControlLimits::for_sweep(0.0)
    {
        return Some(ArrivalOutcome::LostControl);
    }
    if o.contact || o.landing.supported_feet > 0 {
        return Some(ArrivalOutcome::Contact);
    }
    // Reject the assistance envelope even while thrust masks the public phase.
    if o.landing.assist_strength > 0.0 || o.landing.phase != LandingPhase::Flying {
        return Some(ArrivalOutcome::Assisted);
    }
    if o.health < initial.health {
        return Some(ArrivalOutcome::Damaged);
    }
    let altitude = o.ship.position.distance_to(o.planet_motion.position) - o.planet_radius;
    if !altitude.is_finite() || !(MIN_ALTITUDE..=MAX_ALTITUDE).contains(&altitude) {
        return Some(ArrivalOutcome::OutsideRegion);
    }
    None
}

fn setup(
    case: &ArrivalCase,
) -> Result<(SurfaceSortieState, FreeFlightObservation, Target, u32), EvolutionError> {
    case.validate()?;
    let mut world = SurfaceSortieScenario::init_material_flight(
        case.seed,
        1,
        &[(PlayerId::PLAYER_1, case.start)],
    );
    let placed = observation(&world)?;
    let initial_target = Target {
        id: 1,
        position: placed.ship.position + case.target_offset.rotate_radians(case.start.bearing),
        velocity: case.target_velocity.rotate_radians(case.start.bearing),
    };
    // Check the complete prescribed marker path before the episode. A line
    // segment's closest point is not necessarily either endpoint.
    let end = sample(initial_target, case.max_ticks).position;
    let center = placed.planet_motion.position;
    let nearest = Vec2::closest_point_on_segment(center, initial_target.position, end);
    if nearest.distance_to(center) - placed.planet_radius < MIN_ALTITUDE + POSITION_TOLERANCE
        || [initial_target.position, end].iter().any(|p| {
            p.distance_to(center) - placed.planet_radius > MAX_ALTITUDE - POSITION_TOLERANCE
        })
    {
        return Err(error(
            "arrival target path leaves the unassisted fixture region",
        ));
    }
    for tick in 1..=SETUP_TICKS {
        SurfaceSortieScenario::step(
            &mut world,
            &Movement::default().encode(PlayerId::PLAYER_1),
            DT,
        );
        let observed = observation(&world)?;
        if observed.controls_armed {
            if let Some(reason) = failure(&observed, &placed) {
                return Err(error(format!("invalid arrival starting state: {reason:?}")));
            }
            return Ok((world, observed, initial_target, tick));
        }
    }
    Err(error(
        "arrival controls failed to arm within setup deadline",
    ))
}

fn score(
    outcome: ArrivalOutcome,
    measure: Measure,
    elapsed: u32,
    max_ticks: u32,
) -> ArrivalFitness {
    if !matches!(outcome, ArrivalOutcome::Arrived | ArrivalOutcome::Timeout) {
        return ArrivalFitness::default();
    }
    ArrivalFitness {
        completed: u32::from(outcome == ArrivalOutcome::Arrived),
        position_points: ((1.0 - measure.distance / guidance::DISTANCE_SCALE).clamp(0.0, 1.0)
            * 1000.0) as u32,
        velocity_points: ((1.0 - measure.relative_speed / guidance::SPEED_SCALE).clamp(0.0, 1.0)
            * 1000.0) as u32,
        spare_ticks: if outcome == ArrivalOutcome::Arrived {
            max_ticks - elapsed
        } else {
            0
        },
    }
}

pub fn run_episode(
    case: ArrivalCase,
    policy: Policy,
    retain_trace: bool,
) -> Result<ArrivalEpisode, EvolutionError> {
    // Reject an incompatible model before constructing or advancing the world.
    if let Policy::Model(model) = &policy {
        model
            .require_contract(&guidance::contract())
            .map_err(error)?;
    }
    let brain = PracticeBrain::guidance(policy.id());
    let brain_id = brain.identity()?;
    let (mut world, initial, initial_target, setup_ticks) = setup(&case)?;
    let binding = Binding {
        owner: initial.owner,
        spaceling: initial.spaceling,
        vehicle: initial.vehicle,
        planet: initial.planet,
        epoch: 1,
    };
    let mut controller = Controller::new(policy, binding).map_err(error)?;
    let mut current = initial.clone();
    let mut measure = Measure::read(&current, initial_target, 0);
    measure.dwell = 0; // Only scored post-step states count toward arrival.
    let mut elapsed_ticks = 0;
    let mut trace = Vec::new();
    let mut saturated_ticks = [0; 9];
    let mut body_present = true;
    let outcome = loop {
        let target = sample(initial_target, elapsed_ticks);
        let features = Features::read(&current, target);
        // Broken world observations are host errors, not measured bad genomes.
        let encoded = features.encode().map_err(error)?;
        for (count, clipped) in saturated_ticks.iter_mut().zip(encoded.saturated) {
            *count += u32::from(clipped);
        }
        let request = Request {
            binding,
            tick: current.tick,
            target_id: target.id,
            features,
        };
        let decision = controller.decide(request);
        let decision = match decision {
            Ok(decision) => decision,
            Err(e) => {
                if retain_trace {
                    trace.push(TraceTick {
                        before: current.clone(),
                        target_before: target,
                        request,
                        encoded,
                        raw_outputs: None,
                        movement: None,
                        after: Some(current.clone()),
                        measure,
                        error: Some(e.to_string()),
                    });
                }
                break ArrivalOutcome::InvalidOutput;
            }
        };
        SurfaceSortieScenario::step(&mut world, &decision.movement.encode(binding.owner), DT);
        elapsed_ticks += 1;
        let Some(next) = world.free_flight_observation(0) else {
            body_present = false;
            if retain_trace {
                trace.push(TraceTick {
                    before: current.clone(),
                    target_before: target,
                    request,
                    encoded,
                    raw_outputs: decision.raw_outputs,
                    movement: Some(decision.movement),
                    after: None,
                    measure,
                    error: Some("assigned ship disappeared".into()),
                });
            }
            break ArrivalOutcome::LostControl;
        };
        if next.tick != current.tick + 1 {
            return Err(error("arrival world did not advance exactly one tick"));
        }
        measure = Measure::read(&next, sample(initial_target, elapsed_ticks), measure.dwell);
        if !measure.distance.is_finite() || !measure.relative_speed.is_finite() {
            return Err(error("non-finite arrival world state"));
        }
        if retain_trace {
            trace.push(TraceTick {
                before: current,
                target_before: target,
                request,
                encoded,
                raw_outputs: decision.raw_outputs,
                movement: Some(decision.movement),
                after: Some(next.clone()),
                measure,
                error: None,
            });
        }
        current = next;
        if let Some(reason) = failure(&current, &initial) {
            break reason;
        }
        if measure.dwell >= DWELL_TICKS {
            break ArrivalOutcome::Arrived;
        }
        if elapsed_ticks >= case.max_ticks {
            break ArrivalOutcome::Timeout;
        }
    };
    let fitness = score(outcome, measure, elapsed_ticks, case.max_ticks);
    SurfaceSortieScenario::step(&mut world, &Movement::default().encode(binding.owner), DT);
    let released = world.free_flight_observation(0);
    Ok(ArrivalEpisode {
        version: 1,
        runtime: crate::evolution::runtime_id()?,
        recipe: RECIPE.into(),
        case,
        brain,
        brain_id,
        setup_ticks,
        initial,
        initial_target,
        final_observation: body_present.then_some(current),
        elapsed_ticks,
        outcome,
        measure,
        fitness,
        saturated_ticks,
        released,
        trace,
    })
}

#[derive(Debug, Clone, Default)]
pub struct ArrivalTask;
impl EvaluationTask for ArrivalTask {
    type Fitness = ArrivalFitness;
    fn manifest(&self) -> TaskManifest {
        TaskManifest {
            id: RECIPE.into(),
            contract: guidance::contract(),
            configuration: serde_json::json!({
            "world": "material_stationary_blocks_v1", "build": "material_ship_open_v1", "limits": FlightControlLimits::for_sweep(0.0),
            "dt_ns": DT.as_nanos(), "cases": cases(), "position_tolerance": POSITION_TOLERANCE,
            "speed_tolerance": SPEED_TOLERANCE, "dwell_ticks": DWELL_TICKS,
            "radial_altitude": [MIN_ALTITUDE, MAX_ALTITUDE], "setup_deadline": SETUP_TICKS,
            "comparison": "completed_position_velocity_spare_v1" }),
        }
    }
    fn evaluate(&self, model: &DenseModel) -> Result<ArrivalFitness, EvolutionError> {
        let mut total = ArrivalFitness::default();
        for case in cases() {
            let score = run_episode(case, Policy::Model(model.clone()), false)?.fitness;
            total.completed += score.completed;
            total.position_points += score.position_points;
            total.velocity_points += score.velocity_points;
            total.spare_ticks += score.spare_ticks;
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests;
