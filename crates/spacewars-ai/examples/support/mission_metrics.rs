//! Measurements only: no state mutations, controller inputs or clock advances.
use scenario_spacewars::surface_sortie::mission::MissionObservationV1;
use serde::Serialize;
use spacewars_ai::mission_pilot::{MissionGoal, MissionTelemetry};
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize)]
pub struct MissionMetrics {
    pub first_all_owned_tick: Option<u64>,
    pub first_hunt_tick: Option<u64>,
    pub first_contact_tick: Option<u64>,
    pub first_opportunity_tick: Option<u64>,
    pub pursuit_starts: u32,
    pub opportunity_ticks: u64,
    pub mission_ticks: BTreeMap<String, u64>,
    pub phase_ticks: BTreeMap<String, u64>,
    pub longest_phase_ticks: BTreeMap<String, u64>,
    pub visits: Vec<Visit>,
    #[serde(skip)]
    phase: String,
    #[serde(skip)]
    phase_since: u64,
    #[serde(skip)]
    last_event: Option<(u64, &'static str, Option<usize>)>,
}

#[derive(Debug, Serialize)]
pub struct Visit {
    planet: usize,
    selected_tick: u64,
    arrived_tick: Option<u64>,
    landed_tick: Option<u64>,
    claimed_tick: Option<u64>,
    boarded_tick: Option<u64>,
    departed_tick: Option<u64>,
    abandoned_tick: Option<u64>,
    reason: Option<&'static str>,
}

impl MissionMetrics {
    pub fn observe(&mut self, o: &MissionObservationV1, m: &MissionTelemetry) {
        let p = &o.local.combat.recovery.flight.pilot;
        *self
            .mission_ticks
            .entry(m.goal.label().to_owned())
            .or_default() += 1;
        if m.pursuit.is_some() && m.goal == MissionGoal::Hunt {
            self.first_opportunity_tick.get_or_insert(p.tick);
            self.opportunity_ticks += 1;
        }
        if o.planets.iter().all(|planet| {
            planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner))
        }) {
            self.first_all_owned_tick.get_or_insert(p.tick);
            if matches!(m.goal, MissionGoal::Hunt | MissionGoal::Watch) {
                self.first_hunt_tick.get_or_insert(p.tick);
            }
        }
        let phase = if m.goal == MissionGoal::Capture {
            m.capture.as_ref().map_or("capture", |c| c.goal.label())
        } else {
            m.goal.label()
        };
        if self.phase != phase {
            self.phase = phase.to_owned();
            self.phase_since = p.tick;
        }
        *self.phase_ticks.entry(phase.to_owned()).or_default() += 1;
        let longest = self
            .longest_phase_ticks
            .entry(phase.to_owned())
            .or_default();
        *longest = (*longest).max(p.tick.saturating_sub(self.phase_since) + 1);
        let first = self
            .last_event
            .and_then(|key| {
                m.events
                    .iter()
                    .position(|e| (e.tick, e.kind, e.planet) == key)
            })
            .map_or(0, |index| index + 1);
        for event in &m.events[first..] {
            if event.kind == "pursuit_started" {
                self.pursuit_starts += 1;
            }
            if event.kind == "selected" {
                self.visits.push(Visit {
                    planet: event.planet.unwrap(),
                    selected_tick: event.tick,
                    arrived_tick: None,
                    landed_tick: None,
                    claimed_tick: None,
                    boarded_tick: None,
                    departed_tick: None,
                    abandoned_tick: None,
                    reason: None,
                });
            } else if let Some(visit) = self.visits.last_mut() {
                match event.kind {
                    "arrived" => {
                        visit.arrived_tick.get_or_insert(event.tick);
                    }
                    "departed" => {
                        visit.departed_tick.get_or_insert(event.tick);
                    }
                    "replan" => {
                        visit.abandoned_tick.get_or_insert(event.tick);
                        visit.reason = event.reason;
                    }
                    _ => {}
                }
            }
            self.last_event = Some((event.tick, event.kind, event.planet));
        }
        if let Some(visit) = self.visits.last_mut()
            && m.target == Some(visit.planet)
            && let Some(capture) = &m.capture
        {
            let landing = &capture.landing;
            visit.landed_tick = visit.landed_tick.or(landing.landed_tick);
            visit.claimed_tick = visit.claimed_tick.or(landing.claimed_tick);
            visit.boarded_tick = visit.boarded_tick.or(landing.boarded_tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{CombatBreakSettings, Scenario};
    use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
    use spacewars_ai::{
        BrainReset,
        mission_pilot::{MaterialMissionPilot, MissionEvent},
    };
    use std::time::Duration;

    #[test]
    fn same_tick_reselection_retains_the_previous_attempt_and_never_invents_a_claim() {
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        let brain = MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            CombatBreakSettings::default(),
        );
        let mut mission = brain.telemetry().clone();
        let mut metrics = MissionMetrics::default();
        mission.events.push(MissionEvent {
            tick: 1,
            planet: Some(1),
            kind: "selected",
            reason: None,
        });
        metrics.observe(&o, &mission);
        o.local.combat.recovery.flight.pilot.tick = 2;
        mission.events.extend([
            MissionEvent {
                tick: 2,
                planet: Some(1),
                kind: "replan",
                reason: Some("destination already secured"),
            },
            MissionEvent {
                tick: 2,
                planet: Some(0),
                kind: "selected",
                reason: None,
            },
        ]);
        metrics.observe(&o, &mission);
        assert_eq!(metrics.visits.len(), 2);
        assert_eq!(metrics.visits[0].abandoned_tick, Some(2));
        assert_eq!(metrics.visits[1].abandoned_tick, None);
        assert!(
            metrics
                .visits
                .iter()
                .all(|v| v.claimed_tick.is_none() && v.departed_tick.is_none())
        );
        assert_eq!(metrics.first_hunt_tick, None);
        o.local.combat.recovery.flight.pilot.tick = 3;
        metrics.observe(&o, &mission);
        assert_eq!(metrics.visits.len(), 2);
        assert_eq!(metrics.phase_ticks.values().sum::<u64>(), 3);
    }

    #[test]
    fn an_opportunistic_chase_is_distinct_from_hunting_after_all_captures() {
        let mut state = SurfaceSortieScenario::init_material_match(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let o = state.mission_observation(0, None);
        let brain = MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            CombatBreakSettings::default(),
        );
        let mut mission = brain.telemetry().clone();
        mission.goal = MissionGoal::Hunt;
        mission.pursuit = Some(spacewars_ai::mission_pilot::MissionPursuit {
            started_tick: 1,
            last_visible_tick: 1,
            reason: "nearby vulnerable opponent",
        });
        mission.events.push(MissionEvent {
            tick: 1,
            planet: None,
            kind: "pursuit_started",
            reason: Some("nearby vulnerable opponent"),
        });
        let mut metrics = MissionMetrics::default();
        metrics.observe(&o, &mission);
        assert_eq!(metrics.first_opportunity_tick, Some(1));
        assert_eq!(metrics.pursuit_starts, 1);
        assert_eq!(metrics.opportunity_ticks, 1);
        assert_eq!(metrics.first_all_owned_tick, None);
        assert_eq!(metrics.first_hunt_tick, None);
        assert!(metrics.visits.is_empty());
        assert_eq!(metrics.mission_ticks.values().sum::<u64>(), 1);
    }
}
