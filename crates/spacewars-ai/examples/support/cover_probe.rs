//! Fixed-clock offline route and native-choice diagnostics on a cloned world.
use scenario_spacewars::surface_sortie::{SurfaceSortieState, mission::MissionObservationV1};
use serde_json::{Value, json};
use spacewars_ai::mission_policy::MissionBot;
use std::{collections::BTreeSet, path::Path, time::Instant};

pub struct CoverProbe {
    seat: usize,
    ticks: BTreeSet<u64>,
    rows: Vec<Value>,
}

impl CoverProbe {
    pub fn from_args() -> Option<Self> {
        let requested = crate::arg("--probe-cover-ticks", "none");
        if requested == "none" {
            return None;
        }
        let values: Vec<u64> = requested
            .split(',')
            .map(|v| {
                v.parse()
                    .expect("cover probe requires comma-separated world ticks")
            })
            .collect();
        let ticks: BTreeSet<_> = values.iter().copied().collect();
        assert!(!ticks.is_empty() && ticks.len() <= 16 && ticks.len() == values.len());
        let seat = crate::arg("--probe-cover-seat", &crate::arg("--seat", "1"))
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
        state: &SurfaceSortieState,
        seat: usize,
        loop_tick: u64,
        bot: &MissionBot,
        o: &MissionObservationV1,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        if seat != self.seat || !self.ticks.contains(&p.tick) {
            return;
        }
        assert_eq!(state.tick(), p.tick);
        assert!(!self.rows.iter().any(|r| r["world_tick"] == p.tick));
        let clock = Instant::now();
        let choice = bot
            .telemetry()
            .capture
            .as_ref()
            .and_then(|c| c.site)
            .ok_or("no native selected site")
            .and_then(|site| bot.landing_choice_comparison(o, site));
        let choice_ms = clock.elapsed().as_secs_f64() * 1000.0;
        let clock = Instant::now();
        let cloned = state.clone();
        let measure = || cloned.diagnose_landing_routes(seat, p, bot.policy().objective_planning());
        #[cfg(feature = "sensor-profile")]
        let (routes, profile) = {
            let (routes, profile) =
                scenario_spacewars::surface_sortie::sensor_profile::measure(measure);
            (routes, Some(profile))
        };
        #[cfg(not(feature = "sensor-profile"))]
        let (routes, profile) = (measure(), None::<Value>);
        let routes_ms = clock.elapsed().as_secs_f64() * 1000.0;
        let costs: Vec<_> = routes
            .as_ref()
            .into_iter()
            .flatten()
            .flat_map(|s| &s.sites)
            .map(|route| json!({"site":route.site, "cost":route.cost()}))
            .collect();
        self.rows.push(json!({
            "world_tick":p.tick, "loop_tick":loop_tick, "seat":seat,
            "observation":o, "mission":bot.telemetry(),
            "choice":choice.as_ref().ok(), "choice_unknown":choice.as_ref().err(),
            "choice_ms":choice_ms, "route_batches":routes.as_ref().ok(),
            "routes_unknown":routes.as_ref().err(), "route_costs":costs,
            "routes_ms":routes_ms, "route_profile":profile,
        }));
    }

    pub fn finish(self, out: &Path) {
        let unreached: Vec<_> = self
            .ticks
            .iter()
            .filter(|&&tick| !self.rows.iter().any(|r| r["world_tick"] == tick))
            .collect();
        std::fs::write(out.join("cover-probe.json"), serde_json::to_vec_pretty(&json!({
            "schema":1, "model":"observed_cover_routes_v1", "seat":self.seat,
            "requested_world_ticks":self.ticks, "unreached_world_ticks":unreached, "rows":self.rows,
            "scope":"Read-only native ranker plus all observed sites measured in native bounded batches on a cloned world. Extra diagnostic physics queries and timings are outside live quotas. No new observations enter controls. No full-planet, future-cover, arrival or capture guarantee.",
        })).unwrap()).unwrap();
    }
}
