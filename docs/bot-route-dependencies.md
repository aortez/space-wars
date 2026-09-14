# Local dependencies for live objective routes

This continues the [ground-reuse adapter](bot-ground-reuse.md) from
[#90](https://github.com/aortez/space-wars/pull/90), within step 3 of
[#81](https://github.com/aortez/space-wars/issues/81). The opt-in
`live_joint_objective_v3` profile lets a coherent survey finish while unrelated
obstacles move, then checks the physical queries supporting each successful
outward and return route. The existing synchronous policies and live v1/v2
profiles remain available with their original behavior.

## What a local result means

A request still measures one immutable physics snapshot. After each candidate's
joint route search, a resumable job constructs conservative planet-local bounds
for the selected paths' floor rays and spaceling capsule queries. It includes
edge interiors, continuous walking support, and the elevated samples of each
directed jump. Return-path jumps are checked in their actual travel direction.
The ray establishing a retained footing needs its origin-to-first-hit segment;
buried geometry behind that hit cannot change the result.

Before publication, the adapter checks a circle enclosing all the survey's queries.
If it succeeds, the original full survey is still valid. Otherwise it checks
each successful route's areas against both snapshot and current colliders.
Entering, leaving, removed, replaced or differently filtered geometry can revoke
a route. Collider poses are compared in the original and current planet frames;
the collider's own extent bounds angular displacement. Actual shape/area
intersection avoids treating a hollow arena boundary's enclosing box as solid.
The full-survey shortcut uses this same check, so a long rotating collider cannot
bypass path validation through the older region-radius tolerance. Keeping this
region circular avoids unnecessary local checks for changes in unmeasured corners
of its enclosing box.
The existing exclusions for this pilot and its own ship still apply; each
proposed parked hull is handled by its separately rebuilt overlay.

Only surviving positive routes are published in this case, marked
`validated_routes_only`. Omitted candidates are **unknown**. Changed geometry
can create a previously unavailable route, so old failures and claims of current
optimality cannot survive on positive-path checks alone. Costs describe the
selected snapshot paths, whose supporting geometry has been revalidated. This
remains the existing sampled walk/jump feasibility model, not a guarantee of
physical execution or knowledge of future obstacle motion.

If no positive route survives, the adapter withholds the survey and restarts.
If an actual landed-hatch route was requested, that route must survive too.
A bot whose selected site disappears from a locally validated subset replans
without spending a failed-landing attempt. Current landing, movement, transfer
and interaction checks remain authoritative.

## Reuse, lifetime and work limits

The same objective, actor and material revision, actual hatch pose, gravity
tolerance and 120-tick source-age limit still gate a request. Moving obstacles
no longer cancel v3's pending work. Gravity/hatch changes restart it, rebuilding
jump measurements and hull overlays while retaining complete footing/walking
measurements from the **same original snapshot**. Such measurements are a
possibly stale hypothesis until publication passes the checks above.

Landing candidates are only sampled on scheduled updates. When a restart falls
between those updates, v3 can park the salvaged measurements until candidates
return. This fixes a previously observed loss of partial work at that boundary.
Parked and active requests share the same capacity; a parked entry consumes no
dispatch work, keeps its original source age, and is released on expiry, changed
objective, actor loss/removal, missing observations or reset. It cannot keep a
second cancelled job alive or combine snapshots from different ticks. The v2
profile retains its original stricter geometry and restart behavior.

The runners still share 16,384 graph operations and 1,024 physical queries per
update by default. Building dependencies charges one graph operation per path
node/preceding edge, with at most nine capsule and three floor sample-position
calculations per step. These calculations perform no physics query. At most
2,048 area records are retained per candidate's round trip, at most nine
candidates per request, within the existing two-request runner capacity. These
are structural bounds, not allocator-byte measurements.

**Live dependency validation remains synchronous and outside that allowance.**
Reports count its shape/area intersection tests and elapsed time separately.
Collider scans and bounding-box rejection also cost time; intersection counts
alone do not represent all validation work. On-foot/recovery sensors and action
controllers remain synchronous. This profile does not establish a whole-bot
frame-time guarantee or change launcher/Picade defaults.

## Reproduction

```sh
cargo +1.89.0 test --locked --release \
  -p engine-core -p engine-rapier -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_flag_soak --example surface_mission_soak --features sensor-profile

target/release/examples/surface_flag_soak \
  --policy material_mission_v10 --seed 42 --seat 0 --offset -0.8 \
  --mode capture --jetpacks true --survey-landing true --landing-threat false \
  --edit none --expect complete --live-objective-planning true \
  --reuse-objective-ground true --objective-dependencies routes \
  --out /tmp/local-objective-flag

target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --trace true --trace-end-tick 36000 \
  --timing-csv true --measure-draw true \
  --seed 7725194555774358125 --asteroid-interval 3 --out /tmp/local-objective-asteroids

python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --baseline material_mission_v10 --candidate material_mission_v10 \
  --live-objective-planning --reuse-objective-ground --objective-dependencies routes \
  --seconds 180 --seeds 9216675843324634618 --out /tmp/local-versus-synchronous-v10
```

Use `--objective-dependencies region` with ground reuse for retained v2, or omit
reuse for v1. The comparison tool applies live settings only to the candidate
role; this is not an equal-budget comparison of entire bots.

The reference is `642cc6d`, the head of #90. Exact binaries, source patch, commands,
allocation traces, reports and the validation script are saved under
`/home/oldman/.codex/visualizations/2026/09/13/bot-survey-dependencies/verified/`.
Completed dense traces are compressed as `trace.jsonl.gz`. The preceding v2
checkpoint remains under `bot-survey-reuse/`.

## Validation checkpoint

All 709 relevant release tests pass. The tests cover unrelated motion, entering
and leaving a route, inserted/removed/replaced colliders, filters and rigid frame
motion, rotation around distant collider origins, and a return jump obstructed
at its apex while the endpoints and outward walk stay clear. Adapter tests also
cover positive-only publication, withholding stale failures, parked-cache
capacity and lifetime, cloned continuation, source release, and selected-route
revocation without exhausting the landing retry budget. Fifteen live-adapter
tests also pass in debug mode with CI's 16 MiB test-thread stack. Client
all-targets checking and Clippy pass; Clippy reports warnings in unchanged code.

All six physical capture fixtures (both seats, query allowances 512/1024/2048)
capture, board and depart with physical audits passing. At the default allowance,
both seats publish locally validated positive routes on 15 updates. Their
physical-query totals are 977,714 and 957,039, compared with v2's 1,083,943 and
1,119,827. Repeating seat 0 preserves the entire non-timing report and allocations.

For seed `9216675843324634618`, two live bots complete a ten-minute generated
match without asteroids under the same shared allowance:

| Measurement | Ground reuse v2 | Local dependencies v3 |
| --- | ---: | ---: |
| Physical queries dispatched | 9,685,370 | 4,296,848 |
| Graph operations dispatched | 111,378,138 | 155,990,581 |
| Query snapshots built | 891 | 303 |
| Distinct forecasts published | 620 | 899 |
| Publication updates, summed over actors | 8,965 | 12,652 |
| Publication updates requiring local validation | 0 | 384 |

Survey queries fall **55.6%**, while graph operations rise **40.1%**. More surveys
progress through graph search, and path-dependency construction adds graph work.
This comparison combines local validation with retention across deferred
candidate refresh; it does not isolate either change. Different controls can
produce different trajectories and workloads. Both runs reach the time limit
with surviving pilots and the same 1–2 ownership score.

Local validation adds 1,596,182 shape/area intersection tests in the v3 quiet run;
the full-region checks add another 32,939 shape/circle tests. All dependency
validation takes 295.0 ms in aggregate over ten simulated minutes,
with a 2.82 ms maximum call on this desktop. That time is outside the dispatch
quota and included in sensors. The largest checked round trip has 2,018 area
records. There are 1,520 dispatches doing work for both bots; every combined and
per-actor allocation respects the shared allowance, and retained capacity stays
at two. The longest request-to-first-publication delay is 31 ticks; the oldest
source at first publication is 109 ticks. Source age never renews on reuse.

The quiet repeat preserves all 72,000 dense player records, non-timing reports
and allocations. Retained v1/v2 capture cases also preserve their previous
non-timing reports and allocations; the retained v2 ten-minute duel preserves
72,000 player records, and the synchronous v9/v10 match preserves 49,812.

The asteroid run (`7725194555774358125`, strikes every three seconds) submits 56
requests, reuses measurements in 33, and reaches 11 completed surveys with only
negative routes. It withholds all 88 entries in those stale surveys and publishes
no forecast. It performs 315,763 survey queries and 3,258,157 graph operations,
compared with v2's 18 immediately invalidated requests and 18,432 queries. The
match now lasts until the ten-minute time limit with both pilots alive and a
0–2 score; v2 ended on pilot death at tick 26,955. Changed pending/stale timing
affects controls even when no forecast publishes, so this outcome is not evidence
that a new route was used or that v3 is a stronger bot.
Its repeat preserves all 72,000 player records, non-timing reports and allocations.

A three-minute seat-swapped comparison against synchronous v10 gives one candidate
loss and one unfinished match. It exercises role-specific configuration and
physical execution, not an equal-budget strength estimate.

Candidate mission binary SHA-256:
`54fc980f8478e53208f6f24806841a3f74bd2fa384ebe26863df510fb99693e4`.
Reference mission binary SHA-256:
`f1f0d1ca2456ce709f4758e7ac1fe223026f000df58dd9650394e4764d768172`.

After #90 and the compact HUD in #91 merged, this work was rebased onto main
`b936ec7`. All 713 relevant release tests and client all-targets checking pass
there. A rebuilt ten-minute quiet run preserves the 72,000 player records,
non-timing reports and planning allocations from the checkpoint above; rendering
statistics are excluded from that comparison. The rebuilt binaries and replay
are in the sibling `post-rebase/` artifact directory. The timing figures above
remain tied to the explicitly pinned pre-HUD binaries.

These are desktop headless measurements. This profile has not been deployed or
timed on a Picade.

## What the asteroid investigation established

The saved v2 asteroid run cancelled all 18 requests before publication. Temporary
collider diagnostics identified two detached terrain fragments in 16 of those
cancellations, and another debris body in the other two. The fragments were
roughly 122 units from the center of a planet with nominal radius 128, inside
the whole-survey validation region. See `diagnostic.log` and `diagnostic.patch`
in the parent artifact directory; that logging is not part of production code.

Allowing pending surveys to continue first exposed gravity restarts, then the
loss of partial measurements while waiting for the next candidate survey.
Retaining the same snapshot across those boundaries gets some jobs through all
candidates, but this reproduction still publishes no route forecast: completed
surveys have only negative routes, and changed geometry prevents certifying
those failures. A separate controlled moving-collider fixture demonstrates
publication through unrelated motion and revocation when the collider enters a
selected path; it freezes other bodies to isolate dependency behavior. The
physical capture runners keep normal physics and also exercise local publication.

Do not interpret a changed match outcome, more finished jobs, or more publication
updates as an established improvement in bot strength. Investigate the remaining
negative routes by recording their start/destination/disconnection diagnostics
and comparing them with the actual on-foot/jetpack route available at that tick.
The objective forecast currently searches walk/jump routes only. A valid jetpack
crossing or a different landing candidate may require different evidence.
Gravity tolerance and source lifetime were not relaxed to make the counts rise.

## Next boundary

The [follow-up route-failure investigation](bot-objective-route-failures.md)
compares all 88 failed candidate checks with fresh measurements and exercises
cloned physical continuations. It identifies a successful capture/return over a
parked ship using the existing jetpack controller, while every current landing
site still fails the walk/jump-only forecast. This makes a measured prospective
vehicle crossing the next bounded behavior slice.

Keep this profile optional while adding and checking that flight evidence.
Local checks deliberately permit feasible snapshot routes without establishing
current best-route costs.
If validation cost becomes significant, share changed-collider classification
across candidates or schedule that work explicitly before broadening sensor
coverage. Mission-level utility and clock-aware strategy remain subsequent steps.
