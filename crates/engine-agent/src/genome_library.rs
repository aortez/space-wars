//! Immutable genome artifacts and bounded practice trials, independent of jobs.
use crate::{
    evolution::{EvolutionError, atomic_write, read_bounded},
    ship_arrival::{self, ArrivalEpisode},
    ship_qualification,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    guidance::{self, Policy},
    model::{DenseGenome, DenseModel, MAX_MODEL_BYTES},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_ENTRY_BYTES: usize = MAX_MODEL_BYTES + 4096;
const MAX_ENTRIES: usize = 256;
fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub label: String,
    pub run_id: Option<String>,
    pub generation: Option<usize>,
    pub training_arrivals: Option<u32>,
    pub training_cases: Option<usize>,
    pub runtime: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub version: u32,
    pub id: String,
    pub genome: DenseGenome,
    /// First recorded source; subsequent run histories remain in checkpoints.
    pub origin: Origin,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    entry: Entry,
}
impl Entry {
    pub fn new(genome: DenseGenome, origin: Origin) -> Result<Self, EvolutionError> {
        let model = DenseModel::new(genome.clone()).map_err(error)?;
        let entry = Self {
            version: 1,
            id: model.id().into(),
            genome,
            origin,
        };
        entry.validate()?;
        Ok(entry)
    }
    pub fn validate(&self) -> Result<DenseModel, EvolutionError> {
        let model = DenseModel::new(self.genome.clone()).map_err(error)?;
        if self.version != 1
            || self.id != model.id()
            || self.origin.label.len() > 256
            || self.origin.run_id.as_ref().is_some_and(|s| s.len() > 100)
            || self.origin.runtime.as_ref().is_some_and(|s| s.len() > 256)
            || self.origin.training_arrivals.is_some() != self.origin.training_cases.is_some()
            || self
                .origin
                .training_arrivals
                .zip(self.origin.training_cases)
                .is_some_and(|(a, c)| c == 0 || a as usize > c)
        {
            return Err(error("invalid genome identity or provenance"));
        }
        Ok(model)
    }
    pub fn supports_practice(&self) -> bool {
        self.genome.contract == guidance::contract()
    }
}

/// Never replace an existing genome or its first provenance record.
pub fn save(root: &Path, entry: &Entry) -> Result<(), EvolutionError> {
    entry.validate()?;
    fs::create_dir_all(root).map_err(error)?;
    let path = root.join(format!("{}.json", entry.id));
    if path.try_exists().map_err(error)? {
        let existing = load(&path)?;
        if existing.genome != entry.genome {
            return Err(error("genome identity collision"));
        }
        return Ok(());
    }
    let bytes = serde_json::to_vec(&Envelope {
        sha256: hash(&serde_json::to_vec(entry).map_err(error)?),
        entry: entry.clone(),
    })
    .map_err(error)?;
    if bytes.len() > MAX_ENTRY_BYTES {
        return Err(error("genome artifact exceeds size limit"));
    }
    let mut tmp = tempfile::NamedTempFile::new_in(root).map_err(error)?;
    tmp.write_all(&bytes).map_err(error)?;
    tmp.as_file().sync_all().map_err(error)?;
    if let Err(e) = tmp.persist_noclobber(&path) {
        if e.error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error(e));
        }
        if load(&path)?.genome != entry.genome {
            return Err(error("genome identity collision"));
        }
    }
    #[cfg(unix)]
    fs::File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(error)?;
    Ok(())
}
fn load(path: &Path) -> Result<Entry, EvolutionError> {
    let bytes = read_bounded(path, MAX_ENTRY_BYTES)?;
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(error)?;
    if hash(&serde_json::to_vec(&envelope.entry).map_err(error)?) != envelope.sha256 {
        return Err(error("genome provenance checksum differs"));
    }
    envelope.entry.validate()?;
    if path.file_stem().and_then(|s| s.to_str()) != Some(&envelope.entry.id) {
        return Err(error("genome filename and identity differ"));
    }
    Ok(envelope.entry)
}
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}
/// Preserve earlier exports without inventing scores or resuming their jobs.
pub fn refresh(training_root: &Path) -> Result<Catalog, EvolutionError> {
    let root = training_root.join("genomes");
    let mut warnings = Vec::new();
    if training_root.try_exists().map_err(error)? {
        for (i, path) in fs::read_dir(training_root).map_err(error)?.enumerate() {
            if i >= MAX_ENTRIES {
                warnings.push("Legacy export scan limited to 256 directory entries.".into());
                break;
            }
            let path = path.map_err(error)?.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let Some(id) = name.strip_suffix(".best.json") else {
                continue;
            };
            if !id.starts_with("run-")
                || id.len() <= 4
                || id.len() >= 100
                || !id[4..].bytes().all(|c| c.is_ascii_digit() || c == b'-')
                || !path
                    .symlink_metadata()
                    .map_err(error)?
                    .file_type()
                    .is_file()
            {
                continue;
            }
            let import = || -> Result<(), EvolutionError> {
                let model =
                    DenseModel::from_json(&read_bounded(&path, MAX_MODEL_BYTES)?).map_err(error)?;
                save(
                    &root,
                    &Entry::new(
                        model.genome().clone(),
                        Origin {
                            label: "Earlier training export".into(),
                            run_id: Some(id.into()),
                            generation: None,
                            training_arrivals: None,
                            training_cases: None,
                            runtime: None,
                        },
                    )?,
                )
            };
            if let Err(e) = import() {
                warnings.push(format!("{name}: {e}"));
            }
        }
    }
    let mut catalog = catalog(&root)?;
    catalog.warnings.extend(warnings);
    Ok(catalog)
}
pub fn catalog(root: &Path) -> Result<Catalog, EvolutionError> {
    let mut catalog = Catalog {
        entries: bundled()?,
        warnings: vec![],
    };
    if !root.try_exists().map_err(error)? {
        return Ok(catalog);
    }
    for (i, path) in fs::read_dir(root).map_err(error)?.enumerate() {
        if i >= MAX_ENTRIES {
            catalog.warnings.push(format!(
                "Showing at most {MAX_ENTRIES} saved entries; all files remain on disk."
            ));
            break;
        }
        let path = path.map_err(error)?.path();
        if path.extension().is_none_or(|s| s != "json") {
            continue;
        }
        if path
            .symlink_metadata()
            .map_err(error)?
            .file_type()
            .is_symlink()
        {
            continue;
        }
        match load(&path) {
            Ok(entry) => {
                if !catalog.entries.iter().any(|e| e.id == entry.id) {
                    catalog.entries.push(entry);
                }
            }
            Err(e) => catalog.warnings.push(format!(
                "{}: {e}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    catalog
        .entries
        .sort_by(|a, b| a.origin.label.cmp(&b.origin.label).then(a.id.cmp(&b.id)));
    Ok(catalog)
}
fn bundled() -> Result<Vec<Entry>, EvolutionError> {
    let origin = Origin {
        label: "Stationary experiment · seed 3".into(),
        run_id: None,
        generation: Some(48),
        training_arrivals: None,
        training_cases: None,
        runtime: None,
    };
    let mut entries = vec![Entry::new(
        ship_qualification::candidate()?.genome().clone(),
        origin,
    )?];
    let provenance: serde_json::Value = serde_json::from_str(include_str!(
        "../fixtures/ship-arrival-mixed-v1/provenance.json"
    ))
    .map_err(error)?;
    for (index, bytes) in [
        include_bytes!("../fixtures/ship-arrival-mixed-v1/seed-1.json").as_slice(),
        include_bytes!("../fixtures/ship-arrival-mixed-v1/seed-2.json").as_slice(),
        include_bytes!("../fixtures/ship-arrival-mixed-v1/seed-3.json").as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let model = DenseModel::from_json(bytes).map_err(error)?;
        let fixture = &provenance["fixtures"][index];
        if fixture["genome_id"].as_str() != Some(model.id())
            || fixture["sha256"].as_str() != Some(hash(bytes).as_str())
        {
            return Err(error("bundled genome provenance differs"));
        }
        entries.push(Entry::new(
            model.genome().clone(),
            Origin {
                label: format!("Mixed experiment · seed {}", index + 1),
                run_id: None,
                generation: Some(32),
                training_arrivals: Some(
                    fixture["partitions"]["training"]["arrived"]
                        .as_u64()
                        .ok_or_else(|| error("missing training arrivals"))?
                        as u32,
                ),
                training_cases: Some(
                    fixture["partitions"]["training"]["cases"]
                        .as_u64()
                        .ok_or_else(|| error("missing training cases"))?
                        as usize,
                ),
                runtime: provenance["runtime"].as_str().map(String::from),
            },
        )?);
    }
    Ok(entries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Practice {
    Stationary,
    Moving,
}
impl Practice {
    pub fn case(self) -> ship_arrival::ArrivalCase {
        let name = match self {
            Self::Stationary => "stationary_right",
            Self::Moving => "moving_right",
        };
        ship_arrival::cases()
            .into_iter()
            .find(|c| c.name == name)
            .expect("fixed practice case")
    }
}
pub struct Trial {
    pub episode: ArrivalEpisode,
    pub receipt: PathBuf,
}
/// Uses a pinned genome in its own world; never reads or writes a training job.
pub fn practice(root: &Path, entry: &Entry, preset: Practice) -> Result<Trial, EvolutionError> {
    let model = entry.validate()?;
    model
        .require_contract(&guidance::contract())
        .map_err(error)?;
    let episode = ship_arrival::run_episode(preset.case(), Policy::Model(model), true)?;
    let mut summary = episode.clone();
    summary.trace.clear(); // Keep one bounded trace in memory, compact result on disk.
    let bytes = serde_json::to_vec_pretty(&summary).map_err(error)?;
    let directory = root.join(&entry.id);
    fs::create_dir_all(&directory).map_err(error)?;
    let receipt = directory.join(format!("{}.json", hash(&bytes)));
    if receipt.try_exists().map_err(error)? {
        if read_bounded(&receipt, MAX_ENTRY_BYTES)? != bytes {
            return Err(error("practice receipt checksum differs"));
        }
    } else {
        atomic_write(&receipt, &bytes)?;
    }
    Ok(Trial { episode, receipt })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn earlier_exports_migrate_without_inventing_scores_or_replacing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run-123-4.best.json");
        let mut genome = crate::ship_training::template();
        genome.layers[0].biases[0] = 0.125;
        let id = DenseModel::new(genome.clone()).unwrap().id().to_owned();
        let bytes = serde_json::to_vec_pretty(&genome).unwrap();
        fs::write(&path, &bytes).unwrap();
        let imported = refresh(dir.path()).unwrap();
        let entry = imported.entries.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.origin.run_id.as_deref(), Some("run-123-4"));
        assert!(entry.origin.training_arrivals.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        genome.layers[0].biases[0] = 0.25;
        fs::write(&path, serde_json::to_vec(&genome).unwrap()).unwrap();
        fs::write(dir.path().join("run-567-8.best.json"), b"broken").unwrap();
        let refreshed = refresh(dir.path()).unwrap();
        assert!(refreshed.entries.iter().any(|e| e.id == id));
        assert_eq!(refreshed.entries.len(), 6);
        assert_eq!(refreshed.warnings.len(), 1);
    }
    #[test]
    fn immutable_library_keeps_first_origin_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let mut entry = bundled().unwrap().remove(0);
        entry.genome.layers[0].biases[0] += 0.01;
        entry = Entry::new(entry.genome, entry.origin).unwrap();
        save(dir.path(), &entry).unwrap();
        let path = dir.path().join(format!("{}.json", entry.id));
        let before = fs::read(&path).unwrap();
        entry.origin.label = "Another source".into();
        save(dir.path(), &entry).unwrap();
        assert_eq!(before, fs::read(&path).unwrap());
        assert_eq!(catalog(dir.path()).unwrap().entries.len(), 5);
        let mut value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        value["entry"]["origin"]["label"] = "Changed without checksum".into();
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let found = catalog(dir.path()).unwrap();
        assert_eq!(found.entries.len(), 4);
        assert_eq!(found.warnings.len(), 1);
        assert!(found.warnings[0].contains("checksum"));
    }
    #[test]
    fn both_practices_pin_the_genome_and_repeat_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let entry = bundled().unwrap().remove(1);
        for preset in [Practice::Stationary, Practice::Moving] {
            let a = practice(dir.path(), &entry, preset).unwrap();
            let b = practice(dir.path(), &entry, preset).unwrap();
            assert_eq!(a.episode, b.episode);
            assert_eq!(a.receipt, b.receipt);
            assert_eq!(a.episode.brain.controller, entry.id);
            assert_eq!(a.episode.outcome, ship_arrival::ArrivalOutcome::Arrived);
            assert!(!a.episode.trace.is_empty());
            let recorded: serde_json::Value =
                serde_json::from_slice(&fs::read(a.receipt).unwrap()).unwrap();
            assert_eq!(recorded["trace"], serde_json::json!([]));
            assert_eq!(recorded["brain_id"], a.episode.brain_id);
        }
        let mut wrong = entry;
        wrong.genome.contract.role = "another_role".into();
        wrong = Entry::new(wrong.genome, wrong.origin).unwrap();
        assert!(practice(dir.path(), &wrong, Practice::Stationary).is_err());
    }
}
