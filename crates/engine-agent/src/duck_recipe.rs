//! Arrival on the Clock's Platforms course. Every policy uses the same visit and scorer.
use crate::{evolution::EvolutionError, training_workers::EpisodePool};
use engine_common::{
    ClockDuckCoursePattern, ClockDuckOutcome,
    brain_controls::{BodyProfile, CreatureControls, CreatureDecision},
};
pub use scenario_clock::duck_training::{DuckCase, DuckControls};
use scenario_clock::duck_training::{DuckDriver, DuckEpisode, DuckProgress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    c3::{C3Genome, DECISION_TICKS},
    evolution_draws::EvolutionDraws,
};
use std::{iter::Sum, ops::Add};

pub const RECIPE: &str = "clock_duck_arrival_v1";
pub const SCORE: &str = "exit_wall_tags_clears_no_fall_time_v1";
pub const PATTERNS: [ClockDuckCoursePattern; 4] = [
    ClockDuckCoursePattern::Platforms,
    ClockDuckCoursePattern::Terraces,
    ClockDuckCoursePattern::TwoJump,
    ClockDuckCoursePattern::Shortcut,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub exits: u64,
    pub wall_tags: u64,
    pub clears: u64,
    pub no_fall: u64,
    pub ticks: u64,
}

impl Score {
    fn from_terminal(progress: DuckProgress) -> Self {
        Self {
            exits: u64::from(progress.outcome == Some(ClockDuckOutcome::Exited)),
            wall_tags: progress.wall_tags.into_iter().map(u64::from).sum(),
            clears: u64::from(progress.cleared),
            no_fall: u64::from(progress.outcome != Some(ClockDuckOutcome::Fell)),
            ticks: progress.tick,
        }
    }

    pub fn terms(self) -> [u64; 5] {
        [
            self.exits,
            self.wall_tags,
            self.clears,
            self.no_fall,
            self.ticks,
        ]
    }
}
impl Add for Score {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            exits: self.exits + rhs.exits,
            wall_tags: self.wall_tags + rhs.wall_tags,
            clears: self.clears + rhs.clears,
            no_fall: self.no_fall + rhs.no_fall,
            ticks: self.ticks + rhs.ticks,
        }
    }
}
impl Sum for Score {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), Self::add)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Validation,
    Qualification,
}

// Reserve two high bits so no training seed can become a held-out case, even
// across different run seeds or generations. Both doors share the same world.
fn case_seed(run: u64, generation: u64, index: usize, partition: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"spacewars.clock_duck_cases.v1\0");
    for term in [run, generation, index as u64, partition] {
        hash.update(term.to_le_bytes());
    }
    (u64::from_le_bytes(hash.finalize()[..8].try_into().unwrap()) >> 2) | (partition << 62)
}

pub fn training_cases(
    seed: u64,
    generation: u64,
    mix: &[ClockDuckCoursePattern],
    count: usize,
) -> Result<Vec<DuckCase>, EvolutionError> {
    if count == 0
        || mix.is_empty()
        || mix.len() > PATTERNS.len()
        || mix.iter().enumerate().any(|(i, p)| mix[..i].contains(p))
    {
        return Err(EvolutionError(
            "Choose at least one training case and distinct course patterns".into(),
        ));
    }
    // Give both entrance directions equal weight in an even-sized generation.
    // A seeded first side also varies single-case training.
    let first_right = case_seed(seed, generation, 0, 0) & (1 << 31) != 0;
    Ok((0..count)
        .map(|i| {
            let mut case = DuckCase::seeded(case_seed(seed, generation, i, 0));
            case.pattern = mix[(case.seed % mix.len() as u64) as usize];
            case.entrance_right = first_right ^ (i % 2 == 1);
            case
        })
        .collect())
}

pub fn held_out_cases(seed: u64, partition: Partition, count: usize) -> Vec<[DuckCase; 2]> {
    held_out_cases_matching(seed, partition, count, None)
}

fn held_out_cases_matching(
    seed: u64,
    partition: Partition,
    count: usize,
    pattern: Option<ClockDuckCoursePattern>,
) -> Vec<[DuckCase; 2]> {
    let tag = match partition {
        Partition::Validation => 1,
        Partition::Qualification => 2,
    };
    (0..)
        .map(|i| DuckCase::seeded(case_seed(seed, 0, i, tag)))
        .filter(|case| pattern.is_none_or(|p| p == case.pattern))
        .take(count)
        .map(|mut case| {
            case.entrance_right = false;
            [
                case,
                DuckCase {
                    entrance_right: true,
                    ..case
                },
            ]
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reference {
    Scripted,
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visit {
    pub case: DuckCase,
    pub reference: Option<Reference>,
    pub progress: DuckProgress,
    pub score: Score,
    pub invalid_decisions: u32,
    pub decision_ticks: u32,
    /// Entry n applies before stepping tick n * decision_ticks.
    pub controls: Vec<DuckControls>,
}
impl Visit {
    pub fn control_at(&self, tick: u64) -> DuckControls {
        self.controls
            .get((tick / u64::from(self.decision_ticks)) as usize)
            .copied()
            .unwrap_or_default()
    }
    pub fn validate_replay(&self) -> Result<(), EvolutionError> {
        let terminal = self.progress.outcome.is_some()
            && (1..=scenario_clock::DUCK_TICKS).contains(&self.progress.tick);
        let expected = if self.reference == Some(Reference::Scripted) {
            0
        } else {
            self.progress.tick.div_ceil(u64::from(DECISION_TICKS)) as usize
        };
        if !terminal
            || self.decision_ticks != DECISION_TICKS
            || self.controls.len() != expected
            || self.score != Score::from_terminal(self.progress)
            || self
                .controls
                .iter()
                .any(|c| !(-1000..=1000).contains(&c.move_milli))
        {
            return Err(EvolutionError("Invalid Clock duck replay".into()));
        }
        Ok(())
    }
    pub fn driver(&self) -> DuckDriver {
        if self.reference == Some(Reference::Scripted) {
            DuckDriver::Scripted
        } else {
            DuckDriver::External
        }
    }
}

pub fn run_model(case: DuckCase, model: &C3Genome, capture: bool) -> Result<Visit, EvolutionError> {
    let body = BodyProfile::duck();
    model.binding().bind(&body).map_err(EvolutionError)?;
    let episode =
        DuckEpisode::new(case, DuckDriver::External).map_err(|e| EvolutionError(e.into()))?;
    let sensor = episode.sensor().map_err(|e| EvolutionError(e.into()))?;
    run(episode, case, None, capture, |episode| {
        let observation = episode
            .observation(&sensor)
            .map_err(|e| EvolutionError(e.into()))?;
        let output = model
            .infer(&observation)
            .map_err(|e| EvolutionError(e.to_string()))?;
        Ok(CreatureControls::decode(&output, &body))
    })
}

pub fn run_reference(
    case: DuckCase,
    policy: Reference,
    capture: bool,
) -> Result<Visit, EvolutionError> {
    let driver = match policy {
        Reference::Scripted => DuckDriver::Scripted,
        Reference::Random => DuckDriver::External,
    };
    let episode = DuckEpisode::new(case, driver).map_err(|e| EvolutionError(e.into()))?;
    let mut draws = EvolutionDraws::new(
        case.seed,
        0,
        usize::from(case.entrance_right),
        b"clock_duck_random_controls_v1",
    );
    run(episode, case, Some(policy), capture, |_| {
        let output: [f32; 4] = std::array::from_fn(|_| draws.signed());
        Ok(CreatureControls::decode(&output, &BodyProfile::duck()))
    })
}

fn run(
    mut episode: DuckEpisode,
    case: DuckCase,
    reference: Option<Reference>,
    capture: bool,
    mut decide: impl FnMut(&DuckEpisode) -> Result<CreatureDecision, EvolutionError>,
) -> Result<Visit, EvolutionError> {
    let mut controls = DuckControls::default();
    let mut trace = Vec::new();
    let mut invalid_decisions = 0;
    let progress = loop {
        if episode
            .progress()
            .tick
            .is_multiple_of(u64::from(DECISION_TICKS))
            && reference != Some(Reference::Scripted)
        {
            if episode.controls_active() {
                let decision = decide(&episode)?;
                controls = decision.controls;
                invalid_decisions += u32::from(decision.invalid);
            }
            if capture {
                trace.push(controls);
            }
        }
        let progress = episode.step(controls);
        if progress.outcome.is_some() {
            break progress;
        }
    };
    Ok(Visit {
        case,
        reference,
        progress,
        score: Score::from_terminal(progress),
        invalid_decisions,
        decision_ticks: DECISION_TICKS,
        controls: trace,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedScores {
    pub left: Visit,
    pub right: Visit,
    pub worse: Score,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteResult {
    pub exits: u64,
    pub episodes: usize,
    pub both_doors_exited: usize,
    pub score: Score,
    pub pairs: Vec<PairedScores>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldOutReport {
    pub recipe: String,
    pub scoring: String,
    pub decision_ticks: u32,
    pub partition: Partition,
    pub pattern: Option<ClockDuckCoursePattern>,
    pub seed: u64,
    pub genome: String,
    pub candidate: SuiteResult,
    pub scripted: SuiteResult,
    pub random: SuiteResult,
}

fn suite(
    cases: &[[DuckCase; 2]],
    pool: Option<&EpisodePool>,
    evaluate: impl Fn(DuckCase) -> Result<Visit, EvolutionError> + Sync,
) -> Result<SuiteResult, EvolutionError> {
    let cases: Vec<_> = cases.iter().flatten().copied().collect();
    let visits: Vec<_> = match pool {
        Some(pool) => pool.map(&cases, |case| evaluate(*case)),
        None => cases.into_iter().map(evaluate).collect(),
    };
    let mut visits = visits.into_iter();
    let mut pairs = Vec::new();
    while let Some(left) = visits.next() {
        let left = left?;
        let right = visits.next().expect("paired cases")?;
        let worse = left.score.min(right.score);
        pairs.push(PairedScores { left, right, worse });
    }
    Ok(SuiteResult {
        exits: pairs
            .iter()
            .map(|p| p.left.score.exits + p.right.score.exits)
            .sum(),
        episodes: pairs.len() * 2,
        both_doors_exited: pairs.iter().filter(|p| p.worse.exits == 1).count(),
        score: pairs.iter().map(|p| p.worse).sum(),
        pairs,
    })
}

pub fn evaluate_held_out(
    model: &C3Genome,
    seed: u64,
    partition: Partition,
    count: usize,
    pattern: Option<ClockDuckCoursePattern>,
    pool: Option<&EpisodePool>,
) -> Result<HeldOutReport, EvolutionError> {
    if !(1..=256).contains(&count) {
        return Err(EvolutionError(
            "Held-out case count must be 1–256 mirrored pairs".into(),
        ));
    }
    let cases = held_out_cases_matching(seed, partition, count, pattern);
    Ok(HeldOutReport {
        recipe: RECIPE.into(),
        scoring: SCORE.into(),
        decision_ticks: DECISION_TICKS,
        partition,
        pattern,
        seed,
        genome: model.id().into(),
        candidate: suite(&cases, pool, |case| run_model(case, model, false))?,
        scripted: suite(&cases, pool, |case| {
            run_reference(case, Reference::Scripted, false)
        })?,
        random: suite(&cases, pool, |case| {
            run_reference(case, Reference::Random, false)
        })?,
    })
}

#[cfg(test)]
mod tests;
