//! Independent constant-gravity drift and instantaneous impact experiment.
//! Optional departure evidence and support projection qualify bounded settling.
use super::*;

mod departure;
mod native_flight;
mod native_impact;
mod native_support;
mod numerics;
mod support;
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SupportMode {
    Disabled,
    Rigid,
    Settling,
}

#[derive(Debug, PartialEq)]
enum Rejection {
    InvalidTime,
    Geometry,
    Energy,
    Support,
    Reimpact,
    Evidence,
    Limit,
    Backend(Refusal),
}

#[derive(Debug, PartialEq)]
struct Flight {
    duration: f32,
    gravity_work: f64,
    // Quantization error remains visible; it is never a material charge.
    work_error: f64,
    impulse_error: f64,
    position_error: f64,
    predicted_x: f64,
    uncertainty: f64,
    gap_upper: f64,
}

#[derive(Debug, PartialEq)]
struct Impact {
    at: f64,
    incoming: f32,
    outgoing: f32,
    response: AppliedResponse,
}

#[derive(Debug, PartialEq)]
struct StackSupport {
    at: f64,
    duration: f32,
    episodes: [EpisodeId; 2],
    forces: [f64; 2],
    impulses: [f64; 2],
    rounding_impulses: [f64; 2],
    rounding_loss: f64,
    gravity_work: f64,
    kick_energy: f64,
    projection_energy: f64,
    solves: usize,
}

#[derive(Debug, PartialEq)]
struct StackSettlement {
    at: f64,
    episodes: [EpisodeId; 2],
    available: f64,
    dissipated: f64,
    impulses: [f64; 2],
    rounding_impulses: [f64; 2],
    rounding_loss: f64,
    solves: usize,
}

#[derive(Debug, PartialEq)]
struct SupportCorrection {
    at: f64,
    episode: EpisodeId,
    impulse: f64,
    dissipated: f64,
}

#[derive(Debug, Default, PartialEq)]
struct Trace {
    flights: Vec<Flight>,
    impacts: Vec<Impact>,
    events: Vec<TimedEvent>,
    settlements: Vec<support::Settlement>,
    supports: Vec<support::Support>,
    stack_supports: Vec<StackSupport>,
    stack_settlements: Vec<StackSettlement>,
    support_corrections: Vec<SupportCorrection>,
}

#[derive(Debug, PartialEq)]
struct TimedEvent {
    at: f64,
    event: EpisodeEvent,
}

impl Trace {
    fn impact(
        &mut self,
        world: &mut PhysicsWorld,
        plates: &Plates,
        tracker: &mut Option<departure::Tracker>,
        at: f64,
        acceleration: f64,
        support: SupportMode,
    ) -> Result<(), Rejection> {
        if support == SupportMode::Settling && !world.loading.as_ref().unwrap().episodes.is_empty()
        {
            self.settlements.push(support::settle(
                world,
                plates,
                tracker.as_mut().unwrap(),
                at,
                acceleration,
            )?);
            return Ok(());
        }
        let impact = resolve(world, plates, at)?;
        if let Some(tracker) = tracker {
            tracker.anchor(world, plates, acceleration)?;
        }
        self.events.extend(
            world
                .loading_batches()
                .last()
                .unwrap()
                .events
                .iter()
                .map(|event| TimedEvent { at, event: *event }),
        );
        self.impacts.push(impact);
        Ok(())
    }
}

struct FreeDrift;

impl PhysicsHooks for FreeDrift {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        // fly() proves that the ballistic interval stays outside the fixed
        // plate. hold() instead proves zero velocity after support projection.
        // Neither drift admits additional impulses or correction work.
        context.solver_contacts.clear();
    }
}

fn state(world: &PhysicsWorld) -> (f64, f64, f64) {
    let b = &world.raw.bodies[world.body_handle(body(1)).unwrap()];
    (
        f64::from(b.translation().x),
        f64::from(b.linvel().x),
        f64::from(b.mass()),
    )
}

fn mechanical(world: &PhysicsWorld, acceleration: f64) -> f64 {
    let (x, v, mass) = state(world);
    // Potential is relative to the fixed face at x=0, not world translation.
    0.5 * mass * v * v - mass * acceleration * x
}

fn arrival(gap: f64, velocity: f64, acceleration: f64) -> f64 {
    let speed = libm::sqrt(velocity * velocity + 2.0 * acceleration * gap.max(0.0));
    if velocity >= 0.0 {
        2.0 * gap.max(0.0) / (velocity + speed)
    } else {
        (speed - velocity) / acceleration
    }
}

fn resolve(world: &mut PhysicsWorld, plates: &Plates, at: f64) -> Result<Impact, Rejection> {
    // Only verified departure may release an old episode. Never silently grant
    // a new budget to an already-owned approach.
    if !world.loading.as_ref().unwrap().episodes.is_empty() {
        return Err(Rejection::Reimpact);
    }
    let start = plates.positions(world);
    let (_, incoming, mass) = state(world);
    let count = world.material_response_batches().len();
    let (velocity, _) = advance(world, plates, 0.0, false, true).map_err(Rejection::Backend)?;
    if start != plates.positions(world)
        || world.material_response_batches().len() != count + 1
        || velocity[0].x > 0.0
    {
        return Err(Rejection::Energy);
    }
    let response = world.material_response_batches()[count]
        .result
        .as_ref()
        .unwrap()
        .clone();
    let outgoing = velocity[0].x;
    let expected = loss(&response);
    let actual = 0.5 * mass * (incoming - f64::from(outgoing)) * (incoming + f64::from(outgoing));
    if (actual - expected).abs() > 1.0e-5 * 0.5 * mass * incoming * incoming {
        return Err(Rejection::Energy);
    }
    Ok(Impact {
        at,
        incoming: incoming as f32,
        outgoing,
        response,
    })
}

fn fly(
    world: &mut PhysicsWorld,
    plates: &Plates,
    dt: f32,
    acceleration: f64,
    ledger: Option<&mut numerics::Ledger>,
) -> Result<Flight, Rejection> {
    if let Some(ledger) = &ledger {
        ledger.validate(world, acceleration)?;
    }
    let (x, velocity, mass) = state(world);
    let h = f64::from(dt);
    let predicted = x + velocity * h + 0.5 * acceleration * h * h;
    // Acceleration is positive: x(t) is convex, so its maximum is at an
    // endpoint. This proves no hidden crossing, including an intervening apex.
    if !predicted.is_finite() || x > TOUCH || predicted > TOUCH {
        return Err(Rejection::Geometry);
    }
    let before = mechanical(world, acceleration);
    let handle = plates.handles[0];
    let middle = (velocity + 0.5 * acceleration * h) as f32;
    // Restore the first kick's rounding residual before the second kick.
    // Drift uses the actual rounded midpoint; final momentum rounds only once.
    let outgoing = (velocity + acceleration * h) as f32;
    if !middle.is_finite()
        || !outgoing.is_finite()
        || (ledger.is_none() && f64::from(middle) == velocity && f64::from(outgoing) == velocity)
    {
        return Err(Rejection::Energy);
    }
    world.raw.bodies[handle].set_linvel(Vector::X * middle, true);
    let positions = plates.positions(world);
    let mut speed = vec![Vector::ZERO; plates.handles.len()];
    speed[0] = Vector::X * middle;
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.raw.integration_parameters.dt = dt;
    world.raw.step_with_events(&FreeDrift, &());
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    if world.raw.bodies[handle].linvel() != speed[0]
        || plates.handles[1..]
            .iter()
            .any(|h| world.raw.bodies[*h].linvel() != Vector::ZERO)
    {
        return Err(Rejection::Energy);
    }
    plates
        .check_drift(world, &positions, &speed, dt)
        .map_err(Rejection::Backend)?;
    world.raw.bodies[handle].set_linvel(Vector::X * outgoing, true);
    let (end, actual, end_mass) = state(world);
    if end > TOUCH
        || end_mass != mass
        || plates.handles[1..]
            .iter()
            .enumerate()
            .any(|(i, h)| world.raw.bodies[*h].translation() != positions[i + 1])
    {
        return Err(Rejection::Geometry);
    }
    let after = mechanical(world, acceleration);
    let gravity_work = mass * acceleration * (end - x);
    let kinetic_change = 0.5 * mass * (actual - velocity) * (actual + velocity);
    let work_error = gravity_work - kinetic_change;
    let impulse_error = mass * (acceleration * h - (actual - velocity));
    let position_error = (end - x) - (velocity * h + 0.5 * acceleration * h * h);
    if !after.is_finite()
        || !work_error.is_finite()
        || work_error.abs() > 1.0e-5 * before.max(after)
    {
        return Err(Rejection::Energy);
    }
    let drift_uncertainty = numerics::drift_uncertainty(world, x, middle, dt)?;
    if (end - x - f64::from(middle) * h).abs() > drift_uncertainty {
        return Err(Rejection::Geometry);
    }
    let uncertainty =
        drift_uncertainty + (f64::from(middle) - velocity - 0.5 * acceleration * h).abs() * h;
    if let Some(ledger) = ledger {
        ledger.charge(world, work_error, impulse_error, position_error)?;
    }
    let minimum = if velocity < 0.0 && velocity + acceleration * h >= 0.0 {
        x - velocity * velocity / (2.0 * acceleration)
    } else {
        x.min(predicted)
    };
    Ok(Flight {
        duration: dt,
        gravity_work,
        work_error,
        impulse_error,
        position_error,
        predicted_x: predicted,
        // Scale/subdivision-aware drift bound plus measured half-kick rounding.
        uncertainty,
        gap_upper: (-minimum).max(-end).max(-x) + uncertainty,
    })
}

fn step(world: &mut PhysicsWorld, dt: f32, limit: usize) -> Result<Trace, Rejection> {
    step_tracked(world, dt, limit, None)
}

fn step_tracked(
    world: &mut PhysicsWorld,
    dt: f32,
    limit: usize,
    tracker: Option<&mut departure::Tracker>,
) -> Result<Trace, Rejection> {
    step_mode(world, dt, limit, tracker, SupportMode::Disabled)
}

fn step_mode(
    world: &mut PhysicsWorld,
    dt: f32,
    limit: usize,
    tracker: Option<&mut departure::Tracker>,
    support: SupportMode,
) -> Result<Trace, Rejection> {
    if !dt.is_finite() || dt < 0.0 {
        return Err(Rejection::InvalidTime);
    }
    if dt == 0.0 {
        world.step(0.0);
        return Ok(Trace::default());
    }
    if support != SupportMode::Disabled && tracker.is_none() {
        return Err(Rejection::Evidence);
    }
    let plates = Plates::new(world).map_err(Rejection::Backend)?;
    let acceleration = f64::from(world.raw.gravity.x);
    if plates.handles.len() != 2
        || plates.normal != [1.0, 0.0]
        || !acceleration.is_finite()
        || acceleration <= 0.0
        || world.raw.gravity.y != 0.0
        || !world.raw.bodies[plates.handles[0]].is_dynamic()
        || !world.raw.bodies[plates.handles[1]].is_fixed()
        || world.raw.bodies[plates.handles[1]].translation() != Vector::X
        || !world.raw.impulse_joints.is_empty()
        || world.raw.multibody_joints.iter().next().is_some()
        || world.raw.bodies.iter().any(|(_, b)| {
            b.user_force() != Vector::ZERO
                || b.user_torque() != 0.0
                || b.linear_damping() != 0.0
                || b.angular_damping() != 0.0
                || b.is_ccd_enabled()
                || b.gravity_scale() != 1.0
                || b.locked_axes().intersects(LockedAxes::TRANSLATION_LOCKED)
                || (b.is_dynamic() && (!b.mass().is_finite() || b.mass() <= 0.0))
                || b.linvel().y != 0.0
                || b.translation().y != 0.0
        })
        || world.raw.colliders.iter().any(|(_, c)| c.friction() != 0.0)
    {
        return Err(Rejection::Geometry);
    }
    let mut staged = world.clone();
    let mut staged_tracker = tracker.as_deref().cloned();
    if let Some(tracker) = &mut staged_tracker {
        tracker.start(&staged, &plates, acceleration)?;
    }
    staged.raw.gravity = Vector::ZERO;
    staged.loading.as_mut().unwrap().begin_step();
    let mut trace = Trace::default();
    let mut elapsed = 0.0;
    loop {
        let (x, velocity, _) = state(&staged);
        if !x.is_finite() || !velocity.is_finite() || x > TOUCH {
            return Err(Rejection::Geometry);
        }
        if elapsed >= f64::from(dt) {
            break;
        }
        if -x <= TOUCH && velocity >= 0.0 {
            if velocity == 0.0 {
                if support != SupportMode::Disabled && elapsed < f64::from(dt) {
                    if trace.flights.len() + trace.supports.len() >= limit.min(MAX_ADVANCES) {
                        return Err(Rejection::Limit);
                    }
                    let duration = (f64::from(dt) - elapsed) as f32;
                    let (support, event) = support::hold(
                        &mut staged,
                        &plates,
                        staged_tracker.as_mut().unwrap(),
                        elapsed,
                        duration,
                        acceleration,
                    )?;
                    trace.supports.push(support);
                    trace.events.extend(event);
                    break;
                }
                return Err(Rejection::Support);
            }
            trace.impact(
                &mut staged,
                &plates,
                &mut staged_tracker,
                elapsed,
                acceleration,
                support,
            )?;
            if state(&staged).1 == 0.0 {
                continue;
            }
        }
        if elapsed >= f64::from(dt) {
            break;
        }
        if trace.flights.len() >= limit.min(MAX_ADVANCES) {
            return Err(Rejection::Limit);
        }
        let (x, velocity, _) = state(&staged);
        if -x <= TOUCH && velocity == 0.0 {
            return Err(Rejection::Support);
        }
        let remaining = (f64::from(dt) - elapsed) as f32;
        let toi = arrival(-x, velocity, acceleration);
        let mut at_arrival = toi <= f64::from(remaining);
        let mut duration = toi.min(f64::from(remaining)) as f32;
        if staged_tracker.is_some() && velocity < 0.0 {
            let apex = departure::apex_duration(velocity, acceleration)?;
            if apex < duration {
                duration = apex;
                at_arrival = false;
            }
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::Geometry);
        }
        if let Some(tracker) = &staged_tracker {
            tracker.validate(&staged, &plates, acceleration)?;
        }
        let flight = fly(
            &mut staged,
            &plates,
            duration,
            acceleration,
            staged_tracker.as_mut().and_then(|t| t.rounding.as_mut()),
        )?;
        elapsed += f64::from(duration);
        if let Some(tracker) = &mut staged_tracker
            && let Some(event) =
                tracker.after_flight(&mut staged, &plates, acceleration, &flight)?
        {
            trace.events.push(TimedEvent { at: elapsed, event });
        }
        trace.flights.push(flight);
        // A rounded endpoint can already be touching even when its analytic
        // root lies beyond this frame. Do not defer that observed approach to
        // the next call; the same contact rule applies at either boundary.
        let (end_x, end_velocity, _) = state(&staged);
        if at_arrival || (-end_x <= TOUCH && end_velocity > 0.0) {
            if -end_x > TOUCH {
                return Err(Rejection::Geometry);
            }
            trace.impact(
                &mut staged,
                &plates,
                &mut staged_tracker,
                elapsed,
                acceleration,
                support,
            )?;
        }
        if duration == remaining {
            break;
        }
    }
    staged.raw.gravity = world.raw.gravity;
    *world = staged;
    if let (Some(tracker), Some(next)) = (tracker, staged_tracker) {
        *tracker = next;
    }
    Ok(trace)
}
