# Local publication of historical flag surveys

The [geometry investigation](capture-flag-geometry.md) found two completed
walking surveys rejected by motion outside every diagnostic envelope. This
slice records the queries actually issued and validates their local geometry
at publication. The broad circular check and its eight diagnostic envelopes
remain available for comparison; they no longer decide publication.

The result remains **observational**. It does not enter the v13 evaluator or
controller. A validated timestamp means that the historical landing/walking
measurement passed its one-time publication checks. It is not live permission
to land, exit, capture or board. Cover and opponent measurements keep their
source timestamp and are explicitly outside this validation. No sampled climb
is a swept flight, and no result certifies route optimality or negative answers.

## Captured dependencies

The existing charged landing check reports each material ray, capsule, raw hull
placement and hypothetical assembly test through an optional observer. Its
ordinary path uses a no-op observer. Captured full rays use the same normalized
direction as the engine; capsule bounds include their actual orientation;
raw hull bounds come from the exact queried collider shape at its proposed pose.
Both hatch probes, drift/tilt checks, unsuccessful queries and clearance cache
behavior are retained. The three climb queries use the same observer.

The 17-node walking job records all actual snapshot rays and capsules while
executing its existing steps. These include failed nodes and unused edges,
not only the selected path. Warm evidence predating capture makes the capture
incomplete. Invalid geometry, missing query classes or a 1,024-query capture
limit also withhold publication. No additional physics query or planning step
is used to reconstruct the discarded paths.

Each capture retains three optional unions: rays, capsules and hulls. Landing
uses three and walking uses two, so the current certificate contains five
source-frame AABBs. Each query contributes a fixed 0.004-unit margin; repeated
union does not accumulate that margin. ALL groups and no collider exclusions
are conservative choices: ignored actors or empty space between sampled poses
can still cause rejection. The five unions are narrower than the old circle,
but are not minimal query footprints.

The existing validator checks old and current shape overlap in their respective
planet frames, using generational shape identity, metadata/filter/sensor/enabled
state and the collider-extent 0.002-unit motion bound. Replacement with the same
semantic ID still invalidates evidence. Unsupported intersections reject
conservatively. The existing 8,192-body/collider capacities and 30-second source
age remain; publication never renews that age.

## Dependencies outside collision queries

At the existing Ready publication boundary, validation also checks:

- The actual job's source objective, material revision, planet radius and
  current flag endpoint range, including the strict range predicate inside
  objective-matching tolerances.
- Finite source/current gravity and the builder's current rise threshold for
  every positive walk in the small patch. Even the walking branch reads gravity
  before it tests eligibility. The exact edge predicate replaces a generic
  scalar-change tolerance here: walking rays/capsules and the no-jump hull
  overlay have no other gravity dependency. Changed negative edges or a newly
  better path remain outside the published claim.
- The raw live hull's immutable shape/filter identity, the current ship's
  hypothetical hull/feet used by the landing check, and the replacement ship's
  hypothetical assembly used by the walking overlay. Assembly specs compare
  local transforms, shapes and groups independently of world pose.
- The aboard ship/pilot state and the exact other-live-vehicle body-center
  distance predicate at the source landing pose transformed into the current
  planet frame. An empty or sensor-only vehicle still participates in this rule.

New failures occur only at publication. They do not change when the existing
job is cancelled, its source queries, scheduling or per-tick allocation.

`FlagSurveySample.validation` names `captured_query_unions_v1` and records the
source inputs, full area unions, capture counts, predicate failure and area
validation accounting. `geometry` continues to explain a rejection of the
**old circular gate**, including when the local result is accepted.
`local_rescued` and `local_withheld` compare those decisions explicitly.
Capture counts exclude hypothetical assembly and historical cover queries;
those queries remain charged in the original physics-query ledger.

Capture arithmetic runs inside existing dispatch time. `local_setup_ms` records
retaining the non-query source inputs; `local_ms`/`local_area_tests` record
publication checks separately. These and the retained broad-circle diagnostics
are synchronous work outside graph/query fuel. Bounds on stored areas and world
size are not a whole-bot CPU or FPS guarantee.

## Predeclared engineering replay

`tools/validate-flag-local.py` replays the same five enabled conditions from the
prior diagnostic study: the recorded v10/v13 regression and both generated
v13/v13 worlds with quiet/three-second asteroid pressure. These are repeated
engineering cases, not fresh strength samples. The full plan is written before
execution and an existing output directory is refused.

Every run must finish under the same ten-minute deadline with exact full
controller traces, evaluator bytes, physical and mission outcomes, source
measurements/routes and per-tick work logs. Only publication reasons/timestamps
and new local validation evidence may change. Both old diagnostics and new
acceptance/withholding counts must reconcile against every raw sample.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/validate-flag-local.py \
  --reference target/capture-flag-survey/geometry \
  --out target/capture-flag-survey/local-final
```

The reference's complete commands and hashes are in
[capture-flag-geometry-v1.json](data/capture-flag-geometry-v1.json).
Preserve the source/binary hash and raw artifacts; use a new output directory
for subsequent investigations.

## First replay and gravity refinement

The first implementation at `aa75b71` retained an additional generic
`abs(current_gravity - source_gravity) <= 0.01` check. All five replays completed
with exact controls, evaluator, physical outcomes, measurements and allocations.
The [initial record](data/capture-flag-local-initial-v1.json) retained zero
positives: the two earlier regression positives and both generated candidates
were all withheld by this scalar cutoff even though their local geometry
passed.

An independent dependency review confirmed that the complete, no-jump/no-flight
publication branch needs the positive-walk rise predicate, not a scalar-change
cutoff. Ground rays/capsules and walk floor checks are gravity independent;
the hypothetical hull overlay uses gravity only for jump edges, which the patch
does not emit; the round-trip search uses no gravity. All positive base edges
are rechecked against the current rise threshold. The redundant cutoff was
removed without relaxing collision tolerance or changing any query.

A targeted regression accepts a walk at its rise limit, accepts a large gravity
decrease, and rejects an increase of 0.005 that crosses that limit. Finite
source/current gravity remains required. The same predeclared five conditions
are replayed below, rather than selecting only the rescued cases.

## Final replay at `207d07a`

All five runs completed with healthy physics, totaling 30.81 simulated minutes.
The [complete comparison record](data/capture-flag-local-v1.json) retains the
plan, commands, source/binary hashes, each full validation record and accounting.
All five retain exact controller traces, evaluator bytes, physical and mission
outcomes, original measurements/routes and per-tick work logs.

| Recorded condition | Previously published / now published | Rescued by local checks |
| --- | ---: | ---: |
| v10/v13 regression | 2 / 2 | 0 |
| World 0, quiet | 0 / 0 | 0 |
| World 0, three-second asteroids | 0 / 1 | 1 |
| World 1, quiet | 0 / 1 | 1 |
| World 1, three-second asteroids | 0 / 0 | 0 |

Both newly accepted cases are the earlier geometry rejections: planet 0/bearing
55 at source/completion ticks 5193/5950, and planet 1/bearing 6 at 3471/3814.
Their walking distances remain 0/0 and 0.379/0 units. The regression's two
retained positives also remain 0/0. These are adjacent flag/boarding cases,
not evidence for long walks, broad generated-world coverage or bot strength.
No measured route, endpoint, source tick or completion tick changed.

Each of the four completed candidates captured 40 live world-query footprints
and 418 walking world-query footprints, compressed into five conservative
unions. Hypothetical assembly checks and historical cover remain in the
original charged-query totals. Across all five runs, planner work is unchanged
at 2,199 graph operations and 3,444 physical queries. Maximum combined use is
four graph operations and 127 queries in one tick, within the shared 4/384 cap.

The four local publication scans perform 40 shape intersection tests in total;
their measured desktop time totals 0.0458 ms. Retaining non-query source inputs
totals 0.00418 ms across these runs. Capture arithmetic remains inside ordinary
dispatch time. Builds/tests ran concurrently, the sample count is small, and
this is not an isolated CPU benchmark or Pi FPS claim. The broad-circle scan
and diagnostic work remain present; this slice targets valid evidence, not
frame-rate optimization.

Local validation passed 100 physics tests, 485 scenario tests, 155 AI tests and
344 Python tests. Focused tests cover a 2.187-unit outward walk (return already
within boarding range), capture of both hatches and settling probes, physical
blockers in each query class, the +60 climb, a body-center-only vehicle conflict,
raw/current-preview/replacement geometry changes, endpoint range inside match
tolerance, exact rise-threshold changes, expiry, unavailable capture and long
collider rotation. Recording preserves the atomic measurement and query count;
the unrelated-motion fixture preserves every per-tick allocation.

Independent review found no correctness or blocking issue. Its suggested
within-tolerance gravity regression was added, and the walking dependency proof
was independently checked before removing the generic scalar cutoff. Formatting
and strict physics/AI Clippy passed with the known physics
`collapsible_else_if` warning allowed. A broader strict scenario Clippy run also
reported existing warnings in unchanged code; it is not claimed clean.

The final native three-minute observer-off/on test also passed with identical
v13 controls, evaluator output and physical state. Runtime `207d07a` was deployed
to `sw-picade.local` using the app-only updater. The installed client hash is
`1c58a33d4bc5c7b18562f3fbf4c5e2dde45913661e74455fc27dc2422e1aef07`;
the CLI hash remains
`0d33f4d82a80df22cec0a56d74a3903d4fe05fe389100c2b174f752f4a4ca1f3`.
Both were verified on-device. The kiosk is active without automatic restarts,
retains P1 v10 / P2 v13 automatic matches, and exposes the new local-validation
telemetry. No bot brain or launcher setting was added or changed.

The next integration should define freshness/admission rules for this historical
enemy-alternative evidence in the value evaluator, using shadow comparisons
before a new control policy. The evidence here establishes a narrow valid
measurement path; longer walks, broader coverage and strategy improvements
remain unproven.
