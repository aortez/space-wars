//! Optional read-only phase samples, collected outside the timed scenario step.
use scenario_spacewars::SpacewarsStepMetrics;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PhysicsProfile {
    times: BTreeMap<&'static str, Vec<f64>>,
    counts: BTreeMap<&'static str, Vec<usize>>,
    pair_samples: Vec<Value>,
}

impl PhysicsProfile {
    pub fn record_pairs(&mut self, tick: u64, same_body: usize, other: usize, active: usize) {
        self.pair_samples
            .push(json!({"tick": tick, "same_body_candidates": same_body,
            "other_candidates": other, "active_contact_pairs": active}));
    }

    pub fn record(&mut self, m: SpacewarsStepMetrics, step_ms: f64) {
        let r = m.rapier;
        let shared = m.motion_diagnostics_time
            + m.workload_time
            + m.lifecycle_time
            + m.gravity_time
            + m.collision_time
            + m.physics_time;
        for (name, time) in [
            ("world_workload", m.workload_time),
            ("world_lifecycle", m.lifecycle_time),
            ("world_gravity", m.gravity_time),
            ("world_collision_effects", m.collision_time),
            ("world_physics", m.physics_time),
            ("world_motion_diagnostics", m.motion_diagnostics_time),
            ("rapier_step", r.rapier_step_time),
            ("rapier_update", r.update_time),
            ("rapier_user_changes", r.user_changes_time),
            (
                "rapier_kinematic_interpolation",
                r.kinematic_interpolation_time,
            ),
            ("rapier_collision_detection", r.collision_detection_time),
            ("rapier_broad_phase", r.broad_phase_time),
            ("rapier_final_broad_phase", r.final_broad_phase_time),
            ("rapier_narrow_phase", r.narrow_phase_time),
            ("rapier_islands", r.island_time),
            ("rapier_island_constraints", r.island_constraints_time),
            ("rapier_solver", r.solver_time),
        ] {
            self.times
                .entry(name)
                .or_default()
                .push(time.as_secs_f64() * 1000.0);
        }
        self.times
            .entry("sortie_outside_world")
            .or_default()
            .push((step_ms - shared.as_secs_f64() * 1000.0).max(0.0));
        for (name, count) in [
            ("active_bodies", r.active_bodies),
            ("sleeping_bodies", r.sleeping_bodies),
            ("candidate_pairs", r.candidate_pairs),
            ("contact_pairs", r.contact_pairs),
            ("solver_contacts", r.contacts),
            ("entities_added", m.added),
            ("entities_removed", m.removed),
        ] {
            self.counts.entry(name).or_default().push(count);
        }
    }

    pub fn report(self) -> Value {
        let times: BTreeMap<_, _> = self
            .times
            .into_iter()
            .map(|(name, values)| (name, super::timing(values)))
            .collect();
        let counts: BTreeMap<_, _> = self
            .counts
            .into_iter()
            .map(|(name, mut values)| {
                values.sort_unstable();
                let n = values.len();
                let stats = if n == 0 {
                    Value::Null
                } else {
                    json!({"count": n, "mean": values.iter().sum::<usize>() as f64 / n as f64,
                    "p50": values[n / 2], "p95": values[(n * 95 / 100).min(n - 1)],
                    "max": values.last()})
                };
                (name, stats)
            })
            .collect();
        json!({"version": 1, "timings": times, "populations": counts,
            "pair_samples": self.pair_samples, "ccd": null,
            "scope": "world_* phases plus sortie_outside_world partition the scenario step; rapier_* are nested backend timers, not additive; world_physics times the raw step only; post-step event collection is in world_workload; CCD total is unavailable in Rapier 0.34"})
    }
}
