# Retained mixed-arrival genomes

Final training-selected genome from each declared run of
[`material_arrival_mixed_learning_v1`](../../../../docs/ship-mixed-training-experiment.md).
All three runs started with the same retained stationary genome and different
evolutionary random seeds. `provenance.json` records the parent, frozen runtime
and source, settings, artifact hashes, and separate evaluation scores.

Every finalist passed 12/12 training, 11/11 original, 96/96 development, and
48/48 fresh cases. The training set overlaps the original/development sets;
the full gate covers 157 distinct physical starts. No finalist was chosen using
evaluation results. Routine tests replay all three artifacts against this
frozen gate without running evolution or expecting random improvement.

Try any artifact using `ship_curriculum evaluate --policy model --genome PATH`.
The recipe's `train` command remains seeded from the original stationary
genome, not one of these descendants. These remain arrival-component artifacts
for one material build and fixture, with no live match-bot assignment.
