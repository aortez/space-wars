//! Opt-in inspection after controls have consumed the unmodified observation.
use scenario_spacewars::surface_sortie::mission::MissionObservationV1;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub struct ActualLandingProbe {
    seat: usize,
    ticks: BTreeSet<u64>,
    rows: Vec<Value>,
}
impl ActualLandingProbe {
    pub fn from_args() -> Option<Self> {
        let requested = crate::arg("--probe-actual-landing-ticks", "none");
        if requested == "none" {
            return None;
        }
        let values: Vec<u64> = requested.split(',').map(|s| s.parse().unwrap()).collect();
        let ticks: BTreeSet<_> = values.iter().copied().collect();
        assert!(!ticks.is_empty() && ticks.len() <= 16 && values.len() == ticks.len());
        let seat = crate::arg("--probe-actual-landing-seat", "0")
            .parse()
            .unwrap();
        assert!(seat < 2);
        Some(Self {
            seat,
            ticks,
            rows: Vec::new(),
        })
    }
    pub fn observe(
        &mut self,
        seat: usize,
        o: &MissionObservationV1,
        live: &crate::live_planning::LivePlanningRun,
    ) {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        if seat != self.seat || !self.ticks.contains(&tick) {
            return;
        }
        assert!(live.enabled_for(seat));
        assert!(!self.rows.iter().any(|r| r["tick"] == tick));
        let measure = || live.diagnose_actual_request(seat, 1_000_000);
        #[cfg(feature = "sensor-profile")]
        let (probe, profile) = scenario_spacewars::surface_sortie::sensor_profile::measure(measure);
        #[cfg(not(feature = "sensor-profile"))]
        let (probe, profile) = (measure(), None::<Value>);
        self.rows
            .push(json!({"tick":tick,"seat":seat,"observation":o,
            "probe":probe,"diagnostic_profile":profile}));
    }
    pub fn finish(self, out: &Path) {
        let unreached: Vec<_> = self
            .ticks
            .iter()
            .filter(|&&tick| !self.rows.iter().any(|r| r["tick"] == tick))
            .collect();
        let report = json!({"schema":1,"model":"detached_actual_request_v1",
            "seat":self.seat,"requested_ticks":self.ticks,"unreached_ticks":unreached,
            "rows":self.rows,"scope":"Copies the existing actual-touchdown job and its retained immutable snapshot after native controls. At most one million extra native work steps per requested tick. No world advance, queue dispatch, new source clock, publication validation or live permission. Diagnostic work is outside live quotas and ordinary sensor profiling."});
        std::fs::write(
            out.join("actual-landing-probe.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
