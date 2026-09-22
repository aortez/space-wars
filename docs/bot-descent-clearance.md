# Current-clearance descent estimate

The explicit offline candidate replaces the descent phase with a locally
supported estimate from current foot clearance and motion. Approach keeps the
[duration-first expiry rule](bot-approach-expiry.md); alignment, supported contact
and ground costs keep their previous models. No runtime controller, physical
permission, Pi deployment or strategic mission selection changes here.

## Model and evidence boundaries

`tools/estimate-descent-clearance.py` starts with the minimum of two valid foot
clearances divided by the native assist's target sink rate of two units/second.
It adds a local empirical residual for the remaining current descent episode,
then a separate estimate of the flight time after that episode until the
controller acknowledges landing. That later component can include further
contact, alignment or descent episodes; it is not a fixed settling constant.

Nearby training states must match current clearance, foot-height difference,
radial descent/lateral velocity, relative spin, radial angle, assist strength
and site-relative gravity components. Choose the nearest state from each
original attempt by normalized feature distance, then aggregate residuals and
later flight time using medians within worlds and across worlds. Durations do
not select neighbors. Long or densely sampled attempts cannot gain extra votes.

The support tolerances are respectively 4, 2, 2, 1, 0.5, 15 degrees, 0.2, 4 and
2 in the native units. A numeric result requires at least three attempts on two
worlds, within every measured feature domain. Initial and retry evidence stay
separate. Sampling is no denser than one second per episode, with at most 32
states per attempt/context and 2,048 states per context. A clearance bucket limits
the candidate scan; these are offline work bounds, not measured Pi timings.

Both rays must be finite, nonnegative and below the 26-unit no-hit sentinel.
The current local site/frame must be valid, the descent entry observed, native
contact must match the target with zero supported feet, and native assist must
be active with strength at least 0.5. Missing or unsupported evidence returns
unknown. This candidate deliberately has **no duration fallback within descent**,
so coverage losses remain visible. It cannot repair stale ground, renew a route,
reset capture/retry limits or authorize landing.

The [landing-tail investigation](bot-landing-tail.md) supplies 136 known
recordings on 25 worlds. These are now development/calibration data. Standard
phase qualification retains only observed completed descent episodes followed
by uninterrupted landing; later ground failure does not erase a landing.
Interrupted, censored and unobserved episodes and rejected states remain in the
profile's exclusions. Successful durations do not supply success probability.

Calibration retains **939 states: 762 initial from 142 attempts on 23 worlds,
and 177 retry from 33 attempts on 11 worlds**. In a development check withholding each world's samples from its
lookup, 728/762 initial states and 153/177 retry states remain supported. On
727 paired initial states, median absolute landing-time error falls from 0.57
to 0.10 seconds. At initial descent entry, 60/82 eligible training states have
support, with paired error 0.60 to 0.12 seconds. This check includes qualified
training states only; it is development evidence, not independent validation.
The rule was not retuned after this check.

## Frozen new-world comparison

The rule, all five profiles, helper sources, tests, generation/evaluation drivers
and native executables were hashed before generating seven new worlds. The
worlds exclude all 64 previously used seeds. The unchanged runtime remains
`43764580869c1ad56e6b7e9c4b7a21f485496220`.

- Sixteen normal v11-versus-v10 matches: four worlds, swapped seats, asteroid
  intervals zero and three seconds, at most 600 simulated seconds per match.
- Twenty-four controlled trials: three worlds, both seats, requested walking
  bands [2,6), [8,20), [20,40), [40,60), direction −1, fixed side-zero exit and
  offset +0.6, at most 180 seconds including physical setup. Failed setups stay
  in the denominator; this does not establish opposite-direction coverage.

The comparator is the previous duration-first approach estimate. Predictions
are written and hashed before loading the outcome evaluation. Independent-world
and recording checks apply to all five profiles. The original forecasts,
phase clocks, ground costs and native budgets are compared separately from the
new totals.

Primary observations are first-choice +0/+15 seconds and the first observed
descent entry in each attempt. Additional descent observations use the first
fixed one-second recording-grid point at least one or three seconds after that
entry; actual sample ticks are retained. An earlier-ended descent stays ended,
and an attempt without an observed descent stays unavailable. We never select
the final successful descent retrospectively. First-choice +30/+60/+120 seconds,
future phase breaks, failures and censoring remain separate diagnostics.

## Results and decision

All forty simulations finish and pass physical audits. They total **8,562.9
simulated seconds**, with **175,315 planner quota rows** within their graph and
query allowances. Normal matches retain 99 attempts: 48 completed, 50 abandoned
and one ending with the match; 29 never acquire a landing choice. Controlled
trials retain fourteen completed trips, four later frame departures and six
unavailable setups. A landing can still be measured before a later ground/frame
failure; landing and completed-trip denominators therefore differ.

The table compares whole-trip numeric coverage and paired exact **remaining
landing-time** error. Errors include later interruptions, and paired rows use
the same observations for both models. They are descriptive attempt statistics
across four normal and three controlled worlds, not confidence intervals.

| Scope / observation | Numeric trips, old → new | Paired landings | Median absolute landing error, old → new |
| --- | ---: | ---: | ---: |
| Normal, first descent entry | 49 → 15 | 15 | 0.65 → 0.30 s |
| Normal, descent +1 s grid | 33 → 38 | 30 | 0.50 → 0.22 s |
| Normal, descent +3 s grid | 28 → 33 | 27 | 0.45 → 0.14 s |
| Controlled, first descent entry | 15 → 7 | 7 | 1.28 → 0.50 s |
| Controlled, descent +1 s grid | 15 → 15 | 15 | 1.03 → 0.59 s |
| Controlled, descent +3 s grid | 14 → 14 | 14 | 0.93 → 0.39 s |

At descent entry, 34 normal and eight controlled numeric forecasts are lost.
Of those, 28 normal and six controlled fail the native-assisted/strength guard:
23 normal and five controlled have strength below 0.5; the other five normal
and one controlled still report `flying` despite stronger measured assist.
The native phase also checks thrust, so high assist strength alone does not
prove assisted descent. The remaining losses lack domain/local support. These
guards were not relaxed after seeing the results.

At the +1-second grid, six normal estimates are gained and one is lost; controlled
coverage is unchanged. The median paired landing error improves within each of
the seven new worlds at this checkpoint. The actual descent component has
0.10-second median error in normal runs and 0.45 seconds in controlled runs;
the post-descent component has 0.05 and 0.08 seconds respectively. These component
cohorts require an observed episode end and differ from the paired totals.

At first-choice +15 seconds, normal coverage changes **43 → 44** (two gained,
one lost). On 38 paired completed trips, median whole-trip error falls from
**1.08 to 0.37 seconds**. Controlled coverage stays 15; its fourteen paired
completed trips keep the same **2.29-second median**. All initial first-choice
forecasts are unchanged: this model cannot know future descent geometry.

Across 2,774 common recorded updates, the embedded comparator matches its
independent replay exactly. All 2,326 unaffected updates retain forecasts,
envelopes and budgets exactly; 448 common updates invoke the new descent model.
Selections, immutable first forecasts, phase clocks, ground evidence and native
budget limits also match. The 651 sampled descent lookups produce 435 numeric
landing estimates; maximum examined training states is 762, median 451.
This is an offline operation count, not a Pi frame-time measurement.

**Keep the component, but do not promote the strict replacement.** It improves
ordinary descent timing after motion settles, while discarding too much useful
early coverage. Next, test an explicit composition rule retaining the coarse
estimate where appropriate until the current-state estimate has support. Keep
hard evidence/permission guards distinct from a lack of statistical support;
freeze that selection rule before another independent comparison. No fixed
one-second handoff or relaxed support threshold has been chosen from this test.

## Retained failures and interruption evidence

`selected-cases.json` saves eight retrospective examples with exact ticks,
features, phase timelines and adjacent observations. `native-interruptions.json`
keeps counter evidence separate from site acquisition and physical landing.

- **New estimate with a large future interruption miss:** normal
  `world3-asteroids3-seat1`, selection **9677**, first-choice +15 at **11278**.
  The previous retry-duration estimate is unknown. The new landing estimate is
  **26.06 seconds short** after twelve later phase-clock breaks: **seven native
  airborne/not-yet-landed live-route replans and five site acquisitions**.
  At descent +3, tick **11458**, the miss is still 25.99 seconds despite valid
  rays, full assist, nearly upright attitude and ordinary descent speed. Physical
  state-conditioned timing does not predict future terrain interruptions.
- **Unsupported fast fall:** normal `world0-asteroids3-seat0`, selection **4066**,
  tick **5713**. Current descent speed is **14.35**, outside the initial training
  domain [−3.54, 9.25]; lateral speed is also outside its domain. The new estimate
  stays unknown. The old estimate is 4.52 seconds too long. This is a retained
  coverage loss, not a reason to widen the domain after validation.
- **Controlled regression with no later break:**
  `world2-band20-40-dir-1-seat1`, selection **345**, descent entry **1386**.
  Both models are supported, but landing error changes from **+0.27 to −1.11
  seconds**. The ship has only 0.29 downward speed and 1.11 lateral speed at
  entry. Local support does not guarantee improvement for every transient.
- **Ordinary entry withheld:** normal `world0-asteroids0-seat0`, selection
  **2438**, tick **4483**. Assist strength is 0.96 but the native state is still
  `flying`; the new model rejects it. The old landing estimate is only 0.37
  seconds short. This motivates retaining an appropriate early coarse estimate,
  rather than weakening the assisted-state check.

Across all active attempts, the diagnostic records 557 normal native replans
(seven already physically landed) and eleven controlled native replans (seven
already landed). It also retains 154 normal and sixteen controlled site
acquisitions without another native increment, including initial acquisition.
These totals cover failures as well as successful landings; subtype counters
overlap. They are not fitted interruption probabilities. The old normal
first-choice +15 miss of 52.31 seconds in whole-trip time also survives unchanged
outside the descent component. Interruption risk and remote transfer remain
missing inputs to strategic mission selection.

## Verification

All **257 Python tests pass**, including eighteen new tests for current rays,
motion/domain support, world/attempt weighting, residual composition, missing
evidence, immutable first forecasts, native budget guards, retry clocks,
terminal scope, calibration cadence and failed/interrupted training episodes.
Calibration reproduces byte-for-byte. No model/helper/profile changed after
the generation freeze, and no fresh result was used to refit this candidate.

## Reproduction

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-descent-clearance.py calibrate \
  --manifest target/bot-descent-clearance/training-inputs.json \
  --out target/bot-descent-clearance/reproduced-calibration
```

The retained `run-matrix.py` and `evaluate.py` bind generation and both estimator
replays. `analyze.py` audits unchanged comparator fields and produces fixed
checkpoint coverage/errors; `diagnose.py` retains native retry/contact evidence.
The development replay is `develop.py`. All evidence is under
`target/bot-descent-clearance/`, archived under
`/home/oldman/.codex/visualizations/2026/09/21/bot-descent-clearance/`.
The archive includes new recordings, all five profiles, drivers, outputs, tests,
source and patch. Earlier training recordings/diagnostics and native executables
remain hash-bound archive dependencies.
