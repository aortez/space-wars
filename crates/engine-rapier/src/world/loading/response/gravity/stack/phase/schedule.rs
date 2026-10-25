//! Complete bounded stack transaction, using the pair scheduler's exact clock.
use super::*;
use GravityRecord as Record;
use native::schedule::{Limits, apex, arrival, summary, time};

impl Staged {
    fn motion(&self) -> (f64, f64) {
        let a = &self.world.raw.bodies[self.scene.plates.bodies[0]];
        (
            self.scene.frame.x(a.translation().to_array()),
            f64::from(self.scene.frame.speed(a.linvel().to_array())),
        )
    }

    fn record(&self, at: f64, phase: usize, record: Record) -> GravityTimedRecord {
        let loading = self.world.loading.as_ref().unwrap();
        let sources = std::array::from_fn(|i| self.scene.plates.sources[i]);
        GravityTimedRecord {
            at,
            phase,
            roles: GravityRoles::Stack {
                bodies: sources.map(|s| loading.regions[&s.region.id].body),
                sources,
            },
            record,
        }
    }
}

pub(in crate::world::loading::response) fn step(
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
) -> Result<GravityStep, Rejection> {
    with_work_limits(
        world,
        dt,
        policy,
        Limits {
            projections: MAX_PROJECTIONS,
            support_captures: MAX_SUPPORT_CAPTURES,
            ..Limits::default()
        },
    )
}

// Production uses the fixed limits above; reduced limits qualify late rollback.
pub(in crate::world::loading::response) fn with_work_limits(
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
    limits: Limits,
) -> Result<GravityStep, Rejection> {
    let started = Instant::now();
    if !dt.is_finite() || dt < 0.0 {
        return Err(Rejection::InvalidTimestep);
    }
    let mut trace = GravityStep::default();
    if dt == 0.0 {
        world.step(0.0);
        trace.numerical = summary(world);
        trace.wall_time = started.elapsed();
        return Ok(trace);
    }
    let mut staged = Staged::prepare(world, policy, limits.support_captures)?;
    if let Some(event) = staged.bootstrap {
        trace
            .records
            .push(staged.record(0.0, 0, Record::Episode(event)));
    }
    let mut clock = time::Clock::new(dt);
    loop {
        let remaining = clock.remaining()?;
        if remaining == 0.0 {
            break;
        }
        let (x, v) = staged.motion();
        if x >= -TOUCH && v > 0.0 {
            staged = resolve(staged, &mut trace, clock.elapsed(), limits)?;
        }
        if trace.durations.len() >= limits.intervals.min(MAX_GRAVITY_INTERVALS) {
            return Err(Rejection::AdvanceLimit);
        }
        let (x, v) = staged.motion();
        if x >= -TOUCH && v == 0.0 {
            let duration = time::bounded(remaining, remaining)?;
            let at = clock.elapsed();
            clock.advance(duration)?;
            let phase = staged.metrics.len();
            let (next, support, event) = staged.hold(
                duration,
                limits.intervals,
                limits.projections,
                limits.support_captures,
            )?;
            staged = next;
            let support_phase = phase + usize::from(event.is_some());
            if let Some(event) = event {
                trace
                    .records
                    .push(staged.record(at, phase, Record::Episode(event)));
            }
            trace
                .records
                .push(staged.record(at, support_phase, Record::StackSupport(support)));
            trace.durations.push(duration);
            continue;
        }
        let a = staged.scene.acceleration;
        let toi = arrival(x, v, a);
        if !toi.is_finite() || toi <= 0.0 {
            return Err(Rejection::Numerical);
        }
        let mut at_arrival = toi <= remaining;
        let mut duration = time::bounded(toi.min(remaining), remaining)?;
        if v < 0.0 && -v / a < f64::from(duration) {
            let turn = apex(v, a)?;
            if turn < duration {
                duration = turn;
                at_arrival = false;
            }
        }
        let at = clock.elapsed();
        clock.advance(duration)?;
        let phase = staged.metrics.len();
        let (next, flight) = staged.fly(duration, limits.intervals, limits.projections)?;
        staged = next;
        trace.durations.push(duration);
        trace
            .records
            .push(staged.record(at, phase, Record::Support(flight.support)));
        trace.records.push(staged.record(
            clock.elapsed(),
            phase + 1,
            Record::Flight(flight.flight),
        ));
        if let Some(event) = flight.departure {
            trace
                .records
                .push(staged.record(clock.elapsed(), phase + 1, Record::Episode(event)));
        }
        // Rounded touching endpoints must be accounted even when no time remains.
        let (x, v) = staged.motion();
        if at_arrival || (x >= -TOUCH && v > 0.0) {
            staged = resolve(staged, &mut trace, clock.elapsed(), limits)?;
        }
    }
    trace.numerical = summary(&staged.world);
    trace.advances = staged.metrics;
    *world = staged.world;
    trace.wall_time = started.elapsed();
    Ok(trace)
}

fn resolve(
    staged: Staged,
    trace: &mut GravityStep,
    at: f64,
    limits: Limits,
) -> Result<Staged, Rejection> {
    let phase = staged.metrics.len();
    // B-C is always owned. Only A-B's retained owner selects settlement.
    if staged.active()?.incoming.is_some() {
        let (staged, settlement) = staged.settle(limits.projections)?;
        trace
            .records
            .push(staged.record(at, phase, Record::StackSettlement(settlement)));
        return Ok(staged);
    }
    let (staged, arrival) = staged.impact(limits.impacts)?;
    trace
        .records
        .push(staged.record(at, phase, Record::Episode(arrival.impact.event)));
    trace
        .records
        .push(staged.record(at, phase, Record::Impact(arrival.impact)));
    if let Some(correction) = arrival.correction {
        trace
            .records
            .push(staged.record(at, phase, Record::SupportCorrection(correction)));
    }
    Ok(staged)
}

#[cfg(test)]
mod tests;
