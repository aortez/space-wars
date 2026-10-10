# Exhaustive checks for the quiet cave

The captured Weak roof piles do not justify a runtime packing change. Six
complete searches find no legal local deposit with the captured obstacles
present. Four larger searches find no legal deposit of one, two, or three
grains. Trapped grains can remain loose without implying a missed placement.

These are the same seed-42, 30-second Space Wars roof captures used in the
[packing preview investigation](packing-preview-reuse.md). Each query keeps
ships, detached roof pieces, and every unselected grain in the collision world.
It targets the original destination field with the captured gravity and repose
constraint. The new code is compiled only for tests; runtime packing and its
eight-assessment budget are unchanged.

## Search scope and results

The diagnostic enumerates grain membership and destination assignments
independently of the runtime matching, exclusion, and supported-frontier
heuristics. It scans the entire finite destination field, applying the same
1.75-cell reach and wall-crossing samples. Singletons retain the smaller
1.25-cell reach. Destinations must be distinct and connected to existing ground.
Deposits retain each grain's material and durability, pass captured-gravity
repose checks, and have clear exact surface additions. Surface reconstruction
and collision queries use the shared engine primitives.

Complete enumeration of every membership and assignment in six groups:

| Shape / group | Grains | Assignments checked | Stable assignments | Clear assignments |
| --- | ---: | ---: | ---: | ---: |
| Round / 0 | 7 | 626 | 200 | 0 |
| Round / 1 | 3 | 101 | 26 | 0 |
| Round / 2 | 9 | 33 | 13 | 0 |
| Angular / 0 | 14 | 124,905 | 6,225 | 0 |
| Angular / 2 | 8 | 548 | 133 | 0 |
| Angular / 3 | 2 | 0 | 0 | 0 |

Angular group 3 has no reachable vacant destination. Complete enumeration of
every deposit of up to three grains in the remaining groups:

| Shape / group | Grains | Assignments checked | Stable assignments | Clear assignments |
| --- | ---: | ---: | ---: | ---: |
| Round / 3 | 37 | 4,189,658 | 14,015 | 0 |
| Round / 4 | 8 | 21,190 | 0 | 0 |
| Angular / 1 | 22 | 533,420 | 4,235 | 0 |
| Angular / 4 | 10 | 42,772 | 0 | 0 |

Across the actual-world queries, 4,913,253 distinct assignments were checked;
24,847 passed support and stability checks, and none passed clearance. Counts
exclude duplicate destinations and the empty assignment. Larger-group deposits
of four or more grains are unproven. These results also do not cover another
simulation tick, another destination body, combined transactions across roof
pieces, grid expansion, or different reach/repose rules.

The oracle counts a conservative assignment bound before enumeration and fails
if it exceeds 20 million. It does not silently truncate a search and report
impossibility. This is an offline diagnostic, not a candidate for the live step.

## Actor-removal controls

Removing a ship from a restored query can create room for deposits:

| Shape / group | Omitted ship | Oracle maximum | Runtime accepted |
| --- | ---: | ---: | ---: |
| Round / 0 | 10001 | 3 | 1 |
| Round / 1 | 10000 | 2 | 2 |
| Round / 2 | 10000 | 3 | 3 |
| Angular / 0 | 10000 | 0 | 0 |
| Angular / 2 | 10000 | 4 | 4 |

These are independent counterfactual queries. No ship is removed in gameplay,
and the placements are not cumulative. Other obstacles remain in each query.

Round group 0 demonstrates that the bounded runtime planner is not globally
optimal: after removing ship 10001, it accepts one grain where the exhaustive
search finds three. With the ship present, the complete search finds none.
This limitation is recorded without expanding the live search budget.

The three-grain Round and eight-grain Angular captures are now checked-in
regression fixtures. Tests require zero clear assignments in the original
world, maxima of two and four after removing ship 10000, matching runtime
control counts, and unchanged input/world snapshots. The Round control also
guards a coordinated pair that cannot be replaced by a legal singleton.

## Reproduction and validation

Run the small checked-in Angular case through the ignored developer entry:

```sh
SPACEWARS_PACKING_SNAPSHOT="$PWD/crates/engine-rapier/tests/fixtures/spacewars-angular-roof-small.packing" \
  cargo +1.89.0 test --locked --profile ci -p engine-rapier \
  inspect_frozen_packing_exhaustively -- --ignored --nocapture
```

Set `SPACEWARS_PACKING_OMIT_BODIES=10000` for its actor-removal control. Set
`SPACEWARS_PACKING_MAX_GRAINS=3` for a larger capture's complete one-to-three-grain
search. Omissions are validated through the existing read-only inspector;
candidate grains and destination bodies cannot be omitted. Generate all five
groups per shape using:

```sh
cargo +1.89.0 run --locked --release -p scenario-spacewars \
  --example destruction_benchmark -- --roof --strength weak --shape round \
  --seconds 30 --packing-dir target/roof-round --replay
```

Repeat with `--shape angular` and another output directory. Captures bypass
quiet retry delays for inspection and do not promise a live deposit on that
exact tick. The fixture/source hashes and all 15 query results are recorded in
[the evidence file](data/dirt-packing-oracle-20261009.json).

The engine-terrain, engine-rapier, Scorched Earth, Space Wars, and Terrain Lab
suites pass **840 tests**. The environment-driven developer entry is ignored
by the normal suite and was run explicitly for the 15 survey/control queries.
Strict Clippy passes for all engine-rapier targets.

The two new regression tests also pass on the Picade. All 15 explicit survey
and control queries produce identical assignment counts and best placements
on desktop and Pi. This checks the diagnostic results across architectures;
the exhaustive search is not a gameplay performance benchmark.

This pass adds test-only code, fixtures, and documentation. The prior runtime
build and its desktop/Pi benchmark results remain applicable; no new runtime
performance improvement is claimed. Local Weak/Round and Weak/Angular material
duels provide a separate gameplay smoke check, not proof about every cave or
every possible packing arrangement.

The local review is left at a fresh, paused Weak/Angular duel. The installed
Picade client and CLI hashes still match the prior runtime build, so no new
application deployment was needed. Raw logs, source manifests, captures,
executables, and reproduction scripts are in
`target/dirt-partial-packing-review/`.
