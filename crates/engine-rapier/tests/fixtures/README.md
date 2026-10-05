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
