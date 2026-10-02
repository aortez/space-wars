//! Fixed-clock offline route and native-choice diagnostics on a cloned world.
use scenario_spacewars::surface_sortie::{
    SurfaceSortieState, landing_objective::ObjectivePlanning, mission::MissionObservationV1,
};
use serde_json::{Value, json};
use spacewars_ai::mission_policy::MissionBot;
use std::{collections::BTreeSet, path::Path, time::Instant};

pub struct CoverProbe {
    seat: usize,
    ticks: BTreeSet<u64>,
    topology: bool,
    jetpack: bool,
    rows: Vec<Value>,
}

impl CoverProbe {
    pub fn from_args() -> Option<Self> {
        let jetpack: bool = crate::arg("--probe-cover-jetpack", "false")
            .parse()
            .unwrap();
        let topology: bool = crate::arg("--probe-cover-topology", "false")
            .parse()
            .unwrap();
        let requested = crate::arg("--probe-cover-ticks", "none");
        if requested == "none" {
            assert!(!topology, "topology probe requires cover probe ticks");
            assert!(!jetpack, "jetpack probe requires cover probe ticks");
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
            topology,
            jetpack,
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
        let mut row = json!({
            "world_tick":p.tick, "loop_tick":loop_tick, "seat":seat,
            "observation":o, "mission":bot.telemetry(),
            "choice":choice.as_ref().ok(), "choice_unknown":choice.as_ref().err(),
            "choice_ms":choice_ms, "route_batches":routes.as_ref().ok(),
            "routes_unknown":routes.as_ref().err(), "route_costs":costs,
            "routes_ms":routes_ms, "route_profile":profile,
        });
        if self.topology {
            let clock = Instant::now();
            let measure = || {
                if bot.policy().objective_planning() != ObjectivePlanning::JointRoundTrip {
                    return Err("diagnostic requires joint round-trip planning");
                }
                cloned.diagnose_landing_topology(seat, p)
            };
            #[cfg(feature = "sensor-profile")]
            let (topology, profile) =
                scenario_spacewars::surface_sortie::sensor_profile::measure(measure);
            #[cfg(not(feature = "sensor-profile"))]
            let (topology, profile) = (measure(), None::<Value>);
            row["topology_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
            row["topology"] = json!(topology.as_ref().ok());
            row["topology_unknown"] = json!(topology.as_ref().err());
            row["topology_profile"] = json!(profile);
        }
        if self.jetpack {
            let clock = Instant::now();
            let measure = || cloned.diagnose_jetpack_landing_routes(seat, p);
            #[cfg(feature = "sensor-profile")]
            let (jetpack, profile) =
                scenario_spacewars::surface_sortie::sensor_profile::measure(measure);
            #[cfg(not(feature = "sensor-profile"))]
            let (jetpack, profile) = (measure(), None::<Value>);
            row["jetpack_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
            row["jetpack"] = json!(jetpack.as_ref().ok());
            row["jetpack_unknown"] = json!(jetpack.as_ref().err());
            row["jetpack_profile"] = json!(profile);
        }
        self.rows.push(row);
    }

    pub fn finish(self, out: &Path) {
        let unreached: Vec<_> = self
            .ticks
            .iter()
            .filter(|&&tick| !self.rows.iter().any(|r| r["world_tick"] == tick))
            .collect();
        let mut result = json!({
            "schema":1, "model":"observed_cover_routes_v1", "seat":self.seat,
            "requested_world_ticks":self.ticks, "unreached_world_ticks":unreached, "rows":self.rows,
            "scope":"Read-only native ranker plus all observed sites measured in native bounded batches on a cloned world. Extra diagnostic physics queries and timings are outside live quotas. No new observations enter controls. No full-planet, future-cover, arrival or capture guarantee.",
        });
        if self.topology {
            result["topology_model"] = json!("native_landing_graph_v1");
            result["topology_scope"] = json!(
                "Native outer-contour graph, proposed hull exclusions and path witnesses. Without-ship routes omit only the proposed observing ship and never enter gameplay. Extra diagnostic work is outside live quotas."
            );
        }
        if self.jetpack {
            result["jetpack_model"] = json!("existing_powered_landing_probe_v1");
            result["jetpack_scope"] = json!(
                "Native walk/jump versus existing jetpack round trips on the same observation, preserving the proposed hull. Independent native batches and single-site work/rejection records are diagnostic only. No control, equipment, model threshold or live-budget changes."
            );
        }
        std::fs::write(
            out.join("cover-probe.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}
