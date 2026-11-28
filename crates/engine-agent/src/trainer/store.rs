//! The manifest is the commit point. Genome files and a generation's replay and
//! summary are durable before it advances. A crashed append is truncated on resume.
use super::{Generation, GenerationSummary, TrainerConfig, reason};
use crate::{
    duck_recipe,
    evolution::{atomic_write, read_bounded, runtime_id},
};
use engine_common::brain_controls::{CREATURE_PROFILE, CREATURE_SENSES};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use spacewars_ai::c3::{ARCHITECTURE, C3Genome, DECISION_TICKS};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

pub(super) const MANIFEST: &str = "checkpoint.json";
const HISTORY: &str = "generations.jsonl";
const MAILBOX: &str = "pending-edit.json";
const MANIFEST_LIMIT: usize = 1024 * 1024;
const RECORD_LIMIT: usize = 1024 * 1024;
const MODEL_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Contract {
    recipe: String,
    scoring: String,
    senses: String,
    profile: String,
    architecture: String,
    decision_ticks: u32,
    validation: String,
}
impl Default for Contract {
    fn default() -> Self {
        Self {
            recipe: duck_recipe::RECIPE.into(),
            scoring: duck_recipe::SCORE.into(),
            senses: CREATURE_SENSES.into(),
            profile: CREATURE_PROFILE.into(),
            architecture: ARCHITECTURE.into(),
            decision_ticks: DECISION_TICKS,
            validation: "mirrored_worst_v1".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Checkpoint {
    version: u32,
    pub run_id: String,
    pub runtime: String,
    contract: Contract,
    pub config: TrainerConfig,
    pub completed: u32,
    /// Scored slots in descending fitness order. The first member is the elite.
    pub population: Vec<String>,
    pub history_bytes: u64,
    edit_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Edit {
    run_id: String,
    revision: u64,
    pub effective_generation: u32,
    pub config: TrainerConfig,
}

fn read<T: DeserializeOwned>(path: &Path, limit: usize) -> Result<T, String> {
    let bytes = read_bounded(path, limit).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
fn write<T: Serialize>(path: &Path, value: &T, limit: usize) -> Result<(), String> {
    let json = serde_json::to_vec(value).map_err(reason)?;
    if json.len() > limit {
        return Err(format!("{} exceeds size limit", path.display()));
    }
    atomic_write(path, &json).map_err(reason)
}
fn lock_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(reason)
}
pub(super) fn lock_run(dir: &Path) -> Result<File, String> {
    fs::create_dir_all(dir).map_err(reason)?;
    let file = lock_file(&dir.join("worker.lock"))?;
    file.try_lock().map_err(|e| {
        format!(
            "Run directory {} already in use or unavailable: {e}",
            dir.display()
        )
    })?;
    Ok(file)
}
pub(super) fn prepare(dir: &Path) -> Result<(), String> {
    for child in ["genomes", "replays"] {
        fs::create_dir_all(dir.join(child)).map_err(reason)?;
    }
    atomic_write(&dir.join(HISTORY), b"").map_err(reason)
}
fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn load_genome(dir: &Path, id: &str) -> Result<C3Genome, String> {
    if !valid_id(id) {
        return Err("Invalid C3 genome ID".into());
    }
    let bytes = read_bounded(&dir.join("genomes").join(format!("{id}.json")), MODEL_LIMIT)
        .map_err(|e| format!("Could not load genome {id}: {e}"))?;
    let genome = C3Genome::from_json(&bytes).map_err(|e| format!("Genome {id}: {e}"))?;
    if genome.id() != id {
        return Err(format!("Content does not match genome ID {id}"));
    }
    Ok(genome)
}
pub(super) fn save_genome(dir: &Path, genome: &C3Genome) -> Result<(), String> {
    let path = dir.join("genomes").join(format!("{}.json", genome.id()));
    if !path.try_exists().map_err(reason)? {
        atomic_write(&path, &genome.to_json().map_err(reason)?).map_err(reason)?;
    }
    Ok(())
}
pub(super) fn save_population(dir: &Path, population: &[Arc<C3Genome>]) -> Result<(), String> {
    population
        .iter()
        .try_for_each(|genome| save_genome(dir, genome))
}

impl Checkpoint {
    pub fn new(config: TrainerConfig, genomes: &[Arc<C3Genome>]) -> Result<Self, String> {
        let population: Vec<_> = genomes.iter().map(|g| g.id().to_owned()).collect();
        let contract = Contract::default();
        let runtime = runtime_id().map_err(reason)?;
        let identity =
            serde_json::to_vec(&(&contract, &config, &population, &runtime)).map_err(reason)?;
        Ok(Self {
            version: 1,
            run_id: format!("{:x}", Sha256::digest(identity)),
            contract,
            config,
            runtime,
            completed: 0,
            population,
            history_bytes: 0,
            edit_revision: 0,
        })
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        write(&dir.join(MANIFEST), self, MANIFEST_LIMIT)
    }
    pub fn load(dir: &Path) -> Result<Self, String> {
        let s: Self = read(&dir.join(MANIFEST), MANIFEST_LIMIT)?;
        s.config.validate()?;
        if s.version != 1
            || s.contract != Contract::default()
            || !valid_id(&s.run_id)
            || s.completed > s.config.generations
            || s.population.len() != s.config.population
            || s.population.iter().any(|id| !valid_id(id))
            || s.history_bytes > u64::from(s.completed) * 16384
        {
            return Err("Invalid or incompatible Clock trainer checkpoint".into());
        }
        Ok(s)
    }
    pub fn edit(&self, dir: &Path) -> Result<Option<Edit>, String> {
        let path = dir.join(MAILBOX);
        if !path.try_exists().map_err(reason)? {
            return Ok(None);
        }
        let edit: Edit = read(&path, 16384)?;
        if edit.run_id != self.run_id {
            return Err("Edit belongs to a different run".into());
        }
        if edit.revision <= self.edit_revision {
            return Ok(None);
        }
        edit.config.validate_edit(&self.config)?;
        Ok(Some(edit))
    }
    pub fn latest(&self, dir: &Path) -> Result<Option<Generation>, String> {
        if self.completed == 0 {
            return Ok(None);
        }
        let record = self.record(dir, self.completed)?;
        if record.best_genome != self.population[0] || record.config != self.config {
            return Err("Saved winner does not match its checkpoint".into());
        }
        Ok(Some(record))
    }

    pub fn record(&self, dir: &Path, generation: u32) -> Result<Generation, String> {
        if generation == 0 || generation > self.completed {
            return Err("Choose a completed generation for playback".into());
        }
        let record: Generation = read(
            &dir.join("replays").join(format!("{generation}.json")),
            RECORD_LIMIT,
        )?;
        if record.run_id != self.run_id
            || record.generation != generation
            || record.best.genome_id != record.best_genome
            || record.best.case != record.best_case
        {
            return Err("Saved replay does not match its generation".into());
        }
        for replay in [&record.current, &record.best]
            .into_iter()
            .chain(&record.last_finished)
        {
            if replay.generation != generation || !valid_id(&replay.genome_id) {
                return Err("Saved replay does not match its generation".into());
            }
            replay.validate_replay().map_err(reason)?;
        }
        Ok(record)
    }
    pub fn history(&self, dir: &Path) -> Result<Vec<GenerationSummary>, String> {
        let f = File::open(dir.join(HISTORY)).map_err(reason)?;
        let mut history = Vec::new();
        for line in BufReader::new(f.take(self.history_bytes)).lines() {
            let r: GenerationSummary =
                serde_json::from_str(&line.map_err(reason)?).map_err(reason)?;
            if r.generation != history.len() as u32 + 1 || r.run_id != self.run_id {
                return Err("Generation history is out of order".into());
            }
            history.push(r);
        }
        if history.len() != self.completed as usize {
            return Err("Incomplete generation history".into());
        }
        Ok(history)
    }
    pub fn commit(
        &self,
        dir: &Path,
        record: &Generation,
        pop: &[Arc<C3Genome>],
        edit: Option<&Edit>,
    ) -> Result<Self, String> {
        let mut next = self.clone();
        next.completed = record.generation;
        next.config = record.config.clone();
        next.population = pop.iter().map(|g| g.id().to_owned()).collect();
        if let Some(e) = edit {
            next.edit_revision = e.revision;
        }
        write(
            &dir.join("replays")
                .join(format!("{}.json", record.generation)),
            record,
            RECORD_LIMIT,
        )?;
        let mut history = restore_history(dir, self.history_bytes)?;
        let mut bytes = serde_json::to_vec(record.summary()).map_err(reason)?;
        bytes.push(b'\n');
        history.seek(SeekFrom::End(0)).map_err(reason)?;
        history
            .write_all(&bytes)
            .and_then(|_| history.sync_all())
            .map_err(reason)?;
        next.history_bytes += bytes.len() as u64;
        next.save(dir)?;
        Ok(next)
    }
}
pub(super) fn restore_history(dir: &Path, length: u64) -> Result<File, String> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join(HISTORY))
        .map_err(reason)?;
    match f.metadata().map_err(reason)?.len().cmp(&length) {
        std::cmp::Ordering::Less => {
            return Err("Generation history is shorter than its checkpoint".into());
        }
        std::cmp::Ordering::Greater => f
            .set_len(length)
            .and_then(|_| f.sync_all())
            .map_err(reason)?,
        std::cmp::Ordering::Equal => {}
    }
    Ok(f)
}
#[derive(Serialize, Deserialize)]
struct Scheduled {
    run_id: String,
    generation: u32,
    edit: Option<Edit>,
}
fn boundary_lock(dir: &Path) -> Result<File, String> {
    let lock = lock_file(&dir.join("edit.lock"))?;
    lock.lock().map_err(reason)?;
    Ok(lock)
}
fn active(dir: &Path, state: &Checkpoint) -> Result<Option<Scheduled>, String> {
    let path = dir.join("active-generation.json");
    if !path.try_exists().map_err(reason)? {
        return Ok(None);
    }
    let active: Scheduled = read(&path, 32768)?;
    if active.run_id != state.run_id || active.generation > state.completed + 1 {
        return Err("Invalid scheduled generation".into());
    }
    Ok((active.generation > state.completed).then_some(active))
}
/// Freeze scheduling and offspring settings before workers see the generation.
/// A restarted owner repeats the reserved generation with its original edits.
pub(super) fn schedule(dir: &Path, state: &Checkpoint) -> Result<Option<Edit>, String> {
    let _lock = boundary_lock(dir)?;
    if let Some(active) = active(dir, state)? {
        return Ok(active.edit);
    }
    let generation = state.completed + 1;
    let edit = state
        .edit(dir)?
        .filter(|e| e.effective_generation <= generation);
    let schedule = Scheduled {
        run_id: state.run_id.clone(),
        generation,
        edit: edit.clone(),
    };
    write(&dir.join("active-generation.json"), &schedule, 32768)?;
    Ok(edit)
}
pub(super) fn submit_edit(dir: &Path, config: TrainerConfig) -> Result<Edit, String> {
    modify_edit(dir, |current| {
        *current = config;
        Ok(())
    })
}

pub(super) fn modify_edit(
    dir: &Path,
    modify: impl FnOnce(&mut TrainerConfig) -> Result<(), String>,
) -> Result<Edit, String> {
    let _lock = boundary_lock(dir)?;
    let state = Checkpoint::load(dir)?;
    let effective_generation =
        active(dir, &state)?.map_or(state.completed + 1, |s| s.generation + 1);
    if effective_generation > state.config.generations {
        return Err("Completed run cannot accept edits".into());
    }
    let (previous, mut config) = match state.edit(dir)? {
        Some(edit) => (edit.revision, edit.config),
        None => (state.edit_revision, state.config.clone()),
    };
    modify(&mut config)?;
    config.validate_edit(&state.config)?;
    let e = Edit {
        run_id: state.run_id,
        effective_generation,
        revision: previous.checked_add(1).ok_or("Edit sequence exhausted")?,
        config,
    };
    write(&dir.join(MAILBOX), &e, 16384)?;
    Ok(e)
}
