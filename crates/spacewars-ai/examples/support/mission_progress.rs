//! Read-only progress measurement. Replanning is not physical progress.
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    mission::MissionObservationV1, pilot::PilotObservationV1,
};
use serde::Serialize;
use spacewars_ai::{
    mission_pilot::{MissionGoal, MissionTelemetry},
    tactical_sortie::TacticalGoal,
};
use std::collections::BTreeSet;

#[derive(Default, Serialize)]
pub struct MissionProgress {
    pub eligible_ticks: u64,
    pub excluded_ticks: u64,
    pub distance_observed_ticks: u64,
    pub progress_ticks: u64,
    pub ticks_without_progress: u64,
    pub ticks_after_20s_without_progress: u64,
    pub longest_no_progress_ticks: u64,
    #[serde(skip)]
    age: u64,
    #[serde(skip)]
    distance: Option<(String, f32)>,
    #[serde(skip)]
    claim: Option<(String, f32)>,
    #[serde(skip)]
    milestones: BTreeSet<(&'static str, u64)>,
}

pub const SCOPE: &str = "Per-tick observed progress during capture, transfer, launch, recovery and blocked/selection states: a new best goal distance (2 world units in flight, 0.2 on foot), a claim-fraction increase of 0.005, or a new arrived/landed/claimed/boarded/departed/recovery milestone. Changed goal, site, ground-task or local ground-endpoint identity initializes a distance baseline without resetting the no-progress clock. Replanning to the same ground endpoint retains its best distance. Combat, avoidance, patrol and watch ticks are excluded and end an interval. Missing distance observations still permit milestone/claim progress; report distance coverage separately. This measures lack of observed objective progress, not immobility, guaranteed failure or combat effectiveness. Detours can count as no progress.";

fn improved(previous: &mut Option<(String, f32)>, sample: Option<(String, f32, f32)>) -> bool {
    let Some((key, value, threshold)) = sample.filter(|s| s.1.is_finite()) else {
        return false;
    };
    if let Some((old_key, best)) = previous
        && *old_key == key
    {
        if value < *best - threshold {
            *best = value;
            return true;
        }
        return false;
    }
    *previous = Some((key, value));
    false
}

fn ground_distance(p: &PilotObservationV1, target: Vec2) -> Option<f32> {
    p.actor.map(|actor| {
        (actor.position - p.planet.motion.position)
            .rotate_radians(-p.planet.motion.angle)
            .distance_to(target)
    })
}

impl MissionProgress {
    fn record(
        &mut self,
        eligible: bool,
        distance: Option<(String, f32, f32)>,
        claim: Option<(String, f32, f32)>,
        milestone: bool,
    ) {
        if !eligible {
            self.excluded_ticks += 1;
            self.age = 0;
            self.distance = None;
            self.claim = None;
            return;
        }
        self.eligible_ticks += 1;
        self.distance_observed_ticks +=
            u64::from(distance.as_ref().is_some_and(|s| s.1.is_finite()));
        // Evaluate both even when a milestone already establishes progress.
        let moved = improved(&mut self.distance, distance);
        let claimed = improved(&mut self.claim, claim);
        if moved || claimed || milestone {
            self.progress_ticks += 1;
            self.age = 0;
        } else {
            self.ticks_without_progress += 1;
            self.age += 1;
            self.ticks_after_20s_without_progress += u64::from(self.age > 20 * 60);
            self.longest_no_progress_ticks = self.longest_no_progress_ticks.max(self.age);
        }
    }

    pub fn observe(&mut self, o: &MissionObservationV1, m: &MissionTelemetry) {
        let p = &o.local.combat.recovery.flight.pilot;
        let eligible = matches!(
            m.goal,
            MissionGoal::Select
                | MissionGoal::Launch
                | MissionGoal::Transfer
                | MissionGoal::Capture
                | MissionGoal::Recover
                | MissionGoal::Blocked
        );
        let mut milestone = false;
        for event in &m.events {
            if matches!(event.kind, "arrived" | "departed") {
                let new = self.milestones.insert((event.kind, event.tick));
                milestone |= new && event.tick == p.tick;
            }
        }
        if let Some(c) = &m.capture {
            for (kind, tick) in [
                ("landed", c.landing.landed_tick),
                ("claimed", c.landing.claimed_tick),
                ("boarded", c.landing.boarded_tick),
            ] {
                if let Some(tick) = tick {
                    let new = self.milestones.insert((kind, tick));
                    milestone |= new && tick == p.tick;
                }
            }
        }
        if let Some(r) = &m.recovery {
            for (kind, tick) in [
                ("recovery_landed", r.landed_tick),
                ("recovery_exited", r.exited_tick),
                ("rebuilt", r.rebuilt_tick),
                ("recovered", r.completed_tick),
            ] {
                if let Some(tick) = tick {
                    let new = self.milestones.insert((kind, tick));
                    milestone |= new && tick == p.tick;
                }
            }
        }
        let claim = p
            .planet
            .claim
            .as_ref()
            .filter(|c| c.claimant == Some(p.owner))
            .map(|c| {
                (
                    format!(
                        "{}:{:?}:{}:{}",
                        c.planet, c.phase, c.captures, c.neutralizations
                    ),
                    -c.progress,
                    0.005,
                )
            });
        let ground = if m.goal == MissionGoal::Capture {
            m.capture.as_ref().and_then(|c| c.ground.as_ref())
        } else if m.goal == MissionGoal::Recover {
            m.recovery.as_ref().and_then(|r| r.ground.as_ref())
        } else {
            None
        };
        let capture = m
            .capture
            .as_ref()
            .filter(|_| m.goal == MissionGoal::Capture);
        let distance = if let Some(g) = ground
            && let Some(distance) = g.target.and_then(|target| ground_distance(p, target))
        {
            // A route can choose a closer endpoint while the actor stands
            // still. Compare distances only within the same task and local
            // endpoint; a replan to that same endpoint must keep its best.
            Some((
                format!(
                    "ground:{:?}:{}:{:?}:{:?}:{:?}",
                    m.goal, p.planet.index, g.destination, g.started_tick, g.target
                ),
                distance,
                0.2,
            ))
        } else if m.goal == MissionGoal::Launch
            || capture.is_some_and(|c| c.goal == TacticalGoal::Depart)
        {
            Some((
                format!("climb:{}", p.planet.index),
                -p.ship.position.distance_to(p.planet.motion.position),
                2.0,
            ))
        } else if m.goal == MissionGoal::Transfer {
            m.target
                .and_then(|id| o.planets.iter().find(|p| p.index == id))
                .map(|planet| {
                    (
                        format!("transfer:{}", planet.index),
                        p.ship.position.distance_to(planet.motion.position) - planet.radius,
                        2.0,
                    )
                })
        } else if let Some(c) = capture {
            if c.goal == TacticalGoal::SeekCover {
                c.circling_remaining
                    .map(|r| (format!("circle:{:?}", c.site), r * p.planet.radius, 2.0))
            } else {
                c.site
                    .and_then(|id| p.sites.iter().find(|s| s.id == id))
                    .map(|site| {
                        (
                            format!("landing:{:?}:{}", site.id, site.revision),
                            p.ship.position.distance_to(site.vehicle_position),
                            2.0,
                        )
                    })
            }
        } else {
            None
        };
        self.record(eligible, distance, claim, milestone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{PilotLocation, SurfaceSortieScenario},
    };
    use spacewars_ai::{
        BrainReset,
        ground_task::{GroundDestination, GroundNavigationTask},
        mission_pilot::MaterialMissionPilot,
        tactical_capture::TacticalCapturePilot,
    };

    fn ground_fixture() -> (MissionObservationV1, MissionTelemetry) {
        let state = SurfaceSortieScenario::init_material_travel(42, false);
        let mut o = state.mission_observation(0, None);
        let context = BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        };
        let mut m = MaterialMissionPilot::new(context, Default::default())
            .telemetry()
            .clone();
        m.goal = MissionGoal::Capture;
        let mut capture = TacticalCapturePilot::new(context, Default::default())
            .telemetry()
            .clone();
        let mut ground = GroundNavigationTask::new(context, GroundDestination::Flag)
            .telemetry()
            .clone();
        ground.started_tick = Some(1);
        ground.target = Some(Vec2::new(20.0, 150.0));
        capture.ground = Some(ground);
        m.capture = Some(capture);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.location = PilotLocation::OnFoot;
        p.planet.motion.position = Vec2::ZERO;
        p.planet.motion.angle = 0.0;
        p.planet.claim = None;
        p.actor = Some(p.ship);
        p.actor.as_mut().unwrap().position = Vec2::new(0.0, 150.0);
        (o, m)
    }

    #[test]
    fn a_new_ground_endpoint_does_not_erase_a_stationary_stall() {
        let (mut o, mut m) = ground_fixture();
        let mut progress = MissionProgress::default();
        for tick in 1..=1300 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            progress.observe(&o, &m);
        }
        let ground = m.capture.as_mut().unwrap().ground.as_mut().unwrap();
        ground.target = Some(Vec2::new(10.0, 150.0));
        ground.replans += 1;
        o.local.combat.recovery.flight.pilot.tick += 1;
        progress.observe(&o, &m);
        assert_eq!(progress.progress_ticks, 0);
        assert_eq!(progress.longest_no_progress_ticks, 1301);
        assert_eq!(progress.ticks_after_20s_without_progress, 101);
        // Movement toward the newly established endpoint still counts.
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        p.actor.as_mut().unwrap().position.x = 1.0;
        progress.observe(&o, &m);
        assert_eq!(progress.progress_ticks, 1);
        assert_eq!(progress.age, 0);
    }

    #[test]
    fn the_same_endpoint_keeps_its_best_distance_until_the_ground_task_changes() {
        let (mut o, mut m) = ground_fixture();
        let mut progress = MissionProgress::default();
        for (tick, x) in [(1, 0.0), (2, 1.0), (3, 0.0)] {
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.tick = tick;
            p.actor.as_mut().unwrap().position.x = x;
            progress.observe(&o, &m);
        }
        m.capture.as_mut().unwrap().ground.as_mut().unwrap().replans += 1;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 4;
        p.actor.as_mut().unwrap().position.x = 1.0;
        progress.observe(&o, &m);
        assert_eq!(
            progress.progress_ticks, 1,
            "replanning must not reward oscillation"
        );
        assert_eq!(progress.age, 2);
        m.capture
            .as_mut()
            .unwrap()
            .ground
            .as_mut()
            .unwrap()
            .started_tick = Some(5);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 5;
        p.actor.as_mut().unwrap().position.x = 2.0;
        progress.observe(&o, &m);
        assert_eq!(
            progress.progress_ticks, 1,
            "a new task initializes its distance baseline"
        );
        assert_eq!(progress.age, 3);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 6;
        p.actor.as_mut().unwrap().position.x = 3.0;
        progress.observe(&o, &m);
        assert_eq!(progress.progress_ticks, 2);
        assert_eq!(progress.age, 0);
    }

    #[test]
    fn ground_distance_uses_the_rotating_planet_frame() {
        use engine_common::Scenario;
        use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], std::time::Duration::from_nanos(16_666_667));
        let mut p = state.pilot_observation(0, None);
        p.planet.motion.position = Vec2::new(100.0, 200.0);
        p.planet.motion.angle = std::f32::consts::FRAC_PI_2;
        p.actor = Some(p.ship);
        p.actor.as_mut().unwrap().position = Vec2::new(100.0, 203.0);
        assert!((ground_distance(&p, Vec2::new(5.0, 0.0)).unwrap() - 2.0).abs() < 0.0001);
        p.actor.as_mut().unwrap().position.y += 1.0;
        assert!((ground_distance(&p, Vec2::new(5.0, 0.0)).unwrap() - 1.0).abs() < 0.0001);
    }
    fn d(key: &str, distance: f32) -> Option<(String, f32, f32)> {
        Some((key.into(), distance, 0.2))
    }
    #[test]
    fn stationary_replans_do_not_hide_a_stall() {
        let mut m = MissionProgress::default();
        for tick in 0..1300 {
            m.record(true, d(&format!("site-{}", tick / 20), 5.0), None, false);
        }
        assert_eq!(m.longest_no_progress_ticks, 1300);
        assert_eq!(m.ticks_after_20s_without_progress, 100);
        assert_eq!(m.progress_ticks, 0);
    }
    #[test]
    fn oscillations_must_beat_the_best_distance() {
        let mut m = MissionProgress::default();
        for value in [10.0, 9.0, 10.0, 9.0, 10.0, 9.0] {
            m.record(true, d("flag", value), None, false);
        }
        assert_eq!(m.progress_ticks, 1);
        assert_eq!(m.longest_no_progress_ticks, 4);
        m.record(true, None, None, true);
        assert_eq!(m.age, 0);
    }
    #[test]
    fn claim_progress_missing_geometry_and_excluded_combat_are_explicit() {
        let mut m = MissionProgress::default();
        m.record(true, None, Some(("claim".into(), -0.1, 0.005)), false);
        m.record(true, None, Some(("claim".into(), -0.11, 0.005)), false);
        assert_eq!(m.progress_ticks, 1);
        assert_eq!(m.distance_observed_ticks, 0);
        m.record(false, None, None, false);
        assert_eq!(m.excluded_ticks, 1);
        assert_eq!(m.age, 0);
        assert_eq!(
            m.eligible_ticks,
            m.progress_ticks + m.ticks_without_progress
        );
    }
}
