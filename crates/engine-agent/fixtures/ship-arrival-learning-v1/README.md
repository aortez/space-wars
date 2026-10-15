# Retained arrival genomes

Final training-selected genome from each of the three declared runs of
[`material_arrival_learning_v1`](../../../../docs/ship-training-experiment.md).
All three are kept; `provenance.json` identifies the frozen executable, source
archive, run settings, and separate evaluation results.

Seed 3 reached all 11 development cases and is replayed by a deterministic
integration test. The test loads these weights and runs ordinary material
physics; it does not run evolution or depend on a random training improvement.
Seeds 1 and 2 preserve the weaker outcomes from the same recipe. These are
arrival-component artifacts, with no match-bot assignment or broad flight
qualification.

Try any artifact using `ship_training evaluate --policy model --genome PATH`.
The experiment's training command always starts with its declared random
population; these saved examples do not silently seed that recipe.
