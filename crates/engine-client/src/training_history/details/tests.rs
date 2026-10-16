use super::*;
use engine_agent::{
    brain_assembly,
    genome_library::{self, Practice},
    training_history::{self, Key, Kind, TrialRecord},
};
use std::path::Path;

fn read_trial(root: &Path, kind: Kind, id: &str, receipt: &Path) -> TrialRecord {
    let key = Key {
        kind,
        id: id.into(),
        receipt: receipt.file_stem().unwrap().to_str().unwrap().into(),
    };
    let Record::Trial(trial) = training_history::inspect(root, &key).unwrap() else {
        panic!("expected practice receipt")
    };
    *trial
}

fn measured(trial: &TrialRecord) -> Sample {
    pages(&Record::Trial(Box::new(trial.clone())), false)
        .remove(0)
        .sample
        .unwrap()
}

#[test]
fn component_and_assembly_receipts_share_their_execution_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let entries = genome_library::catalog(&root.join("genomes"))
        .unwrap()
        .entries;
    let assembly = brain_assembly::create(root, &entries[0].id, "Comparison fixture").unwrap();
    let mut protocols = Vec::new();
    for preset in [Practice::Stationary, Practice::Moving] {
        let component =
            genome_library::practice(&root.join("trials"), &entries[0], preset).unwrap();
        let assembled = brain_assembly::practice(root, &assembly.id, preset).unwrap();
        assert_eq!(component.episode, assembled.episode);
        let a = read_trial(root, Kind::GenomeTrial, &entries[0].id, &component.receipt);
        let b = read_trial(root, Kind::AssemblyTrial, &assembly.id, &assembled.receipt);
        assert_eq!(measured(&a).protocol.mismatch(&measured(&b).protocol), None);
        assert_eq!(measured(&a).value, measured(&b).value);

        // A different controller can be compared on the same physical task.
        let other = genome_library::practice(&root.join("trials"), &entries[1], preset).unwrap();
        let other = read_trial(root, Kind::GenomeTrial, &entries[1].id, &other.receipt);
        assert_ne!(a.brain.controller, other.brain.controller);
        assert_eq!(
            measured(&other).protocol.mismatch(&measured(&b).protocol),
            None
        );
        protocols.push(measured(&a).protocol);

        type Change = (&'static str, fn(&mut TrialRecord));
        let changes: [Change; 7] = [
            ("runtime", |t| t.runtime.push_str("/other")),
            ("case seed", |t| t.case.seed += 1),
            ("tick budget", |t| t.case.max_ticks -= 1),
            ("coordinator", |t| t.brain.coordinator.push_str("/other")),
            ("host", |t| {
                t.assembly
                    .as_mut()
                    .unwrap()
                    .definition
                    .host
                    .push_str("/other")
            }),
            ("organism", |t| {
                t.assembly
                    .as_mut()
                    .unwrap()
                    .definition
                    .organism
                    .push_str("/other")
            }),
            ("capabilities", |t| {
                t.assembly
                    .as_mut()
                    .unwrap()
                    .definition
                    .required_capabilities
                    .push("other".into())
            }),
        ];
        for (name, change) in changes {
            let mut different = b.clone();
            change(&mut different);
            assert!(
                measured(&a)
                    .protocol
                    .mismatch(&measured(&different).protocol)
                    .is_some(),
                "{name}"
            );
        }

        // An unknown recipe cannot borrow the current implicit host, even if
        // both receipt wrappers carry the same recipe string and brain.
        let mut unknown_component = a;
        let mut unknown_assembly = b;
        unknown_component.recipe = "unrecognized_arrival_v2".into();
        unknown_assembly
            .recipe
            .clone_from(&unknown_component.recipe);
        assert!(
            measured(&unknown_component)
                .protocol
                .mismatch(&measured(&unknown_assembly).protocol)
                .is_some()
        );
    }
    assert!(protocols[0].mismatch(&protocols[1]).is_some());
}
