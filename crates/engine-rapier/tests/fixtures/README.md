# Frozen packing regression

`scorched-angular-pile.packing` contains a 64-grain decision captured from
Scorched Earth's seed-42 Angular run at tick 7,200, after 60 seconds of barrage
and 60 seconds of quiet, with collapse enabled. The runtime before the packing
change is `b1c6a58` (PR #171). There were 163 surviving loose grains.

The old planner's three successive groups contained 64, 39, and 13 grains, and
accepted none. Growth was blocked by a tank and by grains excluded from each
transaction. The new bounded alternatives accept 22 grains in this frozen case
without intersecting either. The regression checks unchanged inputs, repeatable
inspection, vacant destinations, retained material/durability, and clear final
surface additions.

This fixture isolates obstacle-aware packing: it has no repose constraint. Its
format was upgraded to version 2 with that optional field set to None. New live
captures include repose samples when collapse is enabled. The full Scorched
Earth quiet-tail test separately guards against release/deposit cycles with
both grain shapes and the repose gate active.

Replay and draw the case using the `packing_inspector` example documented in
`docs/shared-loose-terrain.md`. Keep this fixture when changing the planner so
before/after decisions are compared against identical geometry.

`scorched-round-repose.packing` and `scorched-angular-repose.packing` retain
the same seed, duration and barrage settings from runtime `5d02c6d` (rebased
without physics changes as `e0a262a` in #174). They include the actual gravity
samples and default 50-degree repose constraint. Round is the second captured
group, 34 of 63 surviving grains; Angular is all 48 surviving grains. Both
previously accepted zero after using all eight surface checks. Recovery that
builds supported cells first accepts 32 and 48 respectively, within the same
movement and surface-check limits. The tests check stable, clear additions and
retained per-grain material/durability. Whole-scene outcomes can still differ
because earlier placements change later impacts and contacts.

`scorched-angular-actor.packing` is the 50-grain group beside tank 11 from the
subsequent supported-frontier runtime (the source/binary manifest is in
`docs/data/dirt-stability-20261004.json`). It uses the same seed-42, 60-second
barrage plus 60-second quiet tail, and includes repose samples. That runtime
accepted zero; removing only tank 11 from the frozen query accepts all 50.
This proves that the tank affects the decision, not that the pile is impossible
to pack with the tank present. The test guards read-only comparison, exact omitted
identities, invalid omissions, clear stable additions and unchanged replay.
Use `packing_inspector --omit-body 11` to reproduce the comparison.

`spacewars-angular-combat-0.packing` and `spacewars-angular-combat-1.packing`
capture the 27- and 37-grain groups from seed 42 after 26 seconds of the existing
combat benchmark, with Angular grains and collapse enabled. Their source/binary
identities are in `docs/data/dirt-performance-20261006.json`; reproduce with that
runtime's `loose_terrain_benchmark --scene combat --mode angular --seconds 26
--slumping --packing-dir target/combat-frozen`. Both bounded queries accept zero,
with 15 and 24 inspection attempts respectively (candidate rejections as well as
surface checks). They exercise repeated shrinking memberships on block terrain,
which previously caused especially long updates. Use `packing_inspector
--profile-repeats 20` to compare query cost against identical geometry; timings
are diagnostic and are not asserted by tests. Before/after reports must agree,
including obstacles, attempted assignments, and the final rejection reason.

`spacewars-round-curved.packing` captures the 57-grain group after tick 292 of
the seed-42 Weak/Round curved cannon fixture, using the gravity integration
runtime recorded in `docs/data/dirt-gravity-20261008.json`. The next simulation
tick exposed a large packing spike. The frozen query accepts 56 grains across
eight surface assessments. It retains sampled planetary gravity, actor and
unselected-grain obstructions, and the interpolated surface. The regression
checks stable, clear additions against those captured gravity samples and
retains each grain's cell material and durability. Reproduce with:

```sh
cargo +1.89.0 run --locked --release -p scenario-spacewars \
  --example destruction_benchmark -- --curved --strength weak --shape round \
  --seconds 5 --packing-tick 292 --packing-dir target/curved-packing --replay
```

`spacewars-round-roof-actor.packing` is group 2 after 30 seconds of the same
runtime's Weak/Round roof fixture. Its nine grains target a detached roof piece.
The bounded query accepts zero with ship 10000 present, and three when that ship
is omitted from the frozen query. The regression retains the obstruction and
read-only omission behavior. This demonstrates an actor's effect on the current
planner, not that no alternative packing arrangement exists. Capture with
`destruction_benchmark --roof --strength weak --shape round --seconds 30
--packing-dir target/roof-packing --replay`.

`spacewars-round-roof-small.packing` and
`spacewars-angular-roof-small.packing` are groups 1 and 2 respectively from
the same 30-second Weak roof captures. They contain three Round and eight
Angular grains. A test-only exhaustive assignment search finds no legal
deposit with the captured world intact. Removing ship 10000 in a copied query
permits at most two and four grains respectively; the bounded planner finds
those same counts. The Round control requires a coordinated pair: no singleton
fits. Regression tests retain all unselected grains as obstacles and check
unchanged snapshot bytes. See [the scoped cave search](../../../../docs/cave-packing-oracle.md)
for the ten-group survey, larger-group limits, and developer entry point.

Capture runs outside the step timer, verifies unchanged observation and motion
hashes, and does not advance quiet timers. Inspection enables recovery planning
and bypasses retry delays, so a frozen query is not a promise to pack on that
exact live tick.
