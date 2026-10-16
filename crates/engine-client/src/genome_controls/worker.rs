use engine_agent::{
    brain_assembly::{self as assemblies, Record},
    evolution::EvolutionError,
    genome_library::{self as library, Entry, Practice, Trial},
};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Selection {
    Component(String),
    Brain(String),
}
pub(super) enum Request {
    Refresh,
    Component(String),
    Select(Selection),
    Save(String),
    Copy { source: String, component: String },
    Trial(Selection, Practice),
    Shutdown,
}
pub(super) enum Reply {
    Catalog(Catalog, Option<Selection>),
    Trial(Box<Trial>),
    Error(String),
}
pub(super) struct Catalog {
    pub components: Vec<Entry>,
    pub brains: Vec<Record>,
    pub warnings: Vec<String>,
}

fn refresh(root: &Path, selection: Option<Selection>) -> Result<Reply, EvolutionError> {
    let components = library::refresh(root)?;
    let mut catalog = Catalog {
        components: components.entries,
        brains: Vec::new(),
        warnings: components.warnings,
    };
    match assemblies::catalog(root) {
        Ok(brains) => {
            catalog.brains = brains.entries;
            catalog.warnings.extend(brains.warnings);
        }
        Err(e) => catalog
            .warnings
            .push(format!("Cannot read saved brains: {e}")),
    }
    // Direct dependency links and newly saved brains remain selectable past a
    // bounded scan. Refresh starts fresh, retaining at most one extra entry.
    match &selection {
        Some(Selection::Component(id)) => {
            let entry = library::resolve(&root.join("genomes"), id)?;
            if !catalog.components.iter().any(|e| e.id == *id) {
                catalog.components.push(entry);
            }
        }
        Some(Selection::Brain(id)) => {
            let record = assemblies::inspect(root, id)?;
            if !catalog.brains.iter().any(|e| e.artifact.id == *id) {
                catalog.warnings.extend(record.warning.iter().cloned());
                catalog.brains.push(record);
            }
        }
        None => {}
    }
    Ok(Reply::Catalog(catalog, selection))
}

fn name(label: &str, copy: bool) -> String {
    let mut name = format!(
        "{}{}",
        label.replace(" experiment", ""),
        if copy { " · copy" } else { "" }
    );
    while name.len() > 128 {
        name.pop();
    }
    name
}

pub(super) fn execute(root: &Path, request: Request) -> Result<Reply, EvolutionError> {
    match request {
        Request::Refresh => refresh(root, None),
        Request::Component(id) => refresh(root, Some(Selection::Component(id))),
        Request::Select(selection) => {
            // Resolve the exact identity before legacy migration. A missing
            // link must fail rather than open another library entry.
            match &selection {
                Selection::Component(id) => {
                    library::resolve(&root.join("genomes"), id)?;
                }
                Selection::Brain(id) => {
                    assemblies::inspect(root, id)?;
                }
            }
            refresh(root, Some(selection))
        }
        Request::Save(id) => {
            let entry = library::resolve(&root.join("genomes"), &id)?;
            let artifact = assemblies::create(root, &id, &name(&entry.origin.label, false))?;
            refresh(root, Some(Selection::Brain(artifact.id)))
        }
        Request::Copy { source, component } => {
            let entry = library::resolve(&root.join("genomes"), &component)?;
            let artifact = assemblies::copy_with_component(
                root,
                &source,
                &component,
                &name(&entry.origin.label, true),
            )?;
            refresh(root, Some(Selection::Brain(artifact.id)))
        }
        Request::Trial(selection, preset) => {
            let trial = match selection {
                Selection::Component(id) => {
                    let entry = library::resolve(&root.join("genomes"), &id)?;
                    library::practice(&root.join("trials"), &entry, preset)?
                }
                Selection::Brain(id) => assemblies::practice(root, &id, preset)?,
            };
            Ok(Reply::Trial(Box::new(trial)))
        }
        Request::Shutdown => unreachable!(),
    }
}
