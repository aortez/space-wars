//! Bounded private flight, impact, support and optional settlement scheduler.
use super::*;

mod time;
use time::Clock;

const MAX_INTERVALS: usize = MAX_GRAVITY_INTERVALS;
const MAX_IMPACTS: usize = MAX_GRAVITY_IMPACTS;

#[derive(Clone, Copy)]
pub(in crate::world::loading::response) struct Limits {
    pub intervals: usize,
    pub impacts: usize,
    pub projections: usize,
    pub support_captures: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            intervals: MAX_INTERVALS,
            impacts: MAX_IMPACTS,
            projections: MAX_GRAVITY_PROJECTIONS,
            support_captures: MAX_GRAVITY_SUPPORT_CAPTURES,
        }
    }
}

pub(in crate::world::loading::response) use super::super::{
    GravityNumericalSummary as NumericalSummary, GravityRecord as Record, GravityStep as Trace,
    GravityTimedRecord as TimedRecord,
};

impl Staged {
    fn record(&self, at: f64, phase: usize, record: Record) -> TimedRecord {
        let loading = self.world.loading.as_ref().unwrap();
        let sources = [self.scene.plates.sources[0], self.scene.plates.sources[1]];
        TimedRecord {
            at,
            phase,
            bodies: sources.map(|s| loading.regions[&s.region.id].body),
            sources,
            record,
        }
    }
}

fn summary(world: &PhysicsWorld) -> Option<NumericalSummary> {
    let h = &world.loading.as_ref()?.gravity.as_ref()?.history;
    Some(NumericalSummary {
        scales: h.scales,
        errors: h.errors,
    })
}

fn arrival(x: f64, v: f64, a: f64) -> f64 {
    let gap = (-x).max(0.0);
    let speed = libm::sqrt(v * v + 2.0 * a * gap);
    if v >= 0.0 {
        2.0 * gap / (v + speed)
    } else {
        (speed - v) / a
    }
}

fn apex(v: f64, a: f64) -> Result<f32, Rejection> {
    let mut duration = (-v / a) as f32;
    for _ in 0..3 {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::TimePrecision);
        }
        if v + a * f64::from(duration) >= 0.0 {
            return Ok(duration);
        }
        duration = f32::from_bits(duration.to_bits() + 1);
    }
    Err(Rejection::TimePrecision)
}

pub(in crate::world::loading::response) fn step(
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
) -> Result<Trace, Rejection> {
    with_limits(world, dt, policy, MAX_INTERVALS, MAX_IMPACTS)
}

// Private configurable limits support late-rollback qualification; production
// callers always use the bounded entry above.
pub(in crate::world::loading::response) fn with_limits(
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
    intervals: usize,
    impacts: usize,
) -> Result<Trace, Rejection> {
    with_work_limits(
        world,
        dt,
        policy,
        Limits {
            intervals,
            impacts,
            ..Limits::default()
        },
    )
}

pub(in crate::world::loading::response) fn with_work_limits(
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
    limits: Limits,
) -> Result<Trace, Rejection> {
    let started = Instant::now();
    if !dt.is_finite() || dt < 0.0 {
        return Err(Rejection::InvalidTimestep);
    }
    let mut trace = Trace::default();
    if dt == 0.0 {
        world.step(0.0);
        trace.numerical = summary(world);
        trace.wall_time = started.elapsed();
        return Ok(trace);
    }
    let mut staged = Staged::prepare(world, policy)?;
    let mut clock = Clock::new(dt);
    let mut impact_count = 0;
    loop {
        let remaining = clock.remaining()?;
        if remaining == 0.0 {
            break;
        }
        let (x, v, _) = staged.motion();
        if x >= -TOUCH && v > 0.0 {
            (staged, impact_count) =
                resolve(staged, &mut trace, clock.elapsed(), impact_count, limits)?;
        }
        if trace.durations.len() >= limits.intervals.min(MAX_INTERVALS) {
            return Err(Rejection::AdvanceLimit);
        }
        let (x, v, _) = staged.motion();
        if x >= -TOUCH && v == 0.0 {
            let duration = time::bounded(remaining, remaining)?;
            let at = clock.elapsed();
            clock.advance(duration)?;
            let phase = staged.metrics.len();
            let (next, support, event) =
                staged.hold(duration, limits.projections, limits.support_captures)?;
            staged = next;
            let support_phase = phase + usize::from(event.is_some());
            if let Some(event) = event {
                trace
                    .records
                    .push(staged.record(at, phase, Record::Episode(event)));
            }
            trace
                .records
                .push(staged.record(at, support_phase, Record::Support(support)));
            trace.durations.push(duration);
            continue;
        }
        let a = staged.scene.acceleration;
        let toi = arrival(x, v, a);
        if !toi.is_finite() || toi <= 0.0 {
            return Err(Rejection::Numerical);
        }
        let mut at_arrival = toi <= remaining;
        // Round the event time normally, but never round beyond the frame's
        // remaining time. A downward-rounded remainder gets its own later phase.
        let mut duration = time::bounded(toi.min(remaining), remaining)?;
        if v < 0.0 && -v / a < f64::from(duration) {
            let turn = apex(v, a)?;
            if turn < duration {
                duration = turn;
                at_arrival = false;
            }
        }
        // Reject temporal loss before doing this phase's backend work.
        clock.advance(duration)?;
        let phase = staged.metrics.len();
        let (next, flight, departure) = staged.fly(duration)?;
        staged = next;
        trace.durations.push(duration);
        trace
            .records
            .push(staged.record(clock.elapsed(), phase, Record::Flight(flight)));
        if let Some(event) = departure {
            trace
                .records
                .push(staged.record(clock.elapsed(), phase, Record::Episode(event)));
        }
        // Rounded backend motion can reach the contact band before the analytic
        // root. Apply the same observed-arrival rule as the next loop iteration,
        // including when this flight consumed the frame's final interval.
        let (x, v, _) = staged.motion();
        if at_arrival || (x >= -TOUCH && v > 0.0) {
            (staged, impact_count) =
                resolve(staged, &mut trace, clock.elapsed(), impact_count, limits)?;
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
    trace: &mut Trace,
    at: f64,
    count: usize,
    limits: Limits,
) -> Result<(Staged, usize), Rejection> {
    let phase = staged.metrics.len();
    if !staged.world.loading.as_ref().unwrap().episodes.is_empty() {
        let (staged, settlement) = staged.settle(limits.projections)?;
        trace
            .records
            .push(staged.record(at, phase, Record::Settlement(settlement)));
        return Ok((staged, count));
    }
    if count >= limits.impacts.min(MAX_IMPACTS) {
        return Err(Rejection::SolveLimit);
    }
    let (staged, impact) = staged.impact()?;
    if impact.outgoing == 0.0 {
        staged.safe_support(0.0)?;
    }
    trace
        .records
        .push(staged.record(at, phase, Record::Episode(impact.event)));
    trace
        .records
        .push(staged.record(at, phase, Record::Impact(impact)));
    Ok((staged, count + 1))
}

#[cfg(test)]
mod tests;
