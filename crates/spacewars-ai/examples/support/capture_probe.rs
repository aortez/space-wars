//! Read-only follow-through of one native site choice, never a forced landing.
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, PlanetClaimPhase, TransferResult,
        mission::MissionObservationV1, pilot::PilotObservationV1,
    },
};
use serde_json::{Value, json};
use spacewars_ai::{
    mission_evaluation::MissionEvaluator,
    mission_pilot::{MissionGoal, MissionTelemetry},
    tactical_capture::CaptureTelemetry,
};

const NAMES: [&str; 7] = [
    "choice",
    "landed",
    "exited",
    "claim_started",
    "claimed",
    "boarded",
    "departed",
];

fn retries(c: &CaptureTelemetry) -> [u32; 8] {
    [
        c.replans,
        c.invalidations,
        c.cover_replans,
        c.solar_replans,
        c.circling_replans,
        c.objective_replans,
        c.landing.invalidations,
        c.landing.landing_retries,
    ]
}

struct Anchor {
    pilot: PilotObservationV1,
    mission: MissionTelemetry,
}

pub struct CaptureProbe {
    pub horizon_ticks: u64,
    anchor: Option<Anchor>,
    source: Option<Value>,
    milestones: [Option<u64>; 7],
    last_observed_tick: Option<u64>,
    observed_rows: u64,
    tactical_completed: Option<u64>,
    touchdown: Option<Value>,
    outcome: Option<Value>,
}

impl CaptureProbe {
    pub fn new(seconds: u64) -> Self {
        assert!((1..=180).contains(&seconds));
        Self {
            horizon_ticks: seconds * 60,
            anchor: None,
            source: None,
            milestones: [None; 7],
            last_observed_tick: None,
            observed_rows: 0,
            tactical_completed: None,
            touchdown: None,
            outcome: None,
        }
    }

    pub fn started(&self) -> bool {
        self.anchor.is_some()
    }

    pub fn done(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn start(
        &mut self,
        o: &MissionObservationV1,
        m: &MissionTelemetry,
        evaluator: &MissionEvaluator,
        choice: &Value,
    ) {
        assert!(!self.started());
        let p = &o.local.combat.recovery.flight.pilot;
        let (reference, unknown) = match evaluator.fresh_capture_reference(o, m) {
            Ok(reference) => (Some(reference), None),
            Err(reason) => (None, Some(reason)),
        };
        self.source = Some(json!({"pilot":p,"mission":m,
            "selected":choice["report"]["selected"],"choice_unknown":choice["unknown"],
            "reference":reference,"reference_unknown":unknown,"physics_queries":0}));
        self.milestones[0] = Some(p.tick);
        self.anchor = Some(Anchor {
            pilot: p.clone(),
            mission: m.clone(),
        });
        // This first slice has a neutral domain. Unsupported source contexts
        // remain explicit outcomes; missing empirical costs do not stop physics.
        if p.planet
            .claim
            .as_ref()
            .is_none_or(|c| c.owner.is_some() || c.flag.is_some())
            || choice["report"].is_null()
        {
            self.last_observed_tick = Some(p.tick);
            self.observed_rows = 1;
            self.stop(p.tick, "unsupported_source", true);
        }
    }

    pub fn observe(&mut self, o: &MissionObservationV1, m: &MissionTelemetry) {
        assert!(!self.done());
        let p = &o.local.combat.recovery.flight.pilot;
        let start = self.milestones[0].unwrap();
        assert_eq!(p.tick, self.last_observed_tick.map_or(start, |t| t + 1));
        self.last_observed_tick = Some(p.tick);
        self.observed_rows += 1;
        if let Some(reason) = self.inspect(o, m) {
            self.stop(p.tick, reason, true);
        } else if p.tick - start >= self.horizon_ticks {
            self.stop(p.tick, "observation_horizon", true);
        }
    }

    pub fn attach_neutral_timing(&mut self, timing: Value) {
        self.source.as_mut().expect("timing needs a capture source")["neutral_timing"] = timing;
    }

    fn inspect(&mut self, o: &MissionObservationV1, m: &MissionTelemetry) -> Option<&'static str> {
        let a = self.anchor.as_ref().unwrap();
        let p = &o.local.combat.recovery.flight.pilot;
        let original = &a.pilot;
        let original_capture = a.mission.capture.as_ref().unwrap();
        let destination = original.planet.index;
        let start = original.tick;
        if p.owner != original.owner
            || p.vehicle != original.vehicle
            || p.spaceling != original.spaceling
        {
            return Some("actor_or_vehicle_changed");
        }
        if !p.ship_available
            || p.ship_health <= 0.0
            || p.ship_form != ShipForm::Ship
            || p.location == PilotLocation::OnFoot && p.actor.is_none()
        {
            return Some("ship_or_pilot_lost");
        }
        if m.goal == MissionGoal::Recover || m.recovery.is_some() {
            return Some("recovery");
        }
        let Some(planet) = o.planets.iter().find(|v| v.index == destination) else {
            return Some("destination_absent");
        };
        let Some(claim) = &planet.claim else {
            return Some("claim_unavailable");
        };
        let initial_claim = original.planet.claim.as_ref().unwrap();
        if planet.revision != original.planet.revision
            || planet.radius != original.planet.radius
            || claim.stage_required_seconds != initial_claim.stage_required_seconds
            || claim.flag_interaction_range != initial_claim.flag_interaction_range
        {
            return Some("material_or_rules_changed");
        }
        if claim.owner.is_some_and(|v| v != p.owner)
            || claim.flag.is_some_and(|f| f.player != p.owner)
            || claim.claimant.is_some_and(|v| v != p.owner)
        {
            return Some("foreign_claim_context");
        }
        if self.milestones[4].is_some() && (claim.owner != Some(p.owner) || claim.flag.is_none()) {
            return Some("ownership_lost");
        }
        if m.replans != a.mission.replans
            || p.tick > start
                && m.events.iter().any(|e| {
                    e.tick == p.tick && matches!(e.kind, "selected" | "replan" | "arrived")
                })
        {
            return Some("attempt_replaced");
        }
        // Native departure clears target/capture. Its two existing admission
        // paths are retained; a synthetic finish can never reach this method.
        if m.events
            .iter()
            .any(|e| e.tick == p.tick && e.kind == "departed" && e.planet == Some(destination))
        {
            if self.milestones[5].is_none()
                || p.location != original.location
                || m.completed_sorties != a.mission.completed_sorties + 1
                || !(self.tactical_completed.is_some()
                    || p.ship.position.distance_to(planet.motion.position) > planet.radius + 70.0)
            {
                return Some("unwitnessed_departure");
            }
            self.milestones[6] = Some(p.tick);
            return Some("departed");
        }
        if m.target != Some(destination)
            || !matches!(m.goal, MissionGoal::Capture | MissionGoal::AvoidSun)
        {
            return Some("retargeted");
        }
        let Some(c) = &m.capture else {
            return Some("capture_ended");
        };
        if c.failed_tick.is_some() || c.failure.is_some() {
            return Some("capture_failed");
        }
        if c.started_tick != original_capture.started_tick
            || retries(c) != retries(original_capture)
        {
            return Some("capture_replanned");
        }
        if c.site != original_capture.site
            || c.landing
                .site
                .is_some_and(|s| Some(s) != original_capture.site)
        {
            return Some("site_changed");
        }
        if p.planet.index != destination && self.milestones[5].is_none() {
            return Some("approach_frame_changed");
        }
        let native = [
            c.landing.landed_tick,
            c.landing.claimed_tick,
            c.landing.boarded_tick,
        ];
        let owned = claim.owner == Some(p.owner)
            && claim.flag.is_some_and(|f| f.player == p.owner)
            && claim.captures > initial_claim.captures;
        let events = [
            (
                1,
                native[0],
                p.landing.phase == LandingPhase::Landed && p.location == original.location,
            ),
            (
                2,
                (p.location == PilotLocation::OnFoot).then_some(p.tick),
                p.last_transfer == TransferResult::Exited && p.transfers > original.transfers,
            ),
            (
                3,
                (p.location == PilotLocation::OnFoot
                    && claim.claimant == Some(p.owner)
                    && matches!(
                        claim.phase,
                        PlanetClaimPhase::Raising | PlanetClaimPhase::Lowering
                    )
                    && claim.progress > 0.0)
                    .then_some(p.tick),
                true,
            ),
            (4, native[1], owned && p.location == PilotLocation::OnFoot),
            (
                5,
                native[2],
                p.location == original.location
                    && p.last_transfer == TransferResult::Boarded
                    && p.transfers >= original.transfers + 2,
            ),
        ];
        let first_landing = self.milestones[1].is_none() && native[0].is_some();
        // The source pose is expressed in the body's rotating local frame so
        // a later touchdown offset doesn't include the planet's own motion.
        let touchdown = first_landing.then(|| {
            let site = original.sites.iter().find(|s| Some(s.id) == original_capture.site).unwrap();
            let expected_local = (site.vehicle_position - original.planet.motion.position)
                .rotate_radians(-original.planet.motion.angle);
            let actual_local = (p.ship.position - planet.motion.position).rotate_radians(-planet.motion.angle);
            json!({"tick":p.tick,"expected_local":expected_local,"actual_local":actual_local,
                "offset":actual_local.distance_to(expected_local),"ship":p.ship,"landing":p.landing})
        });
        for (index, tick, witness) in events {
            if let Some(tick) = tick
                && self.milestones[index].is_none()
            {
                if tick != p.tick || !witness || self.milestones[index - 1].is_none() {
                    return Some("unwitnessed_milestone");
                }
                self.milestones[index] = Some(tick);
                if index == 1 {
                    self.touchdown = touchdown.clone();
                }
            }
        }
        if let Some(tick) = c.completed_tick {
            if self.milestones[5].is_none() || tick < self.milestones[5].unwrap() || tick > p.tick {
                return Some("unwitnessed_completion");
            }
            self.tactical_completed.get_or_insert(tick);
        }
        None
    }

    fn stop(&mut self, tick: u64, reason: &'static str, controller_observed: bool) {
        self.outcome = Some(
            json!({"tick":tick,"reason":reason,"controller_observed":controller_observed,
            "elapsed_ticks":tick-self.milestones[0].unwrap()}),
        );
    }

    pub fn finish(&mut self, tick: u64, match_finished: bool) -> Value {
        if self.started() && !self.done() {
            self.stop(
                tick,
                if match_finished {
                    "match_finished"
                } else {
                    "runner_ended"
                },
                false,
            );
        }
        let milestones: serde_json::Map<_, _> = NAMES
            .into_iter()
            .zip(self.milestones)
            .map(|(name, tick)| (name.to_owned(), json!(tick)))
            .collect();
        json!({"schema":1,"horizon_ticks":self.horizon_ticks,"source":self.source,
            "last_observed_tick":self.last_observed_tick,"observed_rows":self.observed_rows,
            "milestones":milestones,"tactical_completed_tick":self.tactical_completed,
            "touchdown":self.touchdown,"outcome":self.outcome,
            "scope":"First native neutral capture attempt only. Frozen empirical reference, ordinary physical controls, no new queries or live planner work. Contacts and damage alone do not interrupt. Replans/material changes/foreign claims/loss interrupt; Configured horizon and match/runner ends censor. Final command unexecuted; finish cannot add milestones. Unknown references remain unknown; successful phase durations do not price interrupted phases."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::Vec2;
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{PlanetFlagObservation, SurfaceSortieScenario},
    };
    use spacewars_ai::{
        BrainReset,
        mission_pilot::MissionEvent,
        mission_policy::{MissionBot, MissionPolicy},
        tactical_capture::TacticalCapturePilot,
    };
    use std::time::Duration;

    fn fixture() -> (CaptureProbe, MissionObservationV1, MissionTelemetry) {
        let mut state = SurfaceSortieScenario::init_material_match(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let context = BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        };
        let mut o = state.mission_observation(0, None);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 100;
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = None;
        claim.flag = None;
        claim.claimant = None;
        let site = p.sites[0].id;
        o.planets[p.planet.index] = p.planet.clone();
        let mut m = MissionBot::new(MissionPolicy::Planner, context, Default::default())
            .telemetry()
            .clone();
        m.goal = MissionGoal::Capture;
        m.target = Some(p.planet.index);
        let mut c = TacticalCapturePilot::new(context, Default::default())
            .telemetry()
            .clone();
        c.sortie.started_tick = Some(100);
        c.sortie.site = Some(site);
        m.capture = Some(c);
        let mut probe = CaptureProbe::new(120);
        probe.start(
            &o,
            &m,
            &MissionEvaluator::new(1),
            &json!({"report":{"selected":{"site":site}},"unknown":null}),
        );
        probe.observe(&o, &m);
        assert!(!probe.done());
        (probe, o, m)
    }

    fn step(
        probe: &mut CaptureProbe,
        o: &mut MissionObservationV1,
        m: &mut MissionTelemetry,
        update: impl FnOnce(&mut PilotObservationV1, &mut MissionTelemetry),
    ) {
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        update(p, m);
        o.planets[p.planet.index] = p.planet.clone();
        probe.observe(o, m);
    }

    fn through_boarding(
        probe: &mut CaptureProbe,
        o: &mut MissionObservationV1,
        m: &mut MissionTelemetry,
    ) {
        let location = o.local.combat.recovery.flight.pilot.location;
        step(probe, o, m, |p, m| {
            p.landing.phase = LandingPhase::Landed;
            m.capture.as_mut().unwrap().sortie.landing.landed_tick = Some(p.tick);
        });
        step(probe, o, m, |p, _| {
            p.location = PilotLocation::OnFoot;
            p.actor = Some(p.ship);
            p.last_transfer = TransferResult::Exited;
            p.transfers += 1;
        });
        step(probe, o, m, |p, _| {
            let c = p.planet.claim.as_mut().unwrap();
            c.claimant = Some(p.owner);
            c.phase = PlanetClaimPhase::Raising;
            c.progress = 0.1;
        });
        step(probe, o, m, |p, m| {
            let c = p.planet.claim.as_mut().unwrap();
            c.owner = Some(p.owner);
            c.captures += 1;
            c.flag = Some(PlanetFlagObservation {
                player: p.owner,
                position: Vec2::ZERO,
                normal: Vec2::Y,
                raised_fraction: 1.0,
            });
            m.capture.as_mut().unwrap().sortie.landing.claimed_tick = Some(p.tick);
        });
        step(probe, o, m, |p, m| {
            p.location = location;
            p.last_transfer = TransferResult::Boarded;
            p.transfers += 1;
            m.capture.as_mut().unwrap().sortie.landing.boarded_tick = Some(p.tick);
        });
        assert!(!probe.done());
    }

    #[test]
    fn actual_loop_survives_expected_claim_and_cleared_departure_task() {
        for tactical_path in [false, true] {
            let (mut probe, mut o, mut m) = fixture();
            through_boarding(&mut probe, &mut o, &mut m);
            step(&mut probe, &mut o, &mut m, |p, m| {
                if tactical_path {
                    m.capture.as_mut().unwrap().sortie.completed_tick = Some(p.tick);
                }
            });
            step(&mut probe, &mut o, &mut m, |p, m| {
                if !tactical_path {
                    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 71.0);
                }
                m.events.push(MissionEvent {
                    tick: p.tick,
                    planet: m.target,
                    kind: "departed",
                    reason: None,
                });
                m.capture = None;
                m.target = None;
                m.goal = MissionGoal::Select;
                m.completed_sorties += 1;
            });
            let r = probe.finish(107, false);
            assert_eq!(r["outcome"]["reason"], "departed");
            assert_eq!(r["milestones"]["departed"], 107);
            assert_eq!(r["milestones"]["claimed"], 104);
            assert_eq!(r["observed_rows"], 8);
            assert!(r["source"]["reference"].is_null()); // Unknown doesn't prevent real completion.
        }
    }

    #[test]
    fn inner_retries_and_material_changes_interrupt_before_stale_milestones() {
        for kind in 0..4 {
            let (mut probe, mut o, mut m) = fixture();
            step(&mut probe, &mut o, &mut m, |p, m| {
                let c = &mut m.capture.as_mut().unwrap().sortie;
                match kind {
                    0 => c.landing.landing_retries += 1,
                    1 => c.landing.invalidations += 1,
                    2 => c.solar_replans += 1,
                    _ => p.planet.revision += 1,
                }
                c.landing.landed_tick = Some(p.tick);
                p.landing.phase = LandingPhase::Landed;
            });
            let r = probe.finish(101, false);
            assert_eq!(
                r["outcome"]["reason"],
                if kind < 3 {
                    "capture_replanned"
                } else {
                    "material_or_rules_changed"
                }
            );
            assert!(r["milestones"]["landed"].is_null());
        }
    }

    #[test]
    fn settling_damage_and_initial_inner_site_are_not_replans() {
        let (mut probe, mut o, mut m) = fixture();
        step(&mut probe, &mut o, &mut m, |p, m| {
            p.ship_health -= 1.0;
            let c = &mut m.capture.as_mut().unwrap().sortie;
            c.landing.touchdown_adjustments += 1;
            c.landing.site = c.site;
            c.landing.blocked_reason = Some("no usable hatch floor");
            m.goal = MissionGoal::AvoidSun;
        });
        assert!(!probe.done());
    }

    #[test]
    fn milestone_requires_current_physical_witness_and_predecessor() {
        for kind in 0..3 {
            let (mut probe, mut o, mut m) = fixture();
            step(&mut probe, &mut o, &mut m, |p, m| {
                let c = &mut m.capture.as_mut().unwrap().sortie;
                match kind {
                    0 => {
                        c.landing.landed_tick = Some(p.tick - 1);
                        p.landing.phase = LandingPhase::Landed;
                    }
                    1 => {
                        c.landing.landed_tick = Some(p.tick);
                        p.landing.phase = LandingPhase::Flying;
                    }
                    _ => {
                        p.location = PilotLocation::OnFoot;
                        p.actor = Some(p.ship);
                        p.last_transfer = TransferResult::Exited;
                        p.transfers += 1;
                    }
                }
            });
            assert_eq!(
                probe.finish(101, false)["outcome"]["reason"],
                "unwitnessed_milestone"
            );
        }
    }

    #[test]
    fn valid_landing_keeps_touchdown_before_invalid_later_clock() {
        let (mut probe, mut o, mut m) = fixture();
        step(&mut probe, &mut o, &mut m, |p, m| {
            p.landing.phase = LandingPhase::Landed;
            let c = &mut m.capture.as_mut().unwrap().sortie;
            c.landing.landed_tick = Some(p.tick);
            c.landing.claimed_tick = Some(p.tick);
        });
        let r = probe.finish(101, false);
        assert_eq!(r["outcome"]["reason"], "unwitnessed_milestone");
        assert_eq!(r["milestones"]["landed"], 101);
        assert!(r["milestones"]["claimed"].is_null());
        assert_eq!(r["touchdown"]["tick"], 101);
    }

    #[test]
    fn loss_beats_departure_and_finish_never_adds_milestones() {
        let (mut probe, mut o, mut m) = fixture();
        through_boarding(&mut probe, &mut o, &mut m);
        step(&mut probe, &mut o, &mut m, |p, m| {
            p.ship_health = 0.0;
            m.events.push(MissionEvent {
                tick: p.tick,
                planet: m.target,
                kind: "departed",
                reason: None,
            });
            m.capture = None;
            m.target = None;
        });
        let r = probe.finish(107, true);
        assert_eq!(r["outcome"]["reason"], "ship_or_pilot_lost");
        assert!(r["milestones"]["departed"].is_null());
        let (mut probe, _, _) = fixture();
        let r = probe.finish(101, true);
        assert_eq!(r["outcome"]["reason"], "match_finished");
        assert_eq!(r["last_observed_tick"], 100);
        assert_eq!(r["outcome"]["controller_observed"], false);
        assert!(r["milestones"]["landed"].is_null());
    }

    #[test]
    fn bounded_observation_does_not_expire_the_frozen_reference() {
        let (mut probe, mut o, m) = fixture();
        let source = probe.source.clone();
        for tick in 101..=7300 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            probe.observe(&o, &m);
            assert_eq!(probe.done(), tick == 7300);
        }
        let r = probe.finish(7300, false);
        assert_eq!(r["outcome"]["reason"], "observation_horizon");
        assert_eq!(r["source"], source.unwrap());
        assert_eq!(r["observed_rows"], 7201);
    }
}
