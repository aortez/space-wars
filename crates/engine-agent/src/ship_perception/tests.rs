use super::*;
use crate::ship_arrival;
use spacewars_ai::model::{DenseModel, shared::SharedModel};

fn parent() -> DenseModel {
    DenseModel::from_json(include_bytes!(
        "../../fixtures/ship-arrival-mixed-v1/seed-1.json"
    ))
    .unwrap()
}

/// A stationary and a moving target.
fn cases() -> Vec<ArrivalCase> {
    let all = ship_arrival::cases();
    vec![all[1].clone(), all[all.len() - 1].clone()]
}

fn raw_bits(episode: &ArrivalEpisode) -> Vec<Vec<u32>> {
    episode
        .trace
        .iter()
        .map(|tick| {
            tick.raw_outputs
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect()
        })
        .collect()
}

#[test]
fn existing_policies_reproduce_ship_arrival_exactly() {
    for policy in [Policy::Reference, Policy::Model(parent())] {
        for case in cases() {
            let expected = ship_arrival::run_episode(case.clone(), policy.clone(), true).unwrap();
            let actual = run_episode(case, policy.clone(), true).unwrap();
            assert_eq!(actual.episode, expected);
            assert_eq!(raw_bits(&actual.episode), raw_bits(&expected));
            assert!(actual.perception.is_empty());
        }
    }
}

#[test]
fn a_warm_started_child_flies_its_parents_episodes() {
    let parent = parent();
    let genome = perception::warm_start(parent.genome(), 4, 0.5, 3).unwrap();
    let child = SharedModel::new(genome).unwrap();
    for case in cases() {
        let expected =
            ship_arrival::run_episode(case.clone(), Policy::Model(parent.clone()), true).unwrap();
        assert_eq!(expected.outcome, ArrivalOutcome::Arrived);
        let actual = run_episode(case, Policy::Perception(child.clone()), true).unwrap();
        assert_eq!(actual.episode.brain.controller, child.id());
        assert_eq!(actual.episode.brain.contract, perception::contract());
        assert_eq!(
            actual.episode.brain_id,
            brain(&Policy::Perception(child.clone()))
                .identity()
                .unwrap()
        );
        assert_eq!(raw_bits(&actual.episode), raw_bits(&expected));
        let mut normalized = actual.episode.clone();
        normalized.brain = expected.brain.clone();
        normalized.brain_id = expected.brain_id.clone();
        assert_eq!(normalized, expected);
        assert_eq!(actual.perception.len(), expected.trace.len());
        for sensed in &actual.perception {
            assert_eq!(sensed.inputs.len(), perception::INPUTS);
            assert!(
                sensed
                    .inputs
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1.0)
            );
        }
        let untraced =
            run_episode(cases()[0].clone(), Policy::Perception(child.clone()), false).unwrap();
        assert!(untraced.perception.is_empty() && untraced.episode.trace.is_empty());
    }
    // Each policy must carry its own observation contract.
    let mut wrong = child.genome().clone();
    wrong.contract.observation = "arrival_local_9_v1".into();
    let wrong = SharedModel::new(wrong).unwrap();
    assert!(run_episode(cases()[0].clone(), Policy::Perception(wrong), false).is_err());
}
