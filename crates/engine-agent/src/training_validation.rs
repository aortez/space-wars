//! Fixed monitoring cases, excluded from selection and checkpoint RNG streams.
use crate::{
    evolution::EvolutionError,
    ship_arrival::{self, ArrivalCase, ArrivalOutcome},
    ship_curriculum::{self, Partition},
};
use serde::{Deserialize, Serialize};
use spacewars_ai::{guidance::Policy, model::DenseModel};

pub const SUITE: &str = "arrival_monitor_v1";

/// Previously inspected experiment cases, held out from the managed training
/// recipes. These are ongoing validation, not a new untouched qualification set.
pub fn cases() -> Vec<ArrivalCase> {
    [
        Partition::FreshStationary,
        Partition::FreshMoving,
        Partition::FreshInward,
    ]
    .into_iter()
    .flat_map(|p| ship_curriculum::cases(p).into_iter().take(2))
    .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseResult {
    pub case: ArrivalCase,
    pub outcome: ArrivalOutcome,
    pub elapsed_ticks: u32,
    pub distance: f32,
    pub relative_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResult {
    pub suite: String,
    pub generation: usize,
    pub genome_id: String,
    pub cases: Vec<CaseResult>,
}
impl ValidationResult {
    pub fn arrivals(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| c.outcome == ArrivalOutcome::Arrived)
            .count()
    }
    pub(crate) fn check(&self) -> Result<(), EvolutionError> {
        let expected = cases();
        if self.suite != SUITE
            || self.cases.len() != expected.len()
            || self.cases.iter().zip(expected).any(|(r, c)| {
                r.case != c
                    || r.elapsed_ticks > c.max_ticks
                    || !r.distance.is_finite()
                    || r.distance < 0.0
                    || !r.relative_speed.is_finite()
                    || r.relative_speed < 0.0
            })
        {
            return Err(EvolutionError(
                "saved validation suite or measurements differ".into(),
            ));
        }
        Ok(())
    }
}
pub fn evaluate(model: &DenseModel, generation: usize) -> Result<ValidationResult, EvolutionError> {
    let mut results = Vec::new();
    for case in cases() {
        let e = ship_arrival::run_episode(case, Policy::Model(model.clone()), false)?;
        results.push(CaseResult {
            case: e.case,
            outcome: e.outcome,
            elapsed_ticks: e.elapsed_ticks,
            distance: e.measure.distance,
            relative_speed: e.measure.relative_speed,
        });
    }
    Ok(ValidationResult {
        suite: SUITE.into(),
        generation,
        genome_id: model.id().into(),
        cases: results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitoring_cases_are_bounded_and_physically_separate_from_training() {
        let mut training = crate::ship_training::cases(crate::ship_training::Partition::Training);
        training.extend(ship_curriculum::cases(Partition::Training));
        let validation = cases();
        assert_eq!(validation.len(), 6);
        for v in validation {
            assert!(v.max_ticks <= 1200);
            for t in &training {
                let mut t = t.clone();
                t.name.clone_from(&v.name);
                assert_ne!(v, t, "validation must not repeat a physical training case");
            }
        }
    }
}
