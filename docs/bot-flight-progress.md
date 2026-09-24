# Flight approach and interruption investigation

The saved flight misses have two distinct causes: **different physical arrival
states** and **abandoned approaches**. Phase age alone cannot describe both.
The next estimator should use current geometry and motion for ordinary approach
time, while preserving interruption risk as a separate unknown.

This investigation adds `tools/inspect-flight-progress.py`, a read-only extractor.
It processes **56 existing recordings / 681,139 selected-seat observations**:
16 original flight-training matches, 16 latest normal matches and 24 latest
controlled trials. It produces 8,687 sampled diagnostic records. These are known
development recordings, not new validation. No fit, forecast, controller,
physics, runtime default or planning quota changes here.

## What the controller actually measures

The tactical controller's approach gate requires height below 15 units,
absolute sideways error below one unit, and frame-relative speed below four
units/second. Its ordinary descent command is capped at 12 units/second; lateral
correction, turning and braking add time. A private progress clock checks for
at least 0.5 units of distance reduction over each one-second window, and can
retry after ten seconds without such progress. That is different from spending
ten seconds in the approach phase.

The source is `crates/spacewars-ai/src/tactical_sortie.rs`, around the
`TacticalGoal::Approach` branch. Cover loss, solar avoidance and objective/terrain
changes can independently interrupt flight. The public native counters identify
these retry categories; `plan_generation` also counts site removal/reselection
and must not be interpreted as a number of retries.

`LandingTelemetry.altitude` is derived from foot rays. For material terrain it
returns 26 when no surface is found within the ray range. It is not a usable
high-altitude approach distance. The new diagnostic instead uses the selected
site's **suggested ship origin**, `vehicle_position`, measured at the current
observation. The planet's conservative radius is not a substitute for that site.

For ship position `x`, site origin `s`, planet center `c`, translation `V`, spin
`w`, site normal `n` and right vector `r = (n.y, -n.x)`:

```text
height        = (x - s) · n
side_error    = (s - x) · r
frame_velocity_at_ship = V + w * (-(x-c).y, (x-c).x)
relative_velocity      = ship_velocity - frame_velocity_at_ship
closing_speed = -(x - s) · relative_velocity / distance(x, s)
```

Subtracting velocity at the ship removes the rotating frame's contribution.
Subtracting translation alone, or velocity at the landing site, does not.
Height is relative to the site's plane and can be negative on the other side
of a planet; it is neither terrain penetration nor foot clearance. Closing
speed is instantaneous, not a distance/speed ETA. It is null at zero distance.

The extractor retains phase/context/age, height, sideways error, distance,
normal/right/closing speeds, gravity components, frame spin and native retry
counters. A 61-observation queue measures one second of distance, height and
side-error change. Missing observations, invalid site evidence, phase/attempt
changes and material revisions break that window. A revision refresh does not
restart the existing phase clock. Missing geometry or velocity remains unknown;
missing optional gravity stays null. No additional physics queries are needed.

## The large normal miss is mostly abandoned plans

`normal/world3-asteroids0-seat0`, selection **1**, first choice **607**:

| Event | Tick | Evidence |
| --- | ---: | --- |
| Initial approach | 607 | Height 91.21; closing speed 17.42 |
| Cover retries | 864–1951 | Seven increments of `cover_replans`; no solar/live-invalidations increments |
| +15-second checkpoint | 1507 | Four cover retries already observed; three more follow |
| Final site selected | 1965 | Bearing 11; seven cover retries total |
| Final approach | 2071 | Height 58.94; sideways error 15.59 |
| Native landing acknowledgement | 3040 | No further retry |

The initial landing forecast is **17.72 seconds**, versus **40.55 seconds**
observed. There are **22.63 seconds** between the first and final site selections;
the final site's uninterrupted flight takes **17.92 seconds**. This decomposition
accounts for the **22.83-second** landing underestimate without attributing that
entire miss to ordinary descent dynamics. It does not prove a counterfactual
landing time for any rejected site. At +15 seconds, the remaining miss is still
7.74 seconds and the retry forecast has only two historical attempts supporting it.

Across normal checkpoints with both a numeric phase forecast and an observed
landing, posthoc error grouping gives:

| Checkpoint | No later phase break: n / median absolute landing error | Later phase break: n / median absolute landing error |
| --- | ---: | ---: |
| First choice | 21 / 3.10 s | 17 / 6.12 s |
| +15 s | 26 / 1.10 s | 4 / 4.98 s |

These are landing-component comparisons, including landings whose subsequent
ground trip does not complete. They are not the earlier whole-trip denominator
of 35 completed normal attempts. Future breaks are outcome labels only, never
inputs to the causal extractor or a demonstrated prospective risk predictor.
Not every interrupted attempt has a large error, and uninterrupted initial
errors still reach 11.30 seconds. Retry prediction cannot be reduced to a fixed
penalty inferred from this one case.

## Long approaches are not necessarily stalls

The frozen initial-approach cell has **25 episodes from four worlds**, lasting
**5.33–10.37 seconds**. Their entry heights span **39.04–100.53 units**. The latest
normal cohort's 21 uninterrupted initial approaches last 2.77–9.43 seconds,
whereas 11 controlled uninterrupted initial approaches last **7.73–16.82 seconds**
and start at heights **84.92–169.17**. The latter includes one successful landing
followed by an abandoned ground return. Controlled preparation supplies different
arrival states as well as different ground-route lengths; this is not evidence
that equally long approaches are common in normal matches.

In `controlled/world1-band2-6-dir-1-seat1`:

| Tick | Phase / age | Site height | Closing speed | Distance closed in preceding second |
| ---: | --- | ---: | ---: | ---: |
| 637 | Approach / 0 s | 169.17 | 0.55 | Unknown after phase change |
| 1018 | Approach / 6.35 s | 104.72 | 11.33 | 11.44 |
| 1318 | Approach / 11.35 s | 47.53 | 11.19 | 11.40 |
| 1646 | Alignment entry | 14.99 | 2.26 | Unknown after phase change |

At **1318**, the phase-duration model has no surviving calibration episode and
returns unknown. The ship is still making substantial progress. All **seven**
controlled +15-second support gaps in `initial/approach` eventually complete the
whole trip. Their diagnostic windows close **3.66–12.41 units** in the preceding
second, despite ages of **10.40–15.00 seconds**. This does not justify removing
the native stall guard or silently extending duration support.

An exploratory comparison finds physically nearby training states for all seven:
height within ten units, signed closing speed within three units/second and
sideways error within two units. These deliberately simple, posthoc bands yield
11–25 distinct training attempts per checkpoint. For the 1318 case, thirteen
training attempts have nearby states at phase ages **1–5 seconds**. This is
evidence that current state may recover useful support when entry age differs;
the bands are not a selected estimator and no prediction-accuracy improvement
is claimed. Gravity, turning state, history and world dependence still matter.

The other two controlled +15-second flight-support gaps are circling attempts
that eventually leave the controlled planet frame. Both show positive chord
distance closure at that checkpoint. Do not convert positive distance progress
into a probability of completion or use a straight-line metric in place of the
controller's directed circling arc.

## Height alone would miss velocity reversal

`controlled/world2-band40-60-dir-1-seat1` enters approach at **2368**, only
84.92 units above the target, but moving **away at 48.22 units/second**. The
training approaches' closing speeds range from −2.56 to +29.09. At 2548, after
three seconds, this ship is still 115.65 units away while its closing speed has
turned positive. The approach lasts **14.17 seconds** without a retry.

Similarly, `world1-band8-20-dir-1-seat1` enters at 418 moving away at 30.30
units/second; its approach takes 15.62 seconds. Distance alone would confuse
these arrivals with ships already descending. Signed velocity and a short
progress history must accompany any distance-based estimate. These examples
are outside the training entry-velocity range, so a new model must either gain
declared training support or retain an unknown initial forecast.

## Next bounded estimator change

1. Keep the frozen estimator as the comparator and implement an offline
   state-conditioned **approach** estimate. Separate initial/retry contexts;
   measure remaining approach and the later alignment/contact tail explicitly.
   Use height/distance, lateral error and signed velocity, with progress/gravity
   as candidate disambiguators. Age remains observable, not the sole support gate.
2. Require measured physical-state support and independent attempts/worlds.
   A bounded sample lookup or small calibrated formula is sufficient; there is
   no evidence here requiring search or a forward physics simulation. If turning
   cases remain ambiguous, evaluate heading/spin before widening support.
3. Keep circling, replan history, elapsed capture time and native budgets visible.
   Future interruption count is unknown. Do not add this case's spent time to
   every forecast, reset the trip clock, or borrow initial evidence for retries.
4. Declare development cohorts, freeze the candidate, then generate new worlds
   covering ordinary entries, high entries, receding arrivals and pressure.
   Compare identical checkpoints, coverage, failed/unknown attempts and timing
   errors with/without later interruptions. The existing worlds are now known
   diagnostics for that change, even where originally used as a holdout.

No runtime promotion follows from this investigation. Remote-transfer timing,
interruption/completion risk and unsupported powered routes still precede
strategic mission selection based on these costs.

The subsequent [approach-state prototype](bot-approach-state-estimate.md) now
tests this direction on forty new simulations. It recovers two expired-duration
forecasts, but loses initial coverage and regresses ordinary whole-trip timing
through the later flight tail. Keep it as an offline comparator; a narrower
duration/state selection rule is the next experiment.

## Verification and reproduction

All **191 Python tests pass**, including 13 new tests for moving/rotating frames,
missing/stale evidence, bounded contiguous history, revisions versus retries,
prefix invariance, source hashes and controlled scope. All 56 input trace hashes
match the previous evaluations. All **129 eligible checkpoint phase records**
exactly match the existing estimator's clocks. Geometry is available for 128;
the remaining checkpoint retains absent selected-site evidence as unknown.
This work runs no new simulations and makes no runtime performance claim.

```sh
python3 tools/inspect-flight-progress.py \
  --trace target/bot-walking-regimes/normal/world3-asteroids0-seat0/trace.jsonl \
  --seat 0 --out target/flight-progress-replay.jsonl
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

`target/bot-flight-progress/` retains `extract.py`, `design.json`,
`extraction.json`, commands, hash-bound samples/metadata, `analyze.py`,
`training-entries.json`, `approach-episodes.json`, `checkpoint-diagnostics.json`,
`unknown-at-15s.json`, `case-studies.json`, tests and the source patch.
`inspect-cases.py` preserves six physical timelines, including the fast controlled
comparison and the unsuccessful circling counterexample. Outcome joins are a
separate pass after causal output files are closed and hashed.

The evidence archive is under
`/home/oldman/.codex/visualizations/2026/09/21/bot-flight-progress/`. Its manifest
binds these small derived artifacts to the raw-recording dependency archives
`bot-phase-landing-estimate` (September 20) and `bot-walking-regimes` (September
21), without duplicating their large traces. Extract those dependencies and this
archive at the workspace root, then run `extract.py`, `analyze.py` and
`inspect-cases.py` to reproduce. Adjacent archive and commit metadata bind the
evidence to the local checkpoint.
