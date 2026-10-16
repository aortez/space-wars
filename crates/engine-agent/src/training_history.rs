//! Bounded, read-only discovery and inspection of retained training evidence.
//! This module has no worker-control handle and never repairs or deletes files.
use crate::{
    brain_assembly::Artifact,
    evolution::{self, EvolutionConfig, EvolutionError, TaskManifest},
    genome_library,
    ship_arrival::{ArrivalCase, ArrivalFitness, ArrivalOutcome, PracticeBrain},
    training_job::{GenerationProgress, Recipe},
    training_validation::ValidationResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MAX_RECORDS: usize = 2048;
pub const MAX_DIRECTORY_ENTRIES: usize = 8192;
const MAX_RECEIPT_BYTES: usize = 256 * 1024;

fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Kind {
    Run,
    GenomeTrial,
    AssemblyTrial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Key {
    pub kind: Kind,
    pub id: String,
    pub receipt: String,
}
impl Key {
    fn path(&self, root: &Path) -> Result<PathBuf, EvolutionError> {
        match self.kind {
            Kind::Run if valid_run_id(&self.id) && self.receipt.is_empty() => {
                Ok(root.join(format!("{}.json", self.id)))
            }
            Kind::GenomeTrial | Kind::AssemblyTrial
                if genome_library::valid_id(&self.id)
                    && genome_library::valid_id(&self.receipt) =>
            {
                let directory = root.join(if self.kind == Kind::GenomeTrial {
                    "trials"
                } else {
                    "assembly-trials"
                });
                regular_directory(&directory)?;
                regular_directory(&directory.join(&self.id))?;
                Ok(directory
                    .join(&self.id)
                    .join(format!("{}.json", self.receipt)))
            }
            _ => Err(error("invalid history record identity")),
        }
    }
}
fn valid_run_id(id: &str) -> bool {
    id.len() > 4
        && id.len() < 100
        && id.starts_with("run-")
        && id[4..].bytes().all(|c| c.is_ascii_digit() || c == b'-')
}
fn regular_directory(path: &Path) -> Result<(), EvolutionError> {
    if !path.symlink_metadata().map_err(error)?.file_type().is_dir() {
        return Err(error("history directories must not be symbolic links"));
    }
    Ok(())
}
pub(crate) fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, EvolutionError> {
    if !path
        .symlink_metadata()
        .map_err(error)?
        .file_type()
        .is_file()
    {
        return Err(error("history records must be regular files"));
    }
    evolution::read_bounded(path, limit)
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub keys: Vec<Key>,
    pub warnings: Vec<String>,
    pub truncated: bool,
}

/// Read names only; decoding happens for one explicitly selected record.
/// A corrupt checkpoint cannot monopolize a catalog scan or hide its siblings.
pub fn catalog(root: &Path, trials: bool) -> Result<Catalog, EvolutionError> {
    let mut result = Catalog::default();
    let mut scanned = 0;
    let mut directories = if trials {
        vec![
            (
                root.join("assembly-trials"),
                Kind::AssemblyTrial,
                String::new(),
            ),
            (root.join("trials"), Kind::GenomeTrial, String::new()),
        ]
    } else {
        vec![(root.to_owned(), Kind::Run, String::new())]
    };
    while let Some((directory, kind, parent)) = directories.pop() {
        match directory.try_exists() {
            Ok(false) => continue,
            Ok(true) => {}
            Err(e) => {
                result
                    .warnings
                    .push(format!("Cannot inspect {}: {e}", directory.display()));
                continue;
            }
        }
        if let Err(e) = regular_directory(&directory) {
            result.warnings.push(e.0);
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) => {
                result
                    .warnings
                    .push(format!("Cannot list {}: {e}", directory.display()));
                continue;
            }
        };
        for entry in entries {
            if scanned == MAX_DIRECTORY_ENTRIES || result.keys.len() == MAX_RECORDS {
                result.truncated = true;
                break;
            }
            scanned += 1;
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    result.warnings.push(e.to_string());
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind != Kind::Run && parent.is_empty() {
                if genome_library::valid_id(&name) {
                    match entry.file_type() {
                        Ok(t) if t.is_dir() => directories.push((entry.path(), kind, name)),
                        Ok(_) => {}
                        Err(e) => result.warnings.push(e.to_string()),
                    }
                }
                continue;
            }
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if kind == Kind::Run && valid_run_id(id) {
                result.keys.push(Key {
                    kind,
                    id: id.into(),
                    receipt: String::new(),
                });
            } else if kind != Kind::Run && genome_library::valid_id(id) {
                result.keys.push(Key {
                    kind,
                    id: parent.clone(),
                    receipt: id.into(),
                });
            }
        }
        if result.truncated {
            break;
        }
    }
    result
        .keys
        .sort_by(|a, b| b.id.cmp(&a.id).then(a.receipt.cmp(&b.receipt)));
    if result.truncated {
        result.warnings.insert(0, format!("Listing truncated at {MAX_RECORDS} records or {MAX_DIRECTORY_ENTRIES} directory entries; files are preserved."));
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
pub struct Winner {
    pub generation: usize,
    pub genome_id: String,
    pub arrivals: u32,
    pub fitness: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: String,
    pub recipe: Recipe,
    pub selected_initial_genomes: bool,
    pub runtime: String,
    pub config: EvolutionConfig,
    pub task: TaskManifest,
    pub initial_genomes: Vec<String>,
    pub complete: bool,
    pub evaluated_candidates: usize,
    pub evaluation_millis: u64,
    pub progress: Vec<GenerationProgress>,
    pub winners: Vec<Winner>,
    pub validation: Vec<ValidationResult>,
    /// A historical build/recipe can be inspected without being resumable.
    pub compatibility: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Measurements {
    pub distance: f32,
    pub relative_speed: f32,
    pub dwell: u32,
}

/// The compact evidence fields in the version-one episode. Rendering traces
/// and physical observations are not retained by the history browser.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrialRecord {
    pub version: u32,
    pub runtime: String,
    pub recipe: String,
    pub case: ArrivalCase,
    pub brain: PracticeBrain,
    pub brain_id: String,
    pub elapsed_ticks: u32,
    pub outcome: ArrivalOutcome,
    pub measure: Measurements,
    pub fitness: ArrivalFitness,
    #[serde(default)]
    pub assembly: Option<Artifact>,
}
#[derive(Debug, Clone, Serialize)]
pub enum Record {
    Run(Box<RunRecord>),
    Trial(Box<TrialRecord>),
}

pub(crate) fn valid_measurements(
    case: &ArrivalCase,
    ticks: u32,
    distance: f32,
    speed: f32,
) -> bool {
    case.max_ticks > 0
        && ticks <= case.max_ticks
        && distance.is_finite()
        && distance >= 0.0
        && speed.is_finite()
        && speed >= 0.0
}

pub fn inspect(root: &Path, key: &Key) -> Result<Record, EvolutionError> {
    let path = key.path(root)?;
    if key.kind == Kind::Run {
        return crate::training_job::history::inspect(root, &key.id)
            .map(|r| Record::Run(Box::new(r)));
    }
    let bytes = read_regular(&path, MAX_RECEIPT_BYTES)?;
    if hash(&bytes) != key.receipt {
        return Err(error("practice receipt checksum differs"));
    }
    let mut trial: TrialRecord = if key.kind == Kind::AssemblyTrial {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Receipt {
            version: u32,
            assembly: Artifact,
            episode: TrialRecord,
        }
        let receipt: Receipt = serde_json::from_slice(&bytes).map_err(error)?;
        if receipt.version != 1
            || receipt.assembly.id != key.id
            || Artifact::new(receipt.assembly.definition.clone())? != receipt.assembly
            || receipt.assembly.definition.brain != receipt.episode.brain
        {
            return Err(error("invalid assembly receipt identity"));
        }
        let mut episode = receipt.episode;
        episode.assembly = Some(receipt.assembly);
        episode
    } else {
        serde_json::from_slice(&bytes).map_err(error)?
    };
    if key.kind == Kind::GenomeTrial {
        trial.assembly = None;
        if trial.brain.controller != key.id {
            return Err(error("receipt genome identity differs"));
        }
    }
    if trial.version != 1
        || trial.brain_id != trial.brain.identity()?
        || !genome_library::valid_id(&trial.brain.controller)
        || trial.runtime.is_empty()
        || trial.recipe.is_empty()
        || !valid_measurements(
            &trial.case,
            trial.elapsed_ticks,
            trial.measure.distance,
            trial.measure.relative_speed,
        )
    {
        return Err(error(
            "invalid practice receipt version, identity or measurements",
        ));
    }
    Ok(Record::Trial(Box::new(trial)))
}

/// Exact protocol identity, independent of the candidate being measured. A
/// matching protocol permits side-by-side display, never pooled evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Protocol {
    pub domain: String,
    pub runtime: String,
    pub definition: String,
}
impl Protocol {
    pub fn new(
        domain: &str,
        runtime: &str,
        definition: &impl Serialize,
    ) -> Result<Self, EvolutionError> {
        Ok(Self {
            domain: domain.into(),
            runtime: runtime.into(),
            definition: hash(&serde_json::to_vec(definition).map_err(error)?),
        })
    }
    pub fn mismatch(&self, other: &Self) -> Option<&'static str> {
        if self.domain != other.domain {
            Some("Different result types")
        } else if self.runtime != other.runtime {
            Some("Different builds / runtimes")
        } else if self.definition != other.definition {
            Some("Different cases, curricula or scoring protocols")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
