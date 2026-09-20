# Recorded trip timing and exposure

This is an initial calibration pass after [continuous return walking](bot-return-completion.md),
using runtime checkpoint `04253bc` and its `d6e5534` comparator. The ground score
underestimates the main recorded outbound walk, and it omits substantial landing,
claim and departure time. The existing exposure counter misses time on foot.
This checkpoint adds a repeatable offline analysis and regression fixtures;
runtime controls, scoring, deadlines and default policies are unchanged.

## What the current score represents

`LandingObjectiveRoute::cost` ranks a measured round trip. Dividing its score
by 38 gives the following real-valued time reference for each leg:

```text
length / 5 + jumps * 15 / 38
  + flights * max(slower_crossing_seconds + 4 - crossing_chord / 5, 0)
```

The crossing modifier replaces the walking time already counted for that chord
and allows a full recharge. It is not a forecast for the entire mission. The
score omits travel to the planet, circling/descent/settling, exit, claiming,
final boarding and departure. The landing selector has separate approach and
cover terms; those do not make the ground score a complete time prediction.

When there is no existing hostile flag objective, the selector adds no measured
ground-route term. A zero term therefore does not predict zero elapsed time.
Even a measured zero-length route can still require settling, a claim, boarding
and takeoff. Claiming normally needs one three-second stage for empty ground or
two for an enemy flag; interruptions can extend it.

## The paired production trip

Seed `15270103591317955068`, quiet, P1 v11 versus P2 v10, selection tick 10451:

| Phase | Previous walking | Continuous return walking | Selected ground-score reference |
| --- | ---: | ---: | ---: |
| Selection to arrival | 9.13 s | 9.13 s | Omitted |
| Arrival to landing | 22.75 s | 22.75 s | Omitted |
| Landing to exit | 0.017 s | 0.017 s | Omitted |
| Exit to first claim progress | 30.33 s | 30.33 s | 10.60 s |
| First claim progress to ownership | 5.98 s | 5.98 s | Omitted |
| Ownership to boarding acknowledgement | 28.80 s | 13.43 s | 10.12 s |
| Boarding acknowledgement to departure | 3.72 s | 3.73 s | Omitted |
| Whole attempt | 100.73 s | 85.38 s | No whole-trip prediction |

The same site 0:56 and route are retained through landing. Their source is tick
11010, first selected at 11029 and still retained at landing tick 12364. Seeing
that record every tick must not reset its age: the source is already 22.57
seconds old at landing. Material revision remains zero throughout both ground
legs. These are comparisons with the recorded choice, not a hindsight choice
of a faster landing.

The outbound estimate is 53.00 units at five units/second. The actual ground
task subsequently measures 54.93 units over 115 nodes, so the extra distance
does not explain a 30.33-second walk. Across its 1,820 ticks, 1,814 have balanced
support, none requests a jump, and no ground invalidation occurs. There are
1,681 partial directional inputs, 139 neutral inputs and no full directional
inputs; mean absolute input is 0.355. The task spends 27 ticks surveying and
1,792 in its walking goal. This supports investigating the existing proportional
slowdown at each outbound waypoint, which the previous change deliberately
left intact. The claim status `need_settle` is checked before flag proximity;
its many occurrences while moving do **not** mean the pilot spent that time
standing beside the flag waiting to settle.

The freshly observed return route appears at tick 14550, six ticks after
ownership, and measures 52.04 units. Its 10.41-second reference compares with
13.43 seconds through the actual boarding acknowledgement: a 3.03-second
residual, versus 18.39 seconds with the old follower. This interval includes
route acquisition and final boarding. The new return also spends some time
unsupported; its `jump` task label appears without any emitted jump command.
Keep posture/support measurements when testing further speed changes.

The complete ground interval is 49.75 seconds, compared with a 20.72-second
route-score reference. That residual includes claiming, so its ratio must not
be used as a walking-speed multiplier. Another 35.63 seconds occurs outside the
ground interval. A single constant added to every route would conceal these
different causes.

The faster-return match still loses at tick 24429 during pod recovery against
the sun; the old follower wins at 30127. This investigation preserves that
[recorded regression](bot-return-completion.md#fresh-matches-and-limits). It
does not establish better strategic play.

## Exposure and other attempts

The capture task increments `exposed_ticks` or `covered_ticks` only while the
pilot is aboard. Its condition uses an opponent within 300 units and no ground
occlusion from the **ship**. While the pilot is outside, that is geometry around
the parked ship, not visibility from the spaceling or evidence of enemy aim,
weapons, damage or a probability of death.

In the candidate's main trip, this ship-centered condition is positive for
24.82 seconds outbound, zero during claiming, 12.00 during return/boarding and
3.55 during departure. Of the complete ground interval, 36.80 seconds have both
an on-foot pilot and positive ship geometry. That time is absent from the
aboard-only counter. No target, unavailable queries or a missing ship are
unknown in the new analysis; they are not classified as cover. Measuring the
pilot's own visibility remains future sensor work.

Both seats of the three production recordings contain 38 attempts: 20 depart
and 18 abandon. These include repeated shared history, a matched comparator
and just one asteroid seed; they are not 38 independent calibration examples.
Most completed trips have little ground travel. Completed arrival-to-landing
times range from 15.60 to 32.13 seconds. Claim progress can last 11.43 seconds
and departure can take 16.07 seconds. The asteroid recording also has an
8.32-second return after selecting with no existing-flag route term. Preserve
those costs and failed attempts instead of averaging them into a walking rate.

The earlier controlled site 1:45 return provides another measured long leg:
23.06 seconds by route length versus 27.72 through boarding. It uses corrected
physics only from tick 18228 onward. Its outbound history remains old physics
and sparsely observed; it is explicitly excluded from current-physics timing
comparisons. This controlled completion is not a normal production replay from
startup. See the original [harness and limits](bot-return-completion.md#controlled-return).

## Repeatable analysis

[`tools/analyze-trip-costs.py`](../tools/analyze-trip-costs.py) needs only Python
3.10+ and the standard library. It reads existing version-2 mission reports
and version-1 trace rows. It executes no controller, physics step or new world
query. A manifest names the recordings and declares their runtime provenance:

```json
{
  "version": 1,
  "runs": [{
    "label": "quiet-v11",
    "directory": "quiet-v11",
    "source_commit": "04253bc478b764a298f57764ff262d569b3942c3",
    "physics_valid_from_tick": 0
  }]
}
```

Directories resolve relative to the manifest. Normal runs include both seats;
optional `seats` selects a subset. `scope: "continuation"` instead uses the
explicit trial's actor, nominated planet, milestones and stop, excluding later
resumed play. A controlled physics handoff must state its actual switch tick.

```sh
python3 tools/analyze-trip-costs.py \
  --manifest target/bot-trip-calibration/inputs.json \
  --out target/bot-trip-calibration/analysis
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

The JSON output keeps every attempt, phase boundary, censored duration,
prediction origin, source age, material revision, input hash and excluded
comparison reason. A small Markdown table provides a readable overview.
Interpretation rules matter:

- End an attempt at departure, abandonment, reselection or match end. Discard
  later milestones accidentally retained in that visit's metrics. A later
  successful capture cannot complete an earlier abandoned attempt.
- Exit and first claim progress come from observations. Sparse recordings give
  bounds; they do not imply that an unrecorded transition occurred at the next
  sample. A claim can start and reset inside a gap, so later inactive samples
  cannot rule out that earlier start. Missing intermediate boundaries remain
  unknown. An unfinished phase
  gives elapsed time to its stop, not a completed duration.
- Milestone times use telemetry acknowledgements. Claim progress first becomes
  visible one update into the stage, giving 2.98/5.98 seconds for uninterrupted
  nominal 3/6-second claims. Boarding acknowledgement can likewise lag the
  first aboard observation by one tick. No artificial correction is applied.
- Compare the last retained pre-landing estimate with execution; retain the
  first selected estimate separately. A later identical survey cannot become
  the original source of an already retained route. Missing source revision
  stays unknown. The formula is a reference for checkpoint `04253bc`, not a
  reimplementation of native route/crossing validity or a new permission check.
- Emit exact duration ratios only for completed, exact-boundary, densely
  observed phases using the declared physics and unchanged measured material
  revision. Record why other comparisons are excluded. Ground-interval ratios
  include claim time and are not walking calibration.
- Count only observed threat samples; missing ticks widen the possible time
  interval. The output labels ship geometry separately from unmeasured pilot
  visibility and never interprets either as a death probability.

For detailed timing, the existing soak runner accepts `--trace-start-tick 0
--trace-end-tick 36001` to record a full 600-second match densely. This is large
diagnostic output, so use sparse runs to find cases and dense replay to inspect
them. The analyzer also accepts ordinary sparse recordings.

## Validation and retained evidence

The three dense replays contain 132,714 rows. All 24,384 rows present in their
previous recordings match exactly, including both actors' observations,
telemetry and controls. Every non-timing report field except the requested
trace window matches; all 26,081 planning-work rows match after excluding wall
time and remain within graph/query quotas. Physics and material audits are
clean. The quieter candidate, old-walking comparator and asteroid candidate
keep their original outcomes and finish ticks.

Reanalyzing the prior sparse versions preserves all 38 attempt boundaries.
All 185 recorded milestone bounds and 250 phase-duration bounds contain their
dense counterparts. Sparse and dense versions are measurement checks of the
same executions, not additional independent matches.

The Python contract tests run in the existing Linux CI job. They cover route
score units, crossing replacement, unknown data, frozen source provenance,
sparse bounds, missing visibility, changed ground, old physics, unfinished
attempts, reselection and trace actor/tick integrity. This checkpoint changes
no Rust runtime code; the runs reuse the previously verified executables.

Evidence is retained in `target/bot-trip-calibration/` and archived at
`/home/oldman/.codex/visualizations/2026/09/19/bot-trip-calibration/` with binary
hashes, exact commands, dense recordings, verification scripts and analysis.
Its `baseline.json` identifies the inputs; `logging-verification.json`,
`sampling-verification.json` and `walking-measurements.json` preserve the checks.
Restore the preceding `bot-return-completion` archive alongside it for the
controlled recording and original sparse inputs. That dependency's SHA-256 is
`20bf7729fd7d7031fe1b13afd0339e321efd360a303afa76aaa9cb2ec6799614`.
The new archive records its own manifest, source patch and commit binding.

## Next bounded step

The [guarded outbound walking trial](bot-outbound-walking.md) now reduces the
main recorded outbound leg to 14.30 seconds and completes capture/return in
both physical test seats. Matched asteroid and quiet runs retain the comparator
and known regression seed. Support gaps and a small final settling cost remain
explicit in the measurements.

Once those execution costs are stable, compose a read-only complete-trip
estimate with separate approach/landing, outbound, claim, return/boarding and
departure components, plus uncertainty and evidence age. Add explicit on-foot
exposure coverage before using it to rank risk. Broader naturally activated
trips remain necessary before fitting timing constants or changing strategic
weights. No new rollout system, default promotion or larger task clock is
justified by this small sample.
