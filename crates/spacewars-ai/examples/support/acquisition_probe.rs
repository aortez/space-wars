//! Bounded observation of the first site choice after a real capture handoff.
//! This clock never changes controls or enables the native acquisition deadline.
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{PilotLocation, mission::MissionObservationV1, pilot::LandingSiteId},
};
use serde_json::{Value, json};
use spacewars_ai::mission_pilot::{MissionGoal, MissionTelemetry};

pub struct AcquisitionProbe {
    pub horizon_ticks: u64,
    started_tick: Option<u64>,
    last_observed_tick: Option<u64>,
    observed_rows: u64,
    initial_replans: Option<u32>,
    outcome: Option<Value>,
}

impl AcquisitionProbe {
    pub fn new(seconds: u64) -> Self {
        assert!((1..=180).contains(&seconds));
        Self {
            horizon_ticks: seconds * 60,
            started_tick: None,
            last_observed_tick: None,
            observed_rows: 0,
            initial_replans: None,
            outcome: None,
        }
    }

    pub fn start(&mut self, tick: u64) {
        assert!(self.started_tick.is_none());
        self.started_tick = Some(tick);
    }

    pub fn done(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn observe(&mut self, destination: usize, m: &MissionTelemetry, o: &MissionObservationV1) {
        let p = &o.local.combat.recovery.flight.pilot;
        let start = self
            .started_tick
            .expect("acquisition needs an actual handoff");
        let capture = m.capture.as_ref();
        if let Some(c) = capture
            && p.tick == start
        {
            self.initial_replans = Some(c.replans);
        }
        let mut interruption =
            if !p.ship_available || p.ship_health <= 0.0 || p.ship_form != ShipForm::Ship {
                Some("ship_or_pilot_lost")
            } else if m.goal == MissionGoal::Recover || m.recovery.is_some() {
                Some("recovery")
            } else if m.target != Some(destination) {
                Some("retargeted")
            } else if p.tick > start
                && m.events.iter().any(|e| {
                    e.tick == p.tick && matches!(e.kind, "selected" | "replan" | "arrived")
                })
            {
                Some("attempt_replaced")
            } else if capture.is_none() {
                Some("capture_ended")
            } else if capture.is_some_and(|c| c.failed_tick.is_some() || c.failure.is_some()) {
                Some("capture_failed")
            } else if p.planet.index != destination {
                Some("approach_frame_changed")
            } else if capture.is_some_and(|c| Some(c.replans) != self.initial_replans) {
                Some("capture_replanned")
            } else {
                None
            };
        let site = capture.and_then(|c| {
            c.started_tick
                .filter(|&tick| (start..=p.tick).contains(&tick))
                .and(c.site)
                .filter(|s| {
                    s.planet == destination
                        && c.acquisition.is_some_and(|a| {
                            a.tick == p.tick
                                && a.planet == destination
                                && a.selected_site == Some(*s)
                        })
                })
        });
        if interruption.is_none() && site.is_none() {
            interruption = if capture.is_some_and(|c| c.site.is_some()) {
                Some("unwitnessed_site")
            } else if p.location == PilotLocation::OnFoot
                || capture
                    .is_some_and(|c| c.landing.landed_tick.is_some() || c.completed_tick.is_some())
            {
                Some("physical_progress_without_choice")
            } else {
                None
            };
        }
        self.record(p.tick, interruption, site);
    }

    fn record(
        &mut self,
        tick: u64,
        interruption: Option<&'static str>,
        site: Option<LandingSiteId>,
    ) {
        assert!(!self.done());
        let start = self.started_tick.unwrap();
        assert_eq!(tick, self.last_observed_tick.map_or(start, |t| t + 1));
        self.last_observed_tick = Some(tick);
        self.observed_rows += 1;
        // A disrupted attempt is not a successful choice, even if stale site
        // telemetry survives that control tick. A real choice at the horizon
        // is observed; the endpoint command is not executed by the runner.
        let reason = interruption
            .or(site.map(|_| "site_selected"))
            .or((tick - start >= self.horizon_ticks).then_some("observation_horizon"));
        if let Some(reason) = reason {
            self.stop(tick, reason, true, site);
        }
    }

    fn stop(
        &mut self,
        tick: u64,
        reason: &'static str,
        controller_observed: bool,
        site: Option<LandingSiteId>,
    ) {
        self.outcome = Some(json!({"tick":tick,"reason":reason,
            "elapsed_ticks":tick-self.started_tick.unwrap(),"controller_observed":controller_observed,
            "site":site}));
    }

    pub fn finish(&mut self, tick: u64, match_finished: bool) -> Value {
        if self.started_tick.is_some() && !self.done() {
            // The last physics step may end the match. Do not manufacture a
            // controller observation or discover a site from stale telemetry.
            self.stop(
                tick,
                if match_finished {
                    "match_finished"
                } else {
                    "runner_ended"
                },
                false,
                None,
            );
        }
        json!({"schema":1,"horizon_ticks":self.horizon_ticks,"started_tick":self.started_tick,"initial_replans":self.initial_replans,
            "last_observed_tick":self.last_observed_tick,"observed_rows":self.observed_rows,
            "outcome":self.outcome,
            "scope":"Ordinary controls after the actual capture handoff. First observed site choice only, not a safe landing or capture. Half-open waiting interval excludes the terminal controller command. Interrupted and censored attempts are not timing samples. No native deadline enabled; no extra sensors or planner fuel. A null start means the transfer did not arrive."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Option<LandingSiteId> {
        Some(LandingSiteId {
            planet: 1,
            bearing: 5,
        })
    }

    #[test]
    fn arrival_clock_does_not_start_before_a_real_handoff() {
        let mut probe = AcquisitionProbe::new(30);
        let report = probe.finish(500, true);
        assert!(report["started_tick"].is_null());
        assert!(report["outcome"].is_null());
        assert_eq!(report["observed_rows"], 0);
    }

    #[test]
    fn first_choice_is_an_observation_not_an_executed_landing() {
        let mut probe = AcquisitionProbe::new(30);
        probe.start(120);
        probe.record(120, None, None);
        assert!(!probe.done());
        probe.record(121, None, site());
        assert!(probe.done());
        let report = probe.finish(121, false);
        assert_eq!(report["outcome"]["reason"], "site_selected");
        assert_eq!(report["outcome"]["elapsed_ticks"], 1);
        assert_eq!(report["observed_rows"], 2);
    }

    #[test]
    fn interruption_beats_same_tick_choice() {
        let mut probe = AcquisitionProbe::new(30);
        probe.start(120);
        probe.record(120, Some("recovery"), site());
        assert_eq!(probe.finish(120, false)["outcome"]["reason"], "recovery");
    }

    #[test]
    fn choice_at_the_inclusive_endpoint_is_observed() {
        for chosen in [None, site()] {
            let mut probe = AcquisitionProbe::new(1);
            probe.start(120);
            for tick in 120..180 {
                probe.record(tick, None, None);
                assert!(!probe.done());
            }
            probe.record(180, None, chosen);
            let report = probe.finish(180, false);
            assert_eq!(report["outcome"]["elapsed_ticks"], 60);
            assert_eq!(
                report["outcome"]["reason"],
                if chosen.is_some() {
                    "site_selected"
                } else {
                    "observation_horizon"
                }
            );
        }
    }

    #[test]
    fn match_end_has_no_synthetic_controller_row() {
        let mut probe = AcquisitionProbe::new(30);
        probe.start(120);
        probe.record(120, None, None);
        let report = probe.finish(121, true);
        assert_eq!(report["outcome"]["reason"], "match_finished");
        assert_eq!(report["outcome"]["controller_observed"], false);
        assert_eq!(report["last_observed_tick"], 120);
        assert_eq!(report["observed_rows"], 1);
        assert!(report["outcome"]["site"].is_null());
    }
}
