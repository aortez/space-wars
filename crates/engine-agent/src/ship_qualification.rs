//! Frozen follow-up evaluation of the seed-3 arrival genome. No learning or
//! finalist selection occurs here, and baseline failures never remove cases.

use crate::{
    evolution::{EvaluationTask, EvolutionError, runtime_id},
    ship_arrival::{self, ArrivalCase, ArrivalEpisode, ArrivalOutcome, ArrivalTask},
    ship_training::{self, Partition},
};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::pilot::MaterialFlightStart;
use serde::Serialize;
use spacewars_ai::{guidance::Policy, model::DenseModel};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

pub const SUITE: &str = "material_arrival_qualification_v1";
pub const CANDIDATE: &str = "6c0ef53fb27e190243c84903855544a2cb9b97f4f32c92499dd37e541bbc41fb";
pub const FRESH_CASES: usize = 96;
pub const REGRESSION_CASES: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Geometry,
    InitialMotion,
    TargetMotion,
    Combined,
    Regression,
}

impl Group {
    fn name(self) -> &'static str {
        match self {
            Self::Geometry => "geometry",
            Self::InitialMotion => "initial_motion",
            Self::TargetMotion => "target_motion",
            Self::Combined => "combined",
            Self::Regression => "regression",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScheduledCase {
    pub group: Group,
    pub case: ArrivalCase,
}

/// A deterministic grid, declared before evaluating either controller. The
/// integer indexes are case names, not random seeds or sampled world variants.
pub fn cases() -> Vec<ScheduledCase> {
    let mut scheduled = Vec::new();
    for (bearing_index, bearing) in [0.0, FRAC_PI_2, PI, -FRAC_PI_2].into_iter().enumerate() {
        for group in [Group::Geometry, Group::InitialMotion, Group::TargetMotion] {
            // Offset, heading, initial (lateral, radial) speed, target velocity.
            let profiles = match group {
                Group::Geometry => [
                    ((0.0, 8.0), 0.0, (0.0, 0.0), (0.0, 0.0)),
                    ((0.0, 35.0), 0.0, (0.0, 0.0), (0.0, 0.0)),
                    ((-30.0, 8.0), -0.6, (0.0, 0.0), (0.0, 0.0)),
                    ((30.0, 8.0), 0.6, (0.0, 0.0), (0.0, 0.0)),
                    ((0.0, -12.0), 0.0, (0.0, 0.0), (0.0, 0.0)),
                    ((10.0, 18.0), PI, (0.0, 0.0), (0.0, 0.0)),
                ],
                Group::InitialMotion => [
                    ((18.0, 12.0), 0.25, (0.0, 6.0), (0.0, 0.0)),
                    ((-18.0, 12.0), -0.25, (0.0, -6.0), (0.0, 0.0)),
                    ((-18.0, 12.0), 0.25, (6.0, 0.0), (0.0, 0.0)),
                    ((18.0, 12.0), -0.25, (-6.0, 0.0), (0.0, 0.0)),
                    ((18.0, 12.0), 0.75, (-4.0, 4.0), (0.0, 0.0)),
                    ((-18.0, 12.0), -0.75, (4.0, -4.0), (0.0, 0.0)),
                ],
                Group::TargetMotion => [
                    ((18.0, 8.0), 0.3, (0.0, 0.0), (3.0, 0.0)),
                    ((-18.0, 8.0), -0.3, (0.0, 0.0), (-3.0, 0.0)),
                    ((0.0, 18.0), 0.0, (0.0, 0.0), (0.0, 1.5)),
                    ((0.0, -8.0), 0.0, (0.0, 0.0), (0.0, -1.5)),
                    ((15.0, 10.0), 0.5, (0.0, 0.0), (2.0, 1.0)),
                    ((-15.0, 10.0), -0.5, (0.0, 0.0), (-2.0, -1.0)),
                ],
                _ => unreachable!(),
            };
            for (profile, (offset, heading, speed, target_velocity)) in
                profiles.into_iter().enumerate()
            {
                scheduled.push(ScheduledCase {
                    group,
                    case: ArrivalCase {
                        name: format!("qualification_{}_b{bearing_index}_p{profile}", group.name()),
                        seed: 42,
                        start: MaterialFlightStart {
                            bearing,
                            altitude: 90.0,
                            radial_speed: speed.1,
                            lateral_speed: speed.0,
                            heading_offset: heading,
                        },
                        target_offset: Vec2::new(offset.0, offset.1),
                        target_velocity: Vec2::new(target_velocity.0, target_velocity.1),
                        max_ticks: 1200,
                    },
                });
            }
        }
    }
    for (altitude_index, altitude) in [70.0, 100.0, 125.0].into_iter().enumerate() {
        for (bearing_index, bearing) in [FRAC_PI_4, -3.0 * FRAC_PI_4].into_iter().enumerate() {
            for (profile, (offset, heading, speed, target_velocity)) in [
                ((25.0, 8.0), -0.8, (-3.0, -3.0), (2.5, 0.0)),
                ((-25.0, 8.0), 0.8, (3.0, 3.0), (-2.5, 0.0)),
                ((0.0, -8.0), FRAC_PI_2, (3.0, 0.0), (0.0, -1.0)),
                ((0.0, 25.0), -FRAC_PI_2, (0.0, -3.0), (0.0, 2.0)),
            ]
            .into_iter()
            .enumerate()
            {
                scheduled.push(ScheduledCase {
                    group: Group::Combined,
                    case: ArrivalCase {
                        name: format!(
                            "qualification_combined_a{altitude_index}_b{bearing_index}_p{profile}"
                        ),
                        seed: 42,
                        start: MaterialFlightStart {
                            bearing,
                            altitude,
                            radial_speed: speed.1,
                            lateral_speed: speed.0,
                            heading_offset: heading,
                        },
                        target_offset: Vec2::new(offset.0, offset.1),
                        target_velocity: Vec2::new(target_velocity.0, target_velocity.1),
                        max_ticks: 1200,
                    },
                });
            }
        }
    }
    scheduled
}

pub fn regressions() -> Vec<ArrivalCase> {
    [
        Partition::Training,
        Partition::StationaryValidation,
        Partition::MovingTransfer,
    ]
    .into_iter()
    .flat_map(ship_training::cases)
    .collect()
}

pub fn candidate() -> Result<DenseModel, EvolutionError> {
    let model = DenseModel::from_json(include_bytes!(
        "../fixtures/ship-arrival-learning-v1/seed-3.json"
    ))
    .map_err(|e| EvolutionError(e.to_string()))?;
    if model.id() != CANDIDATE {
        return Err(EvolutionError("qualification candidate changed".into()));
    }
    Ok(model)
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub version: u32,
    pub id: &'static str,
    pub runtime: String,
    pub candidate_id: &'static str,
    pub reference_id: &'static str,
    pub parent_recipe: &'static str,
    pub evaluator_parameters: serde_json::Value,
    pub fresh_cases: Vec<ScheduledCase>,
    pub regression_cases: Vec<ArrivalCase>,
    pub required_fresh_arrivals: usize,
    pub required_regression_arrivals: usize,
    pub gate: &'static str,
}

pub fn manifest() -> Result<Manifest, EvolutionError> {
    let mut parameters = ArrivalTask.manifest().configuration;
    parameters
        .as_object_mut()
        .expect("fixed evaluator configuration")
        .remove("cases");
    Ok(Manifest {
        version: 1,
        id: SUITE,
        runtime: runtime_id()?,
        candidate_id: CANDIDATE,
        reference_id: "arrival_reference_v1",
        parent_recipe: ship_arrival::RECIPE,
        evaluator_parameters: parameters,
        fresh_cases: cases(),
        regression_cases: regressions(),
        required_fresh_arrivals: FRESH_CASES,
        required_regression_arrivals: REGRESSION_CASES,
        gate: "all_fresh_and_regression_cases_arrived_no_exclusions_v1",
    })
}

#[derive(Debug, Serialize)]
pub struct Pair {
    pub group: Group,
    pub candidate: ArrivalEpisode,
    pub reference: ArrivalEpisode,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub cases: usize,
    pub both_arrived: usize,
    pub candidate_only: usize,
    pub reference_only: usize,
    pub neither: usize,
}

impl Counts {
    fn record(&mut self, candidate: ArrivalOutcome, reference: ArrivalOutcome) {
        self.cases += 1;
        match (
            candidate == ArrivalOutcome::Arrived,
            reference == ArrivalOutcome::Arrived,
        ) {
            (true, true) => self.both_arrived += 1,
            (true, false) => self.candidate_only += 1,
            (false, true) => self.reference_only += 1,
            (false, false) => self.neither += 1,
        }
    }

    fn candidate_arrivals(&self) -> usize {
        self.both_arrived + self.candidate_only
    }
}

#[derive(Debug, Serialize)]
pub struct GroupResult {
    pub group: Group,
    pub counts: Counts,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub manifest: Manifest,
    pub passed: bool,
    pub fresh: Counts,
    pub regressions: Counts,
    pub groups: Vec<GroupResult>,
    pub episodes: Vec<Pair>,
}

fn passes(fresh: &Counts, regressions: &Counts) -> bool {
    fresh.cases == FRESH_CASES
        && fresh.candidate_arrivals() == FRESH_CASES
        && regressions.cases == REGRESSION_CASES
        && regressions.candidate_arrivals() == REGRESSION_CASES
}

pub fn run() -> Result<Report, EvolutionError> {
    let manifest = manifest()?;
    let model = candidate()?;
    let mut episodes = Vec::new();
    let mut fresh = Counts::default();
    let mut regressions = Counts::default();
    for scheduled in
        manifest
            .fresh_cases
            .iter()
            .cloned()
            .chain(
                manifest
                    .regression_cases
                    .iter()
                    .cloned()
                    .map(|case| ScheduledCase {
                        group: Group::Regression,
                        case,
                    }),
            )
    {
        let candidate =
            ship_arrival::run_episode(scheduled.case.clone(), Policy::Model(model.clone()), false)?;
        let reference = ship_arrival::run_episode(scheduled.case, Policy::Reference, false)?;
        let counts = if scheduled.group == Group::Regression {
            &mut regressions
        } else {
            &mut fresh
        };
        counts.record(candidate.outcome, reference.outcome);
        episodes.push(Pair {
            group: scheduled.group,
            candidate,
            reference,
        });
    }
    let groups = [
        Group::Geometry,
        Group::InitialMotion,
        Group::TargetMotion,
        Group::Combined,
        Group::Regression,
    ]
    .into_iter()
    .map(|group| {
        let mut counts = Counts::default();
        for pair in episodes.iter().filter(|p| p.group == group) {
            counts.record(pair.candidate.outcome, pair.reference.outcome);
        }
        GroupResult { group, counts }
    })
    .collect();
    Ok(Report {
        manifest,
        passed: passes(&fresh, &regressions),
        fresh,
        regressions,
        groups,
        episodes,
    })
}

#[cfg(test)]
mod tests;
