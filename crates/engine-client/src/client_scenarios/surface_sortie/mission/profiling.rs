//! Coarse per-update wall timings, separate from the host's displayed-frame
//! window. No nested query clocks or extra physical queries in normal builds.
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
            "mission_profile_version=2\nmission_profile_samples={}\nmission_profile_first_tick={}\nmission_profile_last_tick={}",
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
            ("total", |s: &Sample| s.total),
            ("world_physics", |s: &Sample| s.world.physics_time),
            ("world_lifecycle", |s: &Sample| s.world.lifecycle_time),
            ("world_workload", |s: &Sample| s.world.workload_time),
            ("world_gravity", |s: &Sample| s.world.gravity_time),
            ("world_collision", |s: &Sample| s.world.collision_time),
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
            let mut values = [Duration::ZERO; LIMIT];
            for (value, sample) in values.iter_mut().zip(&self.samples) {
                *value = select(sample);
            }
            let values = &mut values[..self.samples.len()];
            values.sort_unstable();
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            let p95 = (values.len() * 95).div_ceil(100) - 1;
            let _ = write!(
                text,
                "\nmission_{name}_avg_ms={:.3}\nmission_{name}_p95_ms={:.3}\nmission_{name}_max_ms={:.3}",
                values.iter().map(|d| ms(*d)).sum::<f64>() / values.len() as f64,
                ms(values[p95]),
                ms(*values.last().unwrap())
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
