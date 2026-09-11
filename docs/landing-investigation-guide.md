# Resuming the deferred landing investigations

Parked on 2026-09-10 so work can proceed toward complete material matches.
The current implementation is `37106c6`, with verified results at `1ede0f2`.
See [the result report](objective-landings.md) for the complete measurements,
Pi image identity and controls. These failures remain open; physics audits
passing does not turn an incomplete sortie into a successful mission.

The later [match-pacing investigation](match-pacing.md) adds exact contact
records for damaging on-foot touchdowns and distinguishes them from lethal pod
collisions with wreckage or the world boundary. Use those match-rule cases when
investigating survival; the landing fixtures below retain their original rules.

## Evidence to preserve

The artifact directory is:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/objective-landings-20260910/
```

`final-{desktop,pi}-binaries.json` identifies the exact runners.
`final-source-files.json`, `trial-summary.json` and `final-report-manifest.json`
identify their source and 122 final trial reports. `pi-trials-before-deploy.tar.gz`
contains the remote recordings; `pi-archive-manifest.json` records its hash and
comparison against local files. Frame JSON files are renderer recordings, not
screenshots. `pi-live-{30,75,120,180}.png` are actual device screenshots.

A future source build can change the trajectories. Use the archived runner to
reproduce the old result first, then compare the new source with the same inputs.
For comparisons across checkouts, use separate Cargo targets: a shared target
previously reused stale same-named package artifacts. The rejected build is
recorded in `rejected-shared-cache-note.txt`. The old production baseline is
`7b57342`/`7726c09`, with only `baseline-fixture.patch` added for the launch trial.

## Cratered approaches

Start with this failing desktop case:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_flag_soak -- \
  --seed 42 --seat 0 --offset 0.8 --mode capture --jetpacks true \
  --survey-landing true --landing-threat false --edit crater \
  --expect complete --out /tmp/spacewars-crater-approach
```

At the checkpoint the command writes `report.json`, then exits with a failed
completion assertion. The cut is a real material edit two seconds after the
approach starts. It does not teleport the ship or grant a landing.

| Case | Current full sortie result on desktop and Pi |
| --- | --- |
| Seed 42, seat 0, offset +0.8, crater | Incomplete |
| Seed 42, seat 1, offset -0.8, crater | Incomplete |
| Seed 42, seat 1, offset +0.8, crater | Incomplete; Pi captures and physically departs but misses the tactical completion milestone |
| Seed 42, seat 0, offset -0.8, crater | Complete control case |
| Same four approaches, `--edit flag` | All complete |

Read `final-{desktop,pi}-objectives-edits/<case>/report.json`. Compare:

- `approach_started_tick`, `approach_landed_tick`, `exited_tick`, `claimed_tick`
  and `departed_tick` against `capture.completed_tick` and its failure/replans.
- `objective_surveys`: accepted site IDs, outbound/return failures, revision,
  and `actual` proof at the true touchdown pose.
- `samples[].pilot.flight.pilot`: accepted feet, settling, hatch, transfer
  result, ship pose, planet revision and ownership. Samples are once per second;
  a missing transient in them is not evidence it never happened.

The desktop failures retry blocked exits or unsettled touchdowns, then exhaust
accepted alternatives. The `survey-all-diagnostic-*` runner tested all 64 sites
and produced the same desktop failures. In the inspected failed layouts only
the two neighboring sites have accepted walking/jumping round trips. More
sampling alone is not the next hypothesis to repeat.

Next investigation: test whether prospective jetpack corridors provide a
sheltered alternative away from the damaged touchdown, with enough charge and
usable ground on both ends. Separately determine why the accepted near sites
lose their predicted hatch/support at actual touchdown. Keep those two causes
separate. A straight-line distance or a merely possible future flight must not
become a physical transfer permission.

## Exposed approaches and cover

Use the same command with `--edit none --landing-threat true`. A useful first
case is seed 42, seat 1, offset -0.8. Compare seeds 7/42, both seats and ±0.8.
The fixture's defender physically claims a flag and remains an obstacle; this
switch retains its combat target observation. It is a cover-planning experiment,
not a weapon duel. Generated asteroid duels exercise the combined combat loop.

`baseline-objective-desktop-objectives` records 8/8 captures but 0/8 full sorties.
`candidate7-desktop-objectives-hot` records 6/8 captures and 0/8 full sorties.
Those are the exposed exploratory builds, not the final 122-trial matrix.
Final material-revision handling changed afterward: rerun the exposed fixture
against the final/current policy before attributing exact old timings to it.

Inspect cover replans, chosen sites, exposure, retained surface routes and the
three-second clear-departure requirement. The return walk and departure can
consume the budget after a successful claim. Both the dropped captures and the
missing full completions matter. Keep sheltered alternatives in the shortlist;
choosing only the nearest sites previously caused repeated exposed approaches.

## Cramped hatch return under the hull

The original live failure occurs in generated seed 42, seat 0, unreflected,
with two bots and Mixed asteroid arrivals every three seconds. A current run is:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 42 --seat 0 --mirror false --mode duel \
  --asteroid-interval 3 --seconds 180 --frames true --trace true \
  --probe-ground-start true --out /tmp/spacewars-cramped-return
```

The final Pi policy avoids the previous trap and departs planet 2 at 108.68s.
To investigate the trapped state itself, use the archived reproductions:

- `candidate4-pi-rejoin/ground-start-probes.json`: six eight-second control
  probes from a cloned world. The other probe frames are in the Pi archive.
- `candidate8-pi-rejoin/generated-s42-p0-mFalse-duel`: physical recovery through
  scuttle, rebuild and resumed flight; completed recovery at 119.75s.
- `final-pi-matrix/generated-s42-p0-mFalse-duel`: final policy avoids that trap.

The actor can be balanced and physically supported while no upright node lies
within the graph's three-unit start radius. Inspect `posture.standing_clear`,
`crawl_clearance`, `crawl_floor`, actual capsule angle, nearest footing and
hatch distance. Short rightward inputs moved farther in the cloned probes;
short steps alone did not reliably free the tight trap. An early attempt also
issued movement only on half-second survey ticks; the current rejoin action
runs between surveys.

Current recovery bounds are eight seconds/six units for the escape attempt,
fifteen ticks for a retained short step, and the existing ninety-second ground
budget. Failed hatch return can use the ordinary scuttle/rebuild fallback only
with fresh confirmation. Regained footing/readiness or invalid evidence cancels
it. Do not loosen these bounds merely to make one archived trace finish.

## Code and regression checkpoints

Start in these files:

- `scenarios/spacewars/src/surface_sortie/landing_objective.rs`: proposed hull,
  candidate shortlist, both route directions and actual touchdown survey.
- `crates/spacewars-ai/src/tactical_sortie.rs`: route/cover scoring, revision
  revalidation, rejection and retry budgets.
- `scenarios/spacewars/src/surface_sortie/{ground_navigation,jetpack}.rs` and
  `crates/spacewars-ai/src/ground_task/jetpack.rs`: measured ground/flight edges.
- `ground_posture.rs`, `ground_task/rejoin.rs` and `recovery_task.rs`: cramped
  start evidence and replacement cancellation, under the directories above.

Run the objective, ground-navigation and surface-recovery contracts before the
focused physical reproduction. Then retain the eight quiet contested controls,
flag-footing edits, twenty equipped ground/recovery cases, and the 25-case
mission matrix on both architectures. Neutral capture/hunt is 12/13 prepared,
11/13 weapon contact, with 4/4 additional quiet itineraries. The long seed 7/P2
unreflected itinerary remains a whole-mission timeout; its clock was not extended.

Measure capture, boarding, physical departure and tactical completion separately.
Report objective-refresh sensor cost separately from overall p95: the twice-per-
second survey measured 14.62–19.95ms p95 on Pi, peaking at 21.46ms. Adding jetpack
forecasts must stay bounded and should be checked in the rendered Pi run.

Resume this work when complete matches repeatedly stall on defended/damaged
planets, or when prospective jetpack access becomes necessary for the next
mechanic. Preserve the successful ordinary loop while addressing a reproduced
failure; exhaustive landing perfection is not a prerequisite for match work.

The [fresh-world survey](fresh-world-survey.md) adds a focused ordinary-match
reproduction: seed `3121799525250095703`, asteroids Off, P2 surveys an unchanged
enemy objective for 88.73 seconds with no accepted walking/jumping round trip.
It finally defers Planet 0 at tick 10679 on both desktop and Pi. Start with
bounded no-route deferral before expanding the prospective route model. That
report also separates real pod/ground progress from stalls and preserves an
unsupported airborne recovery failure in seed `9908999338443660350`.
