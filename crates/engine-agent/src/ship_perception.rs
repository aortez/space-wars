//! `ship_arrival::run_episode` for every guidance policy, including those that
//! read the perception blocks. `ship_arrival`'s private helpers are copied
//! here so that pinned experiment sources stay unchanged. For the policies
//! `ship_arrival` accepts, this reproduces its episodes exactly.

use crate::{
    evolution::EvolutionError,
    ship_arrival::{
        ArrivalCase, ArrivalEpisode, ArrivalFitness, ArrivalOutcome, DT, DWELL_TICKS, Measure,
        POSITION_TOLERANCE, PracticeBrain, RECIPE, SPEED_TOLERANCE, TraceTick,
    },
};
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieScenario, SurfaceSortieState,
        flight::FlightControlLimits, free_flight::FreeFlightObservation,
    },
};
use serde::Serialize;
use spacewars_ai::guidance::{
    self, Binding, Controller, Features, Movement, Policy, Request, Target,
    perception::{self, EncodedPerception, Perception},
};

// Copied from `ship_arrival`.
const MIN_ALTITUDE: f32 = 40.0;
const MAX_ALTITUDE: f32 = 220.0;
const MAX_TICKS: u32 = 1200;
const SETUP_TICKS: u32 = 30;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PerceivingEpisode {
    pub episode: ArrivalEpisode,
    /// Each decision's encoded perception, when traces are retained and the
    /// policy reads it.
    pub perception: Vec<EncodedPerception>,
}

fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}

/// The episode's brain record; a perception policy names its own contract.
pub fn brain(policy: &Policy) -> PracticeBrain {
    let brain = PracticeBrain::guidance(policy.id());
    if policy.perceives() {
        PracticeBrain {
            contract: perception::contract(),
            ..brain
        }
    } else {
        brain
    }
}

// Copied from `ship_arrival::ArrivalCase::validate`.
fn validate(case: &ArrivalCase) -> Result<(), EvolutionError> {
    let s = case.start;
    let values = [
        s.bearing,
        s.altitude,
        s.radial_speed,
        s.lateral_speed,
        s.heading_offset,
        case.target_offset.x,
        case.target_offset.y,
        case.target_velocity.x,
        case.target_velocity.y,
    ];
    if values.iter().any(|v| !v.is_finite())
        || !(15.0..=150.0).contains(&s.altitude)
        || s.bearing.abs() > std::f32::consts::TAU
        || s.heading_offset.abs() > std::f32::consts::PI
        || s.radial_speed.abs() > 30.0
        || s.lateral_speed.abs() > 30.0
        || case.target_offset.length() > 80.0
        || case.target_velocity.length() > 5.0
        || !(DWELL_TICKS..=MAX_TICKS).contains(&case.max_ticks)
    {
        return Err(error("invalid arrival fixture specification"));
    }
    Ok(())
}

// Copied from `ship_arrival::Measure::read`.
fn measure(o: &FreeFlightObservation, target: Target, previous_dwell: u32) -> Measure {
    let distance = (target.position - o.ship.position).length();
    let relative_speed = (target.velocity - o.ship.velocity).length();
    Measure {
        distance,
        relative_speed,
        dwell: if distance <= POSITION_TOLERANCE && relative_speed <= SPEED_TOLERANCE {
            previous_dwell + 1
        } else {
            0
        },
    }
}

// Copied from `ship_arrival`.
fn sample(initial: Target, ticks: u32) -> Target {
    Target {
        position: initial.position + initial.velocity * (ticks as f32 * DT.as_secs_f32()),
        ..initial
    }
}

// Copied from `ship_arrival`.
fn observation(state: &SurfaceSortieState) -> Result<FreeFlightObservation, EvolutionError> {
    state
        .free_flight_observation(0)
        .ok_or_else(|| error("assigned arrival ship is missing"))
}

// Copied from `ship_arrival`.
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

// Copied from `ship_arrival`.
fn setup(
    case: &ArrivalCase,
) -> Result<(SurfaceSortieState, FreeFlightObservation, Target, u32), EvolutionError> {
    validate(case)?;
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

// Copied from `ship_arrival`.
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

/// As `ship_arrival::run_episode`. A perception policy reads the practice
/// world's bodies each tick, with the planet as its destination.
pub fn run_episode(
    case: ArrivalCase,
    policy: Policy,
    retain_trace: bool,
) -> Result<PerceivingEpisode, EvolutionError> {
    // Reject an incompatible model before constructing or advancing the world.
    match &policy {
        Policy::Model(model) => model.require_contract(&guidance::contract()),
        Policy::Perception(model) => model.require_contract(&perception::contract()),
        Policy::Reference | Policy::Idle => Ok(()),
    }
    .map_err(error)?;
    let perceives = policy.perceives();
    let brain = brain(&policy);
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
    let mut measure_now = measure(&current, initial_target, 0);
    measure_now.dwell = 0; // Only scored post-step states count toward arrival.
    let mut elapsed_ticks = 0;
    let mut trace = Vec::new();
    let mut perceived = Vec::new();
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
        let decision = if perceives {
            let sensed =
                Perception::read(&current, &world.celestial_bodies(), Some(binding.planet));
            controller.decide_perceiving(request, &sensed)
        } else {
            controller.decide(request)
        };
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
                        measure: measure_now,
                        error: Some(e.to_string()),
                    });
                }
                break ArrivalOutcome::InvalidOutput;
            }
        };
        if retain_trace && let Some(sensed) = &decision.perception {
            perceived.push(sensed.clone());
        }
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
                    measure: measure_now,
                    error: Some("assigned ship disappeared".into()),
                });
            }
            break ArrivalOutcome::LostControl;
        };
        if next.tick != current.tick + 1 {
            return Err(error("arrival world did not advance exactly one tick"));
        }
        measure_now = measure(
            &next,
            sample(initial_target, elapsed_ticks),
            measure_now.dwell,
        );
        if !measure_now.distance.is_finite() || !measure_now.relative_speed.is_finite() {
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
                measure: measure_now,
                error: None,
            });
        }
        current = next;
        if let Some(reason) = failure(&current, &initial) {
            break reason;
        }
        if measure_now.dwell >= DWELL_TICKS {
            break ArrivalOutcome::Arrived;
        }
        if elapsed_ticks >= case.max_ticks {
            break ArrivalOutcome::Timeout;
        }
    };
    let fitness = score(outcome, measure_now, elapsed_ticks, case.max_ticks);
    SurfaceSortieScenario::step(&mut world, &Movement::default().encode(binding.owner), DT);
    let released = world.free_flight_observation(0);
    Ok(PerceivingEpisode {
        episode: ArrivalEpisode {
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
            measure: measure_now,
            fitness,
            saturated_ticks,
            released,
            trace,
        },
        perception: perceived,
    })
}

#[cfg(test)]
mod tests;
