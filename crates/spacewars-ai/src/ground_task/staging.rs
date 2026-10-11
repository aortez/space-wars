//! Reuse a current staging survey for its already measured walking prefix.
use super::*;

impl GroundNavigationTask {
    pub(crate) fn from_staging_map(
        context: BrainReset,
        position: Vec2,
        o: &RecoveryTaskObservationV1,
        map: &GroundMap,
    ) -> Option<Self> {
        let p = &o.flight.pilot;
        let actor = p.actor?;
        if o.version != 1
            || o.flight.version != 2
            || o.flight.flight.version != 1
            || p.version != 1
            || p.owner != context.actor
            || p.location != PilotLocation::OnFoot
            || !p.queries_ready
        {
            return None;
        }
        let foot = (actor.position - p.actor_up * HALF_HEIGHT - p.planet.motion.position)
            .rotate_radians(-p.planet.motion.angle);
        if ![foot.x, foot.y, position.x, position.y]
            .iter()
            .all(|value| value.is_finite())
        {
            return None;
        }
        let mut task = Self::new(
            context,
            GroundDestination::Rebuild {
                planet: map.planet,
                position,
            },
        );
        // This is the measured native map, not a forecast or a resampled map.
        // Reuse the ordinary identity, tick, geometry and size validation.
        let mut measured = o.clone();
        measured.ground = Some(map.clone());
        if !task.update_map(&measured) {
            return None;
        }
        let route = map.route(foot, position, 0.01);
        if route.diagnostics.failure.is_some()
            || route.diagnostics.partial
            || route.diagnostics.jumps != 0
            || route.diagnostics.flights != 0
            || !(2.0..=4.0).contains(&route.diagnostics.length)
            || route.path.last().is_none_or(|id| {
                map.nodes
                    .iter()
                    .find(|node| node.id == *id)
                    .is_none_or(|node| node.position.distance_to(position) > 0.01)
            })
        {
            return None;
        }
        task.set_precise_rebuild(true);
        task.telemetry.staging_seed_tick = Some(map.tick);
        task.telemetry.target = Some(position);
        task.telemetry.path = route.path;
        task.telemetry.route = Some(route.diagnostics);
        task.telemetry.replans = 1;
        task.telemetry.last_progress_tick = p.tick;
        task.last_plan_tick = Some(map.tick);
        Some(task)
    }
}
