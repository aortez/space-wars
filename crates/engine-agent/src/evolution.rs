//! Small deterministic evolutionary runner. Tasks own their worlds and fitness;
//! this module only loads models, schedules candidates, and preserves job state.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use spacewars_ai::model::{DenseGenome, DenseModel, ModelContract};
use std::{
    fmt,
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::OnceLock,
};

pub const CHECKPOINT_VERSION: u32 = 1;
pub const MAX_CHECKPOINT_BYTES: usize = 32 * 1024 * 1024;
const MAX_STORED_PARAMETERS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskManifest {
    pub id: String,
    pub contract: ModelContract,
    pub configuration: serde_json::Value,
}

pub trait EvaluationTask {
    type Fitness: Clone + Ord + Serialize + DeserializeOwned;
    fn manifest(&self) -> TaskManifest;
    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionConfig {
    pub seed: u64,
    pub population: usize,
    pub generations: usize,
    pub elites: usize,
    pub tournament_size: usize,
    pub mutation_probability: f32,
    pub mutation_scale: f32,
    pub initial_scale: f32,
    pub parameter_limit: f32,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            population: 16,
            generations: 8,
            elites: 2,
            tournament_size: 3,
            mutation_probability: 0.2,
            mutation_scale: 0.5,
            initial_scale: 1.0,
            parameter_limit: 8.0,
        }
    }
}

impl EvolutionConfig {
    fn validate(&self, template: &DenseGenome, seeds: usize) -> Result<(), EvolutionError> {
        if !(2..=128).contains(&self.population)
            || !(1..=256).contains(&self.generations)
            || self.elites == 0
            || self.elites >= self.population
            || !(1..=self.population).contains(&self.tournament_size)
            || seeds > self.population
            || !self.mutation_probability.is_finite()
            || !(0.0..=1.0).contains(&self.mutation_probability)
            || !self.parameter_limit.is_finite()
            || !(0.0..=32.0).contains(&self.parameter_limit)
            || self.parameter_limit == 0.0
            || [self.initial_scale, self.mutation_scale]
                .into_iter()
                .any(|v| !v.is_finite() || v < 0.0 || v > self.parameter_limit)
        {
            return Err(EvolutionError(
                "invalid or excessive evolution configuration".into(),
            ));
        }
        let copies = self.population + self.generations + seeds + 1;
        if template.parameter_count().saturating_mul(copies) > MAX_STORED_PARAMETERS {
            return Err(EvolutionError(
                "experiment exceeds the stored parameter budget".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionError(pub String);

impl fmt::Display for EvolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for EvolutionError {}

fn error(e: impl fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub genome: DenseGenome,
    /// The source artifact for a seed, elite copy, or mutated offspring.
    pub source_genome: Option<String>,
    pub mutated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationResult<F> {
    pub generation: usize,
    pub best_genome: DenseGenome,
    pub best_id: String,
    pub best_fitness: F,
    pub population_ids: Vec<String>,
    pub fitness: Vec<F>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionRun<F> {
    runtime: String,
    config: EvolutionConfig,
    task: TaskManifest,
    template: DenseGenome,
    seeds: Vec<DenseGenome>,
    generation: usize,
    population: Vec<Candidate>,
    fitness: Vec<Option<F>>,
    history: Vec<GenerationResult<F>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint<F> {
    version: u32,
    sha256: String,
    run: EvolutionRun<F>,
}

impl<F: Clone + Ord + Serialize + DeserializeOwned> EvolutionRun<F> {
    pub fn new<T: EvaluationTask<Fitness = F>>(
        task: &T,
        template: DenseGenome,
        seeds: Vec<DenseGenome>,
        config: EvolutionConfig,
    ) -> Result<Self, EvolutionError> {
        let model = DenseModel::new(template.clone()).map_err(error)?;
        let manifest = task.manifest();
        model.require_contract(&manifest.contract).map_err(error)?;
        config.validate(&template, seeds.len())?;
        for genome in &seeds {
            validate_seed(genome, &template, &config)?;
        }
        let mut population = Vec::with_capacity(config.population);
        for index in 0..config.population {
            if let Some(genome) = seeds.get(index) {
                population.push(Candidate {
                    genome: genome.clone(),
                    source_genome: Some(
                        DenseModel::new(genome.clone()).map_err(error)?.id().into(),
                    ),
                    mutated: false,
                });
            } else {
                let mut genome = template.clone();
                let mut rng = Draws::new(config.seed, 0, index, b"initial");
                for p in genome.parameters_mut() {
                    *p = rng.signed() * config.initial_scale;
                }
                population.push(Candidate {
                    genome,
                    source_genome: None,
                    mutated: false,
                });
            }
        }
        Ok(Self {
            runtime: runtime_id()?,
            fitness: vec![None; config.population],
            config,
            task: manifest,
            template,
            seeds,
            generation: 0,
            population,
            history: Vec::new(),
        })
    }

    pub fn config(&self) -> &EvolutionConfig {
        &self.config
    }
    pub fn task(&self) -> &TaskManifest {
        &self.task
    }
    pub fn seeds(&self) -> &[DenseGenome] {
        &self.seeds
    }
    pub fn generation(&self) -> usize {
        self.generation
    }
    pub fn population(&self) -> &[Candidate] {
        &self.population
    }
    pub fn fitness(&self) -> &[Option<F>] {
        &self.fitness
    }
    pub fn history(&self) -> &[GenerationResult<F>] {
        &self.history
    }
    pub fn complete(&self) -> bool {
        self.history.len() == self.config.generations
    }

    /// One complete candidate batch is the checkpoint boundary for this prototype.
    /// Failed evaluations leave the slot pending for retry; they are not bad fitness.
    pub fn advance<T: EvaluationTask<Fitness = F>>(
        &mut self,
        task: &T,
    ) -> Result<bool, EvolutionError> {
        if task.manifest() != self.task {
            return Err(EvolutionError("task recipe changed".into()));
        }
        if self.complete() {
            return Ok(false);
        }
        let index = self
            .fitness
            .iter()
            .position(Option::is_none)
            .ok_or_else(|| EvolutionError("invalid evaluation phase".into()))?;
        let model = DenseModel::new(self.population[index].genome.clone()).map_err(error)?;
        let fitness = task.evaluate(&model)?;
        self.fitness[index] = Some(fitness);
        if self.fitness.iter().all(Option::is_some) {
            self.finish_generation()?;
        }
        Ok(true)
    }

    fn finish_generation(&mut self) -> Result<(), EvolutionError> {
        let models = self
            .population
            .iter()
            .map(|candidate| DenseModel::new(candidate.genome.clone()).map_err(error))
            .collect::<Result<Vec<_>, _>>()?;
        let scores = self
            .fitness
            .iter()
            .map(|f| f.as_ref().expect("completed generation").clone())
            .collect::<Vec<_>>();
        let mut ranks: Vec<usize> = (0..models.len()).collect();
        ranks.sort_by(|&a, &b| {
            scores[b]
                .cmp(&scores[a])
                .then_with(|| models[a].id().cmp(models[b].id()))
                .then(a.cmp(&b))
        });
        let winner = ranks[0];
        self.history.push(GenerationResult {
            generation: self.generation,
            best_genome: models[winner].genome().clone(),
            best_id: models[winner].id().into(),
            best_fitness: scores[winner].clone(),
            population_ids: models.iter().map(|m| m.id().into()).collect(),
            fitness: scores.clone(),
        });
        if self.complete() {
            return Ok(());
        }
        let mut next = Vec::with_capacity(self.config.population);
        for &index in &ranks[..self.config.elites] {
            next.push(Candidate {
                genome: models[index].genome().clone(),
                source_genome: Some(models[index].id().into()),
                mutated: false,
            });
        }
        for index in self.config.elites..self.config.population {
            let mut rng = Draws::new(self.config.seed, self.generation + 1, index, b"offspring");
            // Sample ranks with replacement; rank order includes deterministic ties.
            let mut parent_rank = rng.index(ranks.len());
            for _ in 1..self.config.tournament_size {
                parent_rank = parent_rank.min(rng.index(ranks.len()));
            }
            let parent = &models[ranks[parent_rank]];
            let mut genome = parent.genome().clone();
            for value in genome.parameters_mut() {
                if rng.unit() < self.config.mutation_probability {
                    *value = (*value + rng.signed() * self.config.mutation_scale)
                        .clamp(-self.config.parameter_limit, self.config.parameter_limit);
                }
            }
            next.push(Candidate {
                mutated: &genome != parent.genome(),
                genome,
                source_genome: Some(parent.id().into()),
            });
        }
        self.population = next;
        self.fitness.fill(None);
        self.generation += 1;
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), EvolutionError> {
        atomic_write(path, &self.encode()?)
    }

    /// The same validated checkpoint format, for an enclosing atomic job save.
    pub fn encode(&self) -> Result<Vec<u8>, EvolutionError> {
        let envelope = Checkpoint {
            version: CHECKPOINT_VERSION,
            sha256: hash(self)?,
            run: self.clone(),
        };
        let bytes = serde_json::to_vec(&envelope).map_err(error)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(EvolutionError("checkpoint size limit exceeded".into()));
        }
        Ok(bytes)
    }

    pub fn load<T: EvaluationTask<Fitness = F>>(
        path: &Path,
        task: &T,
    ) -> Result<Self, EvolutionError> {
        let bytes = read_bounded(path, MAX_CHECKPOINT_BYTES)?;
        Self::decode(&bytes, task)
    }

    pub fn decode<T: EvaluationTask<Fitness = F>>(
        bytes: &[u8],
        task: &T,
    ) -> Result<Self, EvolutionError> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(EvolutionError("checkpoint size limit exceeded".into()));
        }
        let checkpoint: Checkpoint<F> = serde_json::from_slice(bytes).map_err(error)?;
        if checkpoint.version != CHECKPOINT_VERSION || hash(&checkpoint.run)? != checkpoint.sha256 {
            return Err(EvolutionError(
                "checkpoint version or checksum mismatch".into(),
            ));
        }
        let run = checkpoint.run;
        run.validate(task)?;
        Ok(run)
    }

    fn validate<T: EvaluationTask<Fitness = F>>(&self, task: &T) -> Result<(), EvolutionError> {
        if self.runtime != runtime_id()? {
            return Err(EvolutionError(
                "checkpoint runtime differs; start a new run from compatible genomes".into(),
            ));
        }
        if self.task != task.manifest() {
            return Err(EvolutionError("checkpoint task recipe differs".into()));
        }
        DenseModel::new(self.template.clone())
            .map_err(error)?
            .require_contract(&self.task.contract)
            .map_err(error)?;
        self.config.validate(&self.template, self.seeds.len())?;
        if self.population.len() != self.config.population
            || self.fitness.len() != self.config.population
            || self.generation >= self.config.generations
            || self.history.len() > self.config.generations
            || self.history.len() != self.generation + usize::from(self.complete())
        {
            return Err(EvolutionError("invalid checkpoint progress".into()));
        }
        let evaluated = self.fitness.iter().take_while(|f| f.is_some()).count();
        if self.fitness[evaluated..].iter().any(Option::is_some)
            || self.complete() != (evaluated == self.config.population)
        {
            return Err(EvolutionError(
                "invalid checkpoint evaluation prefix".into(),
            ));
        }
        for genome in self
            .seeds
            .iter()
            .chain(self.population.iter().map(|p| &p.genome))
        {
            validate_seed(genome, &self.template, &self.config)?;
        }
        for (index, result) in self.history.iter().enumerate() {
            validate_seed(&result.best_genome, &self.template, &self.config)?;
            let model = DenseModel::new(result.best_genome.clone()).map_err(error)?;
            if result.generation != index
                || result.best_id != model.id()
                || result.population_ids.len() != self.config.population
                || result.fitness.len() != self.config.population
                || result.fitness.iter().max() != Some(&result.best_fitness)
                || !result.population_ids.contains(&result.best_id)
            {
                return Err(EvolutionError(
                    "invalid checkpoint generation result".into(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_seed(
    genome: &DenseGenome,
    template: &DenseGenome,
    config: &EvolutionConfig,
) -> Result<(), EvolutionError> {
    DenseModel::new(genome.clone()).map_err(error)?;
    if !genome.same_architecture(template)
        || genome
            .parameters()
            .any(|p| p.abs() > config.parameter_limit)
    {
        return Err(EvolutionError(
            "seed architecture or parameter range is incompatible".into(),
        ));
    }
    Ok(())
}

fn hash(value: &impl Serialize) -> Result<String, EvolutionError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(error)?)
    ))
}

/// Keyed counters pin the draw algorithm without serializing an RNG library's
/// internal layout. Each candidate has its own stream, independent of scheduling.
struct Draws {
    seed: u64,
    generation: u64,
    candidate: u64,
    domain: &'static [u8],
    counter: u64,
}
impl Draws {
    fn new(seed: u64, generation: usize, candidate: usize, domain: &'static [u8]) -> Self {
        Self {
            seed,
            generation: generation as u64,
            candidate: candidate as u64,
            domain,
            counter: 0,
        }
    }
    fn next(&mut self) -> u64 {
        let mut hash = Sha256::new();
        hash.update(b"spacewars.evolution.draw.v1\0");
        hash.update(self.domain);
        for value in [self.seed, self.generation, self.candidate, self.counter] {
            hash.update(value.to_le_bytes());
        }
        self.counter += 1;
        u64::from_le_bytes(hash.finalize()[..8].try_into().expect("eight digest bytes"))
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
    fn index(&mut self, count: usize) -> usize {
        (self.next() % count as u64) as usize
    }
}

/// Prototype resumes require the same executable and platform. This deliberately
/// refuses an exact-resume claim after rebuilding or changing the numeric runtime.
pub fn runtime_id() -> Result<String, EvolutionError> {
    static ID: OnceLock<Result<String, EvolutionError>> = OnceLock::new();
    ID.get_or_init(|| {
        let mut file = File::open(std::env::current_exe().map_err(error)?).map_err(error)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 16_384];
        loop {
            let size = file.read(&mut buffer).map_err(error)?;
            if size == 0 {
                break;
            }
            hash.update(&buffer[..size]);
        }
        Ok(format!(
            "diagnostic_v1/{}/{}/{:x}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            hash.finalize()
        ))
    })
    .clone()
}

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, EvolutionError> {
    let file = File::open(path).map_err(error)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > limit {
        return Err(EvolutionError("input file exceeds size limit".into()));
    }
    Ok(bytes)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), EvolutionError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
    temp.write_all(bytes).map_err(error)?;
    temp.as_file().sync_all().map_err(error)?;
    temp.persist(path).map_err(error)?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
