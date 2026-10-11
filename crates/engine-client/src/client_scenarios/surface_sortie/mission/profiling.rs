//! Per-update wall timings, separate from the host's displayed-frame window.
//! Rapier's existing timers are nested within world physics; reporting them
//! adds no clocks or physical queries to the simulation.
use scenario_spacewars::{
    ShipForm, SpacewarsStepMetrics,
    surface_sortie::{
        PilotLocation,
        mission::MissionObservationV1,
        pilot::{LandingSiteId, LandingSiteQuery},
    },
};
use spacewars_ai::mission_policy::MissionBot;
use std::{collections::VecDeque, fmt::Write, time::Duration};

const LIMIT: usize = 120;

#[derive(Clone, Copy)]
pub(super) struct Seat {
    requested: Option<LandingSiteId>,
    query: LandingSiteQuery,
    location: PilotLocation,
    form: ShipForm,
    planet: usize,
    sites: usize,
    recovery_sites: usize,
    ground_nodes: usize,
    objective: bool,
}
impl Seat {
    pub fn read(o: &MissionObservationV1, requested: Option<LandingSiteId>) -> Self {
        let r = &o.local.combat.recovery;
        let p = &r.flight.pilot;
        Self {
            requested,
            query: p.site_query,
            location: p.location,
            form: p.ship_form,
            planet: p.planet.index,
            sites: p.sites.len(),
            recovery_sites: r.sites.len(),
            ground_nodes: r.ground.as_ref().map_or(0, |map| map.nodes.len()),
            objective: o.local.landing_objective.is_some(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Sample {
    pub tick: u64,
    pub sensors: [Duration; 2],
    pub policies: [Duration; 2],
    pub scenario: Duration,
    pub planning: Duration,
    pub total: Duration,
    pub world: SpacewarsStepMetrics,
    pub seats: [Option<Seat>; 2],
}

#[derive(Default)]
pub(super) struct Profile {
    samples: VecDeque<Sample>,
}
impl Profile {
    pub fn record(&mut self, sample: Sample) {
        if self.samples.back().is_some_and(|s| s.tick >= sample.tick) {
            self.samples.clear();
        }
        if self.samples.len() == LIMIT {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn diagnostics(&self, pilots: &[MissionBot; 2]) -> String {
        let Some(latest) = self.samples.back() else {
            return String::new();
        };
        let mut text = format!(
            "mission_profile_version=3\nmission_profile_samples={}\nmission_profile_first_tick={}\nmission_profile_last_tick={}",
            self.samples.len(),
            self.samples.front().unwrap().tick,
            latest.tick
        );
        for (name, select) in [
            (
                "sensor_p1",
                (|s: &Sample| s.sensors[0]) as fn(&Sample) -> Duration,
            ),
            ("sensor_p2", |s: &Sample| s.sensors[1]),
            ("policy_p1", |s: &Sample| s.policies[0]),
            ("policy_p2", |s: &Sample| s.policies[1]),
            ("scenario", |s: &Sample| s.scenario),
            ("planning", |s: &Sample| s.planning),
            ("total", |s: &Sample| s.total),
            ("world_physics", |s: &Sample| s.world.physics_time),
            ("world_lifecycle", |s: &Sample| s.world.lifecycle_time),
            ("world_workload", |s: &Sample| s.world.workload_time),
            ("world_gravity", |s: &Sample| s.world.gravity_time),
            ("world_collision", |s: &Sample| s.world.collision_time),
            ("rapier_step", |s: &Sample| s.world.rapier.rapier_step_time),
            ("rapier_update", |s: &Sample| s.world.rapier.update_time),
            ("rapier_user_changes", |s: &Sample| {
                s.world.rapier.user_changes_time
            }),
            ("rapier_kinematic_interpolation", |s: &Sample| {
                s.world.rapier.kinematic_interpolation_time
            }),
            ("rapier_collision_detection", |s: &Sample| {
                s.world.rapier.collision_detection_time
            }),
            ("rapier_broad_phase", |s: &Sample| {
                s.world.rapier.broad_phase_time
            }),
            ("rapier_final_broad_phase", |s: &Sample| {
                s.world.rapier.final_broad_phase_time
            }),
            ("rapier_narrow_phase", |s: &Sample| {
                s.world.rapier.narrow_phase_time
            }),
            ("rapier_island", |s: &Sample| s.world.rapier.island_time),
            ("rapier_island_constraints", |s: &Sample| {
                s.world.rapier.island_constraints_time
            }),
            ("rapier_solver", |s: &Sample| s.world.rapier.solver_time),
            ("sortie_outside_world", |s: &Sample| {
                s.scenario.saturating_sub(
                    s.world.physics_time
                        + s.world.lifecycle_time
                        + s.world.workload_time
                        + s.world.gravity_time
                        + s.world.collision_time
                        + s.world.motion_diagnostics_time,
                )
            }),
        ] {
            append_timings(&mut text, name, self.samples.iter().map(select));
        }
        self.append_ccd_diagnostics(&mut text);
        // These are backend populations, not counts of CCD queries. Keep them
        // on the same tick window as the timers instead of rescanning the world.
        for (name, select) in [
            (
                "ccd_substeps",
                (|s: &Sample| s.world.rapier.ccd_substeps) as fn(&Sample) -> usize,
            ),
            ("active_bodies", |s: &Sample| s.world.rapier.active_bodies),
            ("sleeping_bodies", |s: &Sample| {
                s.world.rapier.sleeping_bodies
            }),
            ("candidate_pairs", |s: &Sample| {
                s.world.rapier.candidate_pairs
            }),
            ("contact_pairs", |s: &Sample| s.world.rapier.contact_pairs),
            ("contacts", |s: &Sample| s.world.rapier.contacts),
        ] {
            let _ = write!(
                text,
                "\nmission_rapier_{name}_avg={:.3}\nmission_rapier_{name}_max={}\nmission_rapier_{name}_latest={}",
                self.samples.iter().map(|s| select(s) as f64).sum::<f64>()
                    / self.samples.len() as f64,
                self.samples.iter().map(select).max().unwrap(),
                select(latest),
            );
        }
        for seat in 0..2 {
            let actor = seat + 1;
            let count = self
                .samples
                .iter()
                .filter(|s| s.seats[seat].is_some())
                .count();
            let _ = write!(text, "\nmission_p{actor}_bot_observations={count}");
            for (name, accepts) in [
                (
                    "full_survey_requests",
                    (|q| q == LandingSiteQuery::Survey) as fn(LandingSiteQuery) -> bool,
                ),
                ("deferred_surveys", LandingSiteQuery::is_deferred),
                ("selected_site_requests", |q| {
                    matches!(q, LandingSiteQuery::Selected(_))
                }),
            ] {
                let count = self
                    .samples
                    .iter()
                    .filter(|s| s.seats[seat].is_some_and(|s| accepts(s.query)))
                    .count();
                let _ = write!(text, "\nmission_p{actor}_{name}={count}");
            }
            let Some(s) = latest.seats[seat] else {
                continue;
            };
            let _ = write!(text, "\nmission_p{actor}_site_query={:?}", s.query);
            let _ = write!(
                text,
                "\nmission_p{actor}_location={:?}\nmission_p{actor}_form={:?}\nmission_p{actor}_planet={}\nmission_p{actor}_site_request={:?}\nmission_p{actor}_landing_sites={}\nmission_p{actor}_recovery_sites={}\nmission_p{actor}_ground_nodes={}\nmission_p{actor}_objective_survey={}\nmission_p{actor}_task={}",
                s.location,
                s.form,
                s.planet,
                s.requested,
                s.sites,
                s.recovery_sites,
                s.ground_nodes,
                s.objective,
                pilots[seat].label()
            );
        }
        text
    }

    fn append_ccd_diagnostics(&self, text: &mut String) {
        let count = append_timings(
            text,
            "rapier_ccd",
            self.samples.iter().filter_map(|s| s.world.rapier.ccd_time),
        );
        let _ = write!(text, "\nmission_rapier_ccd_samples={count}");
        // Missing backend timers are not zero-cost CCD. Total and nested timers
        // use only samples with a complete CCD timer, with an explicit count.
        for (name, select) in [
            (
                "rapier_ccd_first_impact",
                (|s: &Sample| s.world.rapier.ccd_first_impact_time) as fn(&Sample) -> Duration,
            ),
            ("rapier_ccd_clamping", |s: &Sample| {
                s.world.rapier.ccd_clamping_time
            }),
        ] {
            append_timings(
                text,
                name,
                self.samples
                    .iter()
                    .filter(|s| s.world.rapier.ccd_time.is_some())
                    .map(select),
            );
        }
    }
}

fn append_timings(text: &mut String, name: &str, samples: impl Iterator<Item = Duration>) -> usize {
    let mut values = [Duration::ZERO; LIMIT];
    let mut count = 0;
    for (value, sample) in values.iter_mut().zip(samples) {
        *value = sample;
        count += 1;
    }
    if count == 0 {
        return 0;
    }
    let values = &mut values[..count];
    values.sort_unstable();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let p95 = (count * 95).div_ceil(100) - 1;
    let _ = write!(
        text,
        "\nmission_{name}_avg_ms={:.3}\nmission_{name}_p95_ms={:.3}\nmission_{name}_max_ms={:.3}",
        values.iter().map(|d| ms(*d)).sum::<f64>() / count as f64,
        ms(values[p95]),
        ms(*values.last().unwrap())
    );
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_ccd_timers_are_distinct_from_zero_cost_samples() {
        let mut p = Profile::default();
        let mut sample = Sample::default();
        // A partial backend timer must not leak into the complete-timer window.
        sample.world.rapier.ccd_first_impact_time = Duration::from_secs(1);
        p.record(sample);
        let mut text = String::new();
        p.append_ccd_diagnostics(&mut text);
        assert_eq!(text, "\nmission_rapier_ccd_samples=0");

        sample.tick = 1;
        sample.world.rapier.ccd_time = Some(Duration::from_millis(8));
        sample.world.rapier.ccd_first_impact_time = Duration::from_millis(6);
        sample.world.rapier.ccd_clamping_time = Duration::from_millis(2);
        p.record(sample);
        sample.tick = 2;
        sample.world.rapier.ccd_time = Some(Duration::ZERO);
        sample.world.rapier.ccd_first_impact_time = Duration::ZERO;
        sample.world.rapier.ccd_clamping_time = Duration::ZERO;
        p.record(sample);
        text.clear();
        p.append_ccd_diagnostics(&mut text);
        for expected in [
            "mission_rapier_ccd_samples=2",
            "mission_rapier_ccd_avg_ms=4.000",
            "mission_rapier_ccd_p95_ms=8.000",
            "mission_rapier_ccd_max_ms=8.000",
            "mission_rapier_ccd_first_impact_avg_ms=3.000",
            "mission_rapier_ccd_clamping_avg_ms=1.000",
        ] {
            assert!(text.lines().any(|line| line == expected), "{text}");
        }
    }

    #[test]
    fn sample_window_is_bounded_and_does_not_cross_a_tick_reset() {
        let mut p = Profile::default();
        for tick in 0..=LIMIT as u64 {
            p.record(Sample {
                tick,
                ..Default::default()
            });
        }
        assert_eq!(p.samples.len(), LIMIT);
        assert_eq!(p.samples.front().unwrap().tick, 1);
        p.record(Sample {
            tick: 0,
            ..Default::default()
        });
        assert_eq!(p.samples.len(), 1);
    }
}
