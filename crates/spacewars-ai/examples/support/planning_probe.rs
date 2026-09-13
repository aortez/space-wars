//! Offline quota sweep over a bounded sample of already-measured ground maps.
//! These diagnostic jobs never choose controls or issue additional world queries.
use engine_core::planning::{JobLimits, JobPhase, JobPoll, PlanningJob, PlanningQueue, Work};
use scenario_spacewars::surface_sortie::ground_navigation::{GroundMap, GroundRoundTripJob};
use serde_json::json;
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
    sync::Arc,
    time::Instant,
};

const SNAPSHOTS: usize = 6;

#[derive(Default)]
pub struct PlanningProbe {
    maps: Vec<Arc<GroundMap>>,
    last_tick: [Option<u64>; 2],
}

impl PlanningProbe {
    /// Copy at most six maps, spaced two simulated seconds apart per seat.
    /// Called outside the sensor/policy timing scopes; cache effects may remain.
    pub fn observe(&mut self, seat: usize, map: Option<&GroundMap>) {
        if self.maps.len() == SNAPSHOTS {
            return;
        }
        let Some(map) = map.filter(|m| !m.nodes.is_empty()) else {
            return;
        };
        if self.last_tick[seat].is_some_and(|tick| map.tick < tick + 120) {
            return;
        }
        self.last_tick[seat] = Some(map.tick);
        self.maps.push(Arc::new(map.clone()));
    }

    pub fn finish(self, out: &Path) {
        let mut trace = BufWriter::new(fs::File::create(out.join("planning-budget.csv")).unwrap());
        writeln!(
            trace,
            "jobs,quota,tick,global_graph,global_queries,actor,graph,queries,age,phase"
        )
        .unwrap();
        let mut runs = Vec::new();
        if !self.maps.is_empty() {
            for jobs in [1, 2, 6] {
                for quota in [16, 64, 256, 1024] {
                    let mut queue = PlanningQueue::new(jobs);
                    let mut expected = Vec::new();
                    let mut work = Vec::new();
                    let mut tokens = Vec::new();
                    let mut setup_ms = Vec::new();
                    for actor in 0..jobs {
                        let map = Arc::clone(&self.maps[actor % self.maps.len()]);
                        // A repeatable graph workload, not an executable sortie:
                        // start/hatch at the first measured footing; target at
                        // the middle footing, using a four-unit actor range.
                        let start = map.nodes[0].position;
                        let target = map.nodes[map.nodes.len() / 2].position;
                        let reference = map
                            .routes()
                            .round_trip_to_actor_target(start, target, 4.0, start);
                        let clock = Instant::now();
                        let job = GroundRoundTripJob::new(map, start, target, 4.0, start);
                        setup_ms.push(clock.elapsed().as_secs_f64() * 1000.0);
                        let mut drained = job.clone();
                        while drained.next_work().is_some() {
                            drained.step();
                        }
                        assert_eq!(drained.output(), Some(&reference));
                        work.push(drained.work());
                        expected.push(reference);
                        // Arbitrary scheduler identities, not real extra players.
                        tokens.push(
                            queue
                                .submit(100 + actor as u64 * 7, (), JobLimits::default(), job)
                                .unwrap(),
                        );
                    }
                    let mut ready_ticks = vec![None; jobs];
                    let mut charged = vec![0_u64; jobs];
                    let mut dispatch_ms = Vec::new();
                    let mut ticks = 0;
                    // One unit would finish within total_work steps. A guard
                    // catches lost work/starvation without using a time cutoff.
                    let total_work: u64 = work.iter().map(|w| w.operations).sum();
                    while ready_ticks.iter().any(Option::is_none) {
                        ticks += 1;
                        assert!(ticks <= total_work);
                        let clock = Instant::now();
                        let report = queue.advance(Work {
                            graph: quota,
                            physics_queries: 0,
                        });
                        dispatch_ms.push(clock.elapsed().as_secs_f64() * 1000.0);
                        assert!(report.charged.graph <= quota);
                        assert_eq!(report.charged.physics_queries, 0);
                        assert_eq!(
                            report.jobs.iter().map(|j| j.charged.graph).sum::<u32>(),
                            report.charged.graph
                        );
                        for (actor, allocation) in report.jobs.iter().enumerate() {
                            charged[actor] += u64::from(allocation.charged.graph);
                            writeln!(
                                trace,
                                "{jobs},{quota},{ticks},{},{},{},{},{},{},{:?}",
                                report.charged.graph,
                                report.charged.physics_queries,
                                allocation.request.actor,
                                allocation.charged.graph,
                                allocation.charged.physics_queries,
                                allocation.age_ticks,
                                allocation.phase
                            )
                            .unwrap();
                            if ready_ticks[actor].is_none() && allocation.phase == JobPhase::Ready {
                                assert_eq!(
                                    queue.poll(tokens[actor], &()),
                                    JobPoll::Ready(&expected[actor])
                                );
                                assert_eq!(charged[actor], work[actor].operations);
                                ready_ticks[actor] = Some(ticks);
                            }
                        }
                    }
                    let elapsed_at_60_hz: Vec<_> = ready_ticks
                        .iter()
                        .map(|t| t.unwrap() as f64 / 60.0)
                        .collect();
                    runs.push(json!({"jobs":jobs, "global_graph_per_tick":quota,
                        "global_queries_per_tick":0, "limits":JobLimits::default(),
                        "ticks":ticks,"ready_ticks":ready_ticks,"seconds_if_dispatched_at_60_hz":elapsed_at_60_hz,
                        "charged_graph":charged,"work":work,"setup":super::timing(setup_ms),
                        "dispatch":super::timing(dispatch_ms),"equivalent_to_synchronous":true,
                        "results":expected.iter().map(|r|json!({"endpoint":r.endpoint.map(|n|n.id),
                            "failure":r.outbound.diagnostics.failure})).collect::<Vec<_>>()}));
                    queue.reset();
                }
            }
        }
        trace.flush().unwrap();
        let maps: Vec<_> = self
            .maps
            .iter()
            .map(|map| {
                json!({"actor":map.actor,
            "planet":map.planet,"revision":map.revision,"tick":map.tick,"nodes":map.nodes.len(),
            "edges":map.edges.len(),"rejected":map.rejected.len()})
            })
            .collect();
        fs::write(out.join("planning-budget.json"), serde_json::to_vec_pretty(&json!({
            "version":1,"scope":"offline frozen-map graph jobs; live bots still plan synchronously",
            "status":if maps.is_empty(){"no_measured_ground_maps"}else{"complete"},
            "snapshot_limit":SNAPSHOTS,"snapshots":maps,"runs":runs,
            "query":"start/hatch at first footing, target at middle footing, range 4",
            "timing_scope":"queue.advance only; construction measured separately; excludes snapshot copies, polling, reference solves, CSV IO, world surveys and live control",
            "memory_scope":"at most six shared maps and six retained graph jobs; capacity limits job count, not bytes; ready workspaces remain until reset",
            "policy_work_quota":"unchanged: null (synchronous); this is not an equal-budget bot match"
        })).unwrap()).unwrap();
    }
}
