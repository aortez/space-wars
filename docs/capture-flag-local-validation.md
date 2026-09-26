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
