use super::*;

fn components(root: &Path) -> Vec<genome_library::Entry> {
    genome_library::catalog(&root.join("genomes"))
        .unwrap()
        .entries
}

#[test]
fn named_assemblies_copy_dependencies_without_mutating_behavior_or_first_names() {
    let dir = tempfile::tempdir().unwrap();
    let genomes = components(dir.path());
    let a = create(dir.path(), &genomes[0].id, "Original").unwrap();
    let path = dir.path().join("assemblies").join(format!("{}.json", a.id));
    let bytes = fs::read(&path).unwrap();
    assert_eq!(load(dir.path(), &a.id).unwrap(), a);
    assert!(
        dir.path()
            .join("genomes")
            .join(format!("{}.json", genomes[0].id))
            .is_file()
    );
    assert_eq!(
        create(dir.path(), &genomes[0].id, "Ignored rename").unwrap(),
        a
    );
    assert_eq!(read_name(dir.path(), &a.id).unwrap(), "Original");
    rename(dir.path(), &a.id, "My arrival brain").unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(read_name(dir.path(), &a.id).unwrap(), "My arrival brain");

    let b = copy_with_component(dir.path(), &a.id, &genomes[1].id, "Copy").unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(load(dir.path(), &a.id).unwrap(), a);
    assert_eq!(a.definition.brain.controller, genomes[0].id);
    assert_eq!(b.definition.brain.controller, genomes[1].id);
    assert_eq!(b.definition.brain.contract, a.definition.brain.contract);
    assert!(copy_with_component(dir.path(), &a.id, &genomes[0].id, "Duplicate").is_err());
    assert_eq!(catalog(dir.path()).unwrap().entries.len(), 2);
    assert!(rename(dir.path(), &a.id, "\n").is_err());
}

#[test]
fn saved_assembly_reproduces_exact_practice_inputs_and_results_for_both_cases() {
    let dir = tempfile::tempdir().unwrap();
    let component = components(dir.path()).remove(0);
    let assembly = create(dir.path(), &component.id, "Trial brain").unwrap();
    for preset in [Practice::Stationary, Practice::Moving] {
        let reference =
            genome_library::practice(&dir.path().join("trials"), &component, preset).unwrap();
        let trial = practice(dir.path(), &assembly.id, preset).unwrap();
        assert_eq!(trial.episode, reference.episode);
        assert!(!trial.episode.trace.is_empty());
        rename(dir.path(), &assembly.id, "Renamed between trials").unwrap();
        let repeated = practice(dir.path(), &assembly.id, preset).unwrap();
        assert_eq!(trial.receipt, repeated.receipt);
        assert_eq!(trial.episode, repeated.episode);
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(trial.receipt).unwrap()).unwrap();
        assert_eq!(receipt["assembly"]["id"], assembly.id);
        assert_eq!(receipt["episode"]["brain"]["controller"], component.id);
        assert_eq!(receipt["episode"]["runtime"], trial.episode.runtime);
        assert_eq!(receipt["episode"]["case"]["name"], preset.case().name);
        assert_eq!(receipt["episode"]["trace"], serde_json::json!([]));
    }
}

#[test]
fn missing_corrupt_and_incompatible_dependencies_never_substitute_a_controller() {
    let dir = tempfile::tempdir().unwrap();
    let mut custom = components(dir.path()).remove(0);
    custom.genome.layers[0].biases[0] += 0.125;
    custom = genome_library::Entry::new(custom.genome, custom.origin).unwrap();
    genome_library::save(&dir.path().join("genomes"), &custom).unwrap();
    let assembly = create(dir.path(), &custom.id, "Custom").unwrap();
    let path = dir
        .path()
        .join("genomes")
        .join(format!("{}.json", custom.id));
    fs::remove_file(&path).unwrap();
    let found = catalog(dir.path()).unwrap();
    assert!(
        found.entries[0]
            .unavailable
            .as_ref()
            .unwrap()
            .contains("Missing guidance genome")
    );
    assert!(practice(dir.path(), &assembly.id, Practice::Stationary).is_err());
    fs::write(&path, b"broken").unwrap();
    assert!(practice(dir.path(), &assembly.id, Practice::Stationary).is_err());

    let bundled = components(dir.path()).remove(0);
    let good = create(dir.path(), &bundled.id, "Bundled").unwrap();
    let bundled_path = dir
        .path()
        .join("genomes")
        .join(format!("{}.json", bundled.id));
    fs::write(&bundled_path, b"corrupt bundled artifact").unwrap();
    assert!(practice(dir.path(), &good.id, Practice::Stationary).is_err());
    assert!(genome_library::resolve(&dir.path().join("genomes"), &bundled.id).is_err());

    let mut wrong = components(dir.path()).remove(1);
    wrong.genome.contract.organism = "different_vehicle_v1".into();
    wrong = genome_library::Entry::new(wrong.genome, wrong.origin).unwrap();
    genome_library::save(&dir.path().join("genomes"), &wrong).unwrap();
    assert!(create(dir.path(), &wrong.id, "Incompatible").is_err());
    assert!(copy_with_component(dir.path(), &assembly.id, &wrong.id, "Incompatible copy").is_err());
    assert!(!dir.path().join("assembly-trials").exists());
}

#[test]
fn execution_contract_is_independent_of_mutation_architecture() {
    let dir = tempfile::tempdir().unwrap();
    let mut entry = components(dir.path()).remove(0);
    entry.genome = ship_arrival::neutral_genome();
    entry = genome_library::Entry::new(entry.genome, entry.origin).unwrap();
    assert!(
        !entry
            .genome
            .same_architecture(&crate::ship_training::template())
    );
    genome_library::save(&dir.path().join("genomes"), &entry).unwrap();
    let assembly = create(dir.path(), &entry.id, "Other architecture").unwrap();
    assert!(
        catalog(dir.path()).unwrap().entries[0]
            .unavailable
            .is_none()
    );
    assert!(practice(dir.path(), &assembly.id, Practice::Stationary).is_ok());
}

#[test]
fn unsupported_hosts_missing_capabilities_and_undeclared_cases_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let component = components(dir.path()).remove(0);
    let base = Definition::arrival(&component.id);
    let mut wrong_host = base.clone();
    wrong_host.host = "future_tank_host_v1".into();
    let mut missing_capability = base.clone();
    missing_capability.required_capabilities.pop();
    let mut wrong_target = base.clone();
    wrong_target.brain.target_source = "another_target_v1".into();
    for definition in [wrong_host, missing_capability, wrong_target] {
        let unsupported = Artifact::new(definition).unwrap();
        save(dir.path(), &unsupported, "Unsupported").unwrap();
        assert!(practice(dir.path(), &unsupported.id, Practice::Stationary).is_err());
    }
    let entries = catalog(dir.path()).unwrap().entries;
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.unavailable.is_some()));
    let mut definition = base;
    definition.practices = vec![Practice::Stationary];
    let stationary = Artifact::new(definition).unwrap();
    save(dir.path(), &stationary, "Stationary only").unwrap();
    assert!(practice(dir.path(), &stationary.id, Practice::Moving).is_err());
    assert!(!dir.path().join("assembly-trials").exists());
}

#[test]
fn corrupt_behavior_is_excluded_but_damaged_names_do_not_replace_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let component = components(dir.path()).remove(0);
    let assembly = create(dir.path(), &component.id, "Intact").unwrap();
    fs::write(metadata_path(dir.path(), &assembly.id), b"bad name").unwrap();
    let found = catalog(dir.path()).unwrap();
    assert_eq!(found.entries[0].artifact, assembly);
    assert!(found.entries[0].unavailable.is_none());
    assert_eq!(found.warnings.len(), 1);
    rename(dir.path(), &assembly.id, "Repaired name").unwrap();
    let path = dir
        .path()
        .join("assemblies")
        .join(format!("{}.json", assembly.id));
    let mut changed = assembly.clone();
    changed.definition.brain.coordinator = "another_coordinator".into();
    // Even a recomputed outer checksum cannot authorize a different behavior ID.
    let corrupt = encode(&changed).unwrap();
    fs::write(&path, &corrupt).unwrap();
    let found = catalog(dir.path()).unwrap();
    assert!(found.entries.is_empty());
    assert_eq!(found.warnings.len(), 1);
    assert!(practice(dir.path(), &assembly.id, Practice::Stationary).is_err());
    assert!(create(dir.path(), &component.id, "Do not overwrite corruption").is_err());
    assert_eq!(fs::read(path).unwrap(), corrupt);
    assert!(load(dir.path(), "../outside").is_err());
    assert!(genome_library::resolve(&dir.path().join("genomes"), "../outside").is_err());
}
