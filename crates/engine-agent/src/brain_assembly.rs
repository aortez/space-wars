//! Saved practice brains pin behavior and dependencies independently of names.
use crate::{
    evolution::{EvolutionError, atomic_write, read_bounded},
    genome_library::{self, Practice, Trial, valid_id},
    ship_arrival::{self, PracticeBrain},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use spacewars_ai::guidance::{self, Policy};
use std::{fs, io::Write, path::Path};

const MAX_BYTES: usize = 32 * 1024;
const MAX_ENTRIES: usize = 256;

fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub version: u32,
    pub organism: String,
    pub host: String,
    pub required_capabilities: Vec<String>,
    pub brain: PracticeBrain,
    pub practices: Vec<Practice>,
}
impl Definition {
    pub fn arrival(genome: &str) -> Self {
        Self {
            version: 1,
            organism: "spacewars_pilot_v1".into(),
            host: "material_arrival_practice_v1".into(),
            required_capabilities: vec![
                "constant_velocity_target_v1".into(),
                "material_ship_open_free_flight_v1".into(),
            ],
            brain: PracticeBrain::guidance(genome),
            practices: vec![Practice::Stationary, Practice::Moving],
        }
    }

    /// Host support is separate from artifact integrity and model architecture.
    pub fn require_supported(&self) -> Result<(), EvolutionError> {
        let expected = Self::arrival(&self.brain.controller);
        if self.version != expected.version || self.host != expected.host {
            return Err(error(format!("Unsupported practice host: {}", self.host)));
        }
        if self.organism != expected.organism
            || self.required_capabilities != expected.required_capabilities
        {
            return Err(error(
                "This practice host does not support the assembly's organism or required capabilities.",
            ));
        }
        if self.brain != expected.brain {
            return Err(error(
                "Unsupported target source, coordinator, or guidance contract. The saved brain cannot be substituted with the current host.",
            ));
        }
        if self.practices.is_empty()
            || self.practices.len() > expected.practices.len()
            || self.practices.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(error("Invalid or repeated practice scenarios."));
        }
        Ok(())
    }

    fn identity(&self) -> Result<String, EvolutionError> {
        let mut digest = Sha256::new();
        digest.update(b"spacewars.brain_assembly.v1\0");
        digest.update(serde_json::to_vec(self).map_err(error)?);
        Ok(format!("{:x}", digest.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub definition: Definition,
}
impl Artifact {
    pub fn new(definition: Definition) -> Result<Self, EvolutionError> {
        let artifact = Self {
            id: definition.identity()?,
            definition,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    fn validate(&self) -> Result<(), EvolutionError> {
        if self.definition.version != 1
            || !valid_id(&self.id)
            || !valid_id(&self.definition.brain.controller)
            || self.id != self.definition.identity()?
            || serde_json::to_vec(self).map_err(error)?.len() > MAX_BYTES / 2
        {
            return Err(error("Invalid assembly version or behavioral identity."));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    sha256: String,
    value: T,
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, EvolutionError> {
    let bytes = serde_json::to_vec(&Envelope {
        sha256: hash(&serde_json::to_vec(value).map_err(error)?),
        value,
    })
    .map_err(error)?;
    if bytes.len() > MAX_BYTES {
        return Err(error("Assembly artifact exceeds size limit."));
    }
    Ok(bytes)
}
fn read<T: DeserializeOwned + Serialize>(path: &Path) -> Result<T, EvolutionError> {
    if !path
        .symlink_metadata()
        .map_err(error)?
        .file_type()
        .is_file()
    {
        return Err(error("Assembly artifacts must be regular files."));
    }
    let envelope: Envelope<T> =
        serde_json::from_slice(&read_bounded(path, MAX_BYTES)?).map_err(error)?;
    if envelope.sha256 != hash(&serde_json::to_vec(&envelope.value).map_err(error)?) {
        return Err(error("Assembly artifact checksum differs."));
    }
    Ok(envelope.value)
}

/// Never overwrite behavior or receipts, including when another writer wins.
fn save_immutable(path: &Path, bytes: &[u8]) -> Result<(), EvolutionError> {
    let parent = path
        .parent()
        .ok_or_else(|| error("Missing storage directory."))?;
    fs::create_dir_all(parent).map_err(error)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
    temp.write_all(bytes).map_err(error)?;
    temp.as_file().sync_all().map_err(error)?;
    if let Err(e) = temp.persist_noclobber(path) {
        if e.error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error(e));
        }
        if !path
            .symlink_metadata()
            .map_err(error)?
            .file_type()
            .is_file()
            || read_bounded(path, MAX_BYTES)? != bytes
        {
            return Err(error(
                "Existing assembly artifact differs; restore it before saving.",
            ));
        }
    }
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(error)?;
    Ok(())
}

pub fn load(root: &Path, id: &str) -> Result<Artifact, EvolutionError> {
    if !valid_id(id) {
        return Err(error("Invalid assembly ID."));
    }
    let artifact: Artifact = read(&root.join("assemblies").join(format!("{id}.json")))?;
    artifact.validate()?;
    if artifact.id != id {
        return Err(error("Assembly filename and identity differ."));
    }
    Ok(artifact)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u32,
    assembly_id: String,
    name: String,
}
fn check_name(name: &str) -> Result<(), EvolutionError> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(error(
            "Assembly name must contain 1–128 bytes without control characters.",
        ));
    }
    Ok(())
}
fn metadata_path(root: &Path, id: &str) -> std::path::PathBuf {
    root.join("assemblies")
        .join("names")
        .join(format!("{id}.json"))
}
fn read_name(root: &Path, id: &str) -> Result<String, EvolutionError> {
    let metadata: Metadata = read(&metadata_path(root, id))?;
    if metadata.version != 1 || metadata.assembly_id != id {
        return Err(error(
            "Assembly name metadata has a different identity or version.",
        ));
    }
    check_name(&metadata.name)?;
    Ok(metadata.name)
}
pub fn rename(root: &Path, id: &str, name: &str) -> Result<(), EvolutionError> {
    load(root, id)?;
    check_name(name)?;
    let path = metadata_path(root, id);
    fs::create_dir_all(path.parent().unwrap()).map_err(error)?;
    atomic_write(
        &path,
        &encode(&Metadata {
            version: 1,
            assembly_id: id.into(),
            name: name.trim().into(),
        })?,
    )
}

fn save(root: &Path, artifact: &Artifact, name: &str) -> Result<(), EvolutionError> {
    artifact.validate()?;
    check_name(name)?;
    save_immutable(
        &root
            .join("assemblies")
            .join(format!("{}.json", artifact.id)),
        &encode(artifact)?,
    )?;
    // Saving the same composition again must not rename an existing brain.
    let path = metadata_path(root, &artifact.id);
    match path.symlink_metadata() {
        Ok(_) => {
            read_name(root, &artifact.id)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let bytes = encode(&Metadata {
                version: 1,
                assembly_id: artifact.id.clone(),
                name: name.trim().into(),
            })?;
            // A concurrent name creation may choose a different valid name.
            if let Err(e) = save_immutable(&path, &bytes) {
                read_name(root, &artifact.id).map_err(|_| e)?;
            }
        }
        Err(e) => return Err(error(e)),
    }
    Ok(())
}

pub fn create(root: &Path, genome_id: &str, name: &str) -> Result<Artifact, EvolutionError> {
    let component = genome_library::resolve(&root.join("genomes"), genome_id)?;
    component
        .validate()?
        .require_contract(&guidance::contract())
        .map_err(error)?;
    let artifact = Artifact::new(Definition::arrival(genome_id))?;
    check_name(name)?;
    // Persist bundled dependencies too, so a saved assembly has no champion alias.
    genome_library::save(&root.join("genomes"), &component)?;
    save(root, &artifact, name)?;
    Ok(artifact)
}

pub fn copy_with_component(
    root: &Path,
    source_id: &str,
    genome_id: &str,
    name: &str,
) -> Result<Artifact, EvolutionError> {
    let mut definition = load(root, source_id)?.definition;
    definition.require_supported()?;
    if definition.brain.controller == genome_id {
        return Err(error("Choose a different guidance component for the copy."));
    }
    let component = genome_library::resolve(&root.join("genomes"), genome_id)?;
    component
        .validate()?
        .require_contract(&definition.brain.contract)
        .map_err(error)?;
    definition.brain.controller = genome_id.into();
    let artifact = Artifact::new(definition)?;
    check_name(name)?;
    genome_library::save(&root.join("genomes"), &component)?;
    save(root, &artifact, name)?;
    Ok(artifact)
}

#[derive(Debug, Clone)]
pub struct Record {
    pub artifact: Artifact,
    pub name: String,
    pub unavailable: Option<String>,
    pub warning: Option<String>,
}
#[derive(Debug, Default)]
pub struct Catalog {
    pub entries: Vec<Record>,
    pub warnings: Vec<String>,
}
fn resolve_component(
    root: &Path,
    artifact: &Artifact,
) -> Result<genome_library::Entry, EvolutionError> {
    artifact.definition.require_supported()?;
    let component =
        genome_library::resolve(&root.join("genomes"), &artifact.definition.brain.controller)?;
    component
        .validate()?
        .require_contract(&artifact.definition.brain.contract)
        .map_err(error)?;
    Ok(component)
}

pub fn inspect(root: &Path, id: &str) -> Result<Record, EvolutionError> {
    let artifact = load(root, id)?;
    let (name, warning) = match read_name(root, id) {
        Ok(name) => (name, None),
        Err(e) => (
            format!("Practice brain {}", &id[..12]),
            Some(format!("Name for {id}: {e}")),
        ),
    };
    let unavailable = resolve_component(root, &artifact)
        .err()
        .map(|e| e.to_string());
    Ok(Record {
        artifact,
        name,
        unavailable,
        warning,
    })
}

pub fn catalog(root: &Path) -> Result<Catalog, EvolutionError> {
    let directory = root.join("assemblies");
    let mut catalog = Catalog::default();
    if !directory.try_exists().map_err(error)? {
        return Ok(catalog);
    }
    for (index, entry) in fs::read_dir(directory).map_err(error)?.enumerate() {
        if index >= MAX_ENTRIES {
            catalog
                .warnings
                .push("Assembly scan limited to 256 entries; all files remain on disk.".into());
            break;
        }
        let path = entry.map_err(error)?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|id| id.to_str())
            .unwrap_or_default();
        match inspect(root, id) {
            Ok(record) => {
                catalog.warnings.extend(record.warning.iter().cloned());
                catalog.entries.push(record);
            }
            Err(e) => catalog.warnings.push(format!("{}: {e}", path.display())),
        }
    }
    catalog
        .entries
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.artifact.id.cmp(&b.artifact.id)));
    Ok(catalog)
}

#[derive(Serialize)]
struct Receipt<'a> {
    version: u32,
    assembly: &'a Artifact,
    episode: &'a ship_arrival::ArrivalEpisode,
}

/// Resolve from storage again before running. Cached browser availability is not
/// authority to execute a deleted, corrupted or replaced dependency.
pub fn practice(root: &Path, id: &str, preset: Practice) -> Result<Trial, EvolutionError> {
    let artifact = load(root, id)?;
    let component = resolve_component(root, &artifact)?;
    if !artifact.definition.practices.contains(&preset) {
        return Err(error(
            "This assembly does not declare the selected practice scenario.",
        ));
    }
    let episode =
        ship_arrival::run_episode(preset.case(), Policy::Model(component.validate()?), true)?;
    if episode.brain != artifact.definition.brain {
        return Err(error(
            "The running practice host differs from the saved assembly.",
        ));
    }
    let mut summary = episode.clone();
    summary.trace.clear();
    let bytes = serde_json::to_vec_pretty(&Receipt {
        version: 1,
        assembly: &artifact,
        episode: &summary,
    })
    .map_err(error)?;
    if bytes.len() > MAX_BYTES {
        return Err(error("Assembly practice receipt exceeds size limit."));
    }
    let receipt = root
        .join("assembly-trials")
        .join(id)
        .join(format!("{}.json", hash(&bytes)));
    save_immutable(&receipt, &bytes)?;
    Ok(Trial { episode, receipt })
}

#[cfg(test)]
mod tests;
