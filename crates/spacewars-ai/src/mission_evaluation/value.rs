//! One-step ownership value. No fitted combat, survival or economic weights.
use super::*;

pub const MODEL: &str = "capture_mission_value_v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CaptureValue {
    /// Own count +1; taking an opponent's flag also subtracts one from theirs.
    pub ownership_swing: u8,
    /// Until we have a rebuild foothold, compare completion time alone.
    pub priority_units: u8,
    pub seconds_per_unit: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValueComparison {
    pub objective: &'static str,
    pub preferred: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ValueDecision {
    pub current: CaptureValue,
    pub destination: CaptureValue,
    /// Improvement expressed in seconds at the current trip's ownership value.
    pub equivalent_seconds_saved: f32,
}

pub(super) fn enabled(policy: &str) -> bool {
    policy == crate::mission_policy::MissionPolicy::ValuePlanner.id()
}

pub(super) fn finish(report: &mut MissionEvaluation) {
    let Some(value) = &mut report.value_comparison else {
        return;
    };
    if report.inactive_reason.is_some()
        || report.candidates_truncated
        || report.candidates.is_empty()
        || report.candidates.iter().any(|c| c.total_seconds.is_none())
    {
        return;
    }
    value.preferred = report
        .candidates
        .iter()
        .filter(|c| c.reference_exceeds_match_time != Some(true))
        .filter_map(|c| Some((c.planet, c.value?.seconds_per_unit?)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(planet, _)| planet);
}

pub(super) fn decision(
    current: &CaptureCandidate,
    destination: &CaptureCandidate,
) -> Option<ValueDecision> {
    let a = current.value?;
    let b = destination.value?;
    if a.priority_units == 0 || b.priority_units == 0 {
        return None;
    }
    let saved = current.total_seconds?
        - destination.total_seconds? * f32::from(a.priority_units) / f32::from(b.priority_units);
    Some(ValueDecision {
        current: a,
        destination: b,
        equivalent_seconds_saved: saved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mission_pilot::MissionEvent, mission_policy::MissionPolicy};

    fn fixture() -> (MissionObservationV1, MissionTelemetry, Vec<LocalEvidence>) {
        let (_, mut o, bot) = super::super::tests::fixture();
        o.sun = None;
        o.boundary.center = Vec2::ZERO;
        o.boundary.radius = 2000.0;
        o.planets.truncate(3);
        for (i, planet) in o.planets.iter_mut().enumerate() {
            planet.motion.position =
                [Vec2::ZERO, Vec2::new(300.0, 150.0), Vec2::new(-500.0, 0.0)][i];
            planet.motion.velocity = Vec2::ZERO;
            planet.radius = 50.0;
            let claim = planet.claim.as_mut().unwrap();
            claim.owner = [None, Some(PlayerId::PLAYER_2), Some(PlayerId::PLAYER_1)][i];
            claim.flag = None;
        }
        o.match_context.as_mut().unwrap().owned_planets = [1, 1];
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.ship.position = Vec2::new(0.0, 150.0);
        p.ship.velocity = Vec2::ZERO;
        p.ship.angle = 0.0;
        p.ship.spin = 0.0;
        p.gravity = -Vec2::Y * 10.0;
        p.planet = o.planets[0].clone();
        p.sites.clear();
        let mut mission = bot.telemetry().clone();
        mission.policy = MissionPolicy::ValuePlanner.id();
        mission.target = Some(0);
        mission.events.push(MissionEvent {
            tick: p.tick,
            planet: Some(0),
            kind: "selected",
            reason: None,
        });
        let evidence = o
            .planets
            .iter()
            .take(2)
            .enumerate()
            .map(|(i, planet)| {
                let mut sample =
                    super::super::tests::known(planet, p.tick, if i == 0 { 70.0 } else { 80.0 });
                sample.remote = true;
                sample
            })
            .collect();
        (o, mission, evidence)
    }

    #[test]
    fn enemy_capture_can_justify_a_longer_trip_but_first_foothold_prefers_time() {
        let (mut o, mission, evidence) = fixture();
        let report = super::super::tests::finish(snapshot(&o, &mission, &evidence));
        assert_eq!(report.preferred_by_time, Some(0));
        assert_eq!(report.value_comparison.as_ref().unwrap().preferred, Some(1));
        assert!(report.candidates[1].total_seconds > report.candidates[0].total_seconds);
        let decision = decision(&report.candidates[0], &report.candidates[1]).unwrap();
        assert_eq!(decision.current.ownership_swing, 1);
        assert_eq!(decision.destination.ownership_swing, 2);
        assert!(decision.equivalent_seconds_saved >= 14.0);
        assert_eq!(report.charged_work, 3);
        o.match_context.as_mut().unwrap().owned_planets[0] = 0;
        o.planets[2].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2);
        o.planets.truncate(2);
        let report = super::super::tests::finish(snapshot(&o, &mission, &evidence));
        assert_eq!(report.value_comparison.as_ref().unwrap().preferred, Some(0));
        assert_eq!(report.candidates[1].value.unwrap().priority_units, 1);
    }

    #[test]
    fn expensive_enemy_unknown_ground_and_match_deadline_are_not_free_value() {
        let (o, mission, evidence) = fixture();
        for mutation in 0..3 {
            let mut o = o.clone();
            let mut evidence = evidence.clone();
            match mutation {
                0 => evidence[1].costs.as_mut().unwrap().landing = 160.0,
                1 => evidence[1].costs = None,
                2 => o.match_context.as_mut().unwrap().remaining_seconds = Some(75.0),
                _ => unreachable!(),
            }
            let report = super::super::tests::finish(snapshot(&o, &mission, &evidence));
            assert_eq!(
                report.value_comparison.as_ref().unwrap().preferred,
                if mutation == 1 { None } else { Some(0) }
            );
        }
    }

    #[test]
    fn selection_rechecks_motion_and_ownership_before_controls_and_refreshes_its_source() {
        let (mut o, mission, evidence) = fixture();
        let mut evaluator = MissionEvaluator::new(1);
        evaluator.actors.insert(
            0,
            ActorState {
                evidence,
                ..Default::default()
            },
        );
        for tick in 1..=2 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            evaluator.observe(&o, &mission);
            evaluator.advance(tick, DEFAULT_WORK);
        }
        assert!(evaluator.selection(&o, &mission).is_some());
        for mutation in 0..10 {
            let mut changed = o.clone();
            let p = &mut changed.local.combat.recovery.flight.pilot;
            match mutation {
                0 => p.ship.position.x += 3.0,
                1 => p.ship.velocity.y += 3.0,
                2 => p.ship.angle += 0.2,
                3 => p.ship.spin += 0.3,
                4 => p.gravity.x += 1.0,
                5 => changed.planets[1].motion.position.x += 3.0,
                6 => changed.planets[1].motion.velocity.x += 3.0,
                7 => changed.match_context.as_mut().unwrap().owned_planets[0] = 0,
                8 => {
                    changed
                        .local
                        .combat
                        .recovery
                        .flight
                        .flight
                        .limits
                        .turn_speed *= 0.5
                }
                9 => p.planet = changed.planets[1].clone(),
                _ => unreachable!(),
            }
            assert!(
                evaluator.selection(&changed, &mission).is_none(),
                "mutation {mutation}"
            );
        }
        o.local.combat.recovery.flight.pilot.tick = 3;
        o.local.combat.recovery.flight.pilot.ship.position.x += 3.0;
        evaluator.observe(&o, &mission);
        assert!(evaluator.latest(PlayerId::PLAYER_1).is_none());
        assert_eq!(
            evaluator
                .advance(
                    3,
                    Work {
                        graph: 1,
                        physics_queries: 0
                    }
                )
                .graph,
            1
        );
        assert!(evaluator.latest(PlayerId::PLAYER_1).is_none());
        o.local.combat.recovery.flight.pilot.tick = 4;
        evaluator.observe(&o, &mission);
        evaluator.advance(4, DEFAULT_WORK);
        assert!(evaluator.selection(&o, &mission).is_some());
    }
}
