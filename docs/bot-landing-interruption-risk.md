# Near-term landing interruption evidence

Recent native replans identify a more interruption-prone group, especially during
approach and circling. They do not establish inevitable failure, and a healthy
descent can still suffer its first live-route invalidation. The next small model
should condition on flight phase and recent replans, report its evidence and
missing coverage, and be tested on new worlds before affecting decisions.

This step adds `tools/inspect-landing-risk.py` and tests. It changes no timing
profile, controller, physical permission or Pi deployment. The
[combined descent estimator](bot-descent-regimes.md) remains the offline timing
baseline. This is a diagnosis of known recordings, not independent validation
or a fitted success probability.

## Population and prospective observations

The diagnostic binds **216 existing recordings on 39 worlds**: the original
136 landing-tail recordings, forty strict-clearance recordings and forty descent
composition recordings. Native runtime remains
`43764580869c1ad56e6b7e9c4b7a21f485496220`.

There are **624 observed attempts**: 289 completed, 283 abandoned, sixteen ending
with the match, sixteen controlled ship losses and twenty frame departures.
Normal matches contribute 535 attempts on 24 worlds; controlled trials contribute
89 on fifteen worlds. The 120 controlled trials retain all setup outcomes,
including thirty unavailable and four wrong-planet setups. Trial outcomes and
observed attempts are different denominators: some failed setups contain an
observed attempt. No source evaluation has an unobserved attempt.

Use one observation per attempt at first landing choice +0 and +15 seconds as
the primary checkpoints. +30/+60/+120 are secondary diagnostics. Ended attempts,
missing checkpoints and unavailable plans are counted, not replaced with later
successful phase entries. **212 attempts never acquire a choice** and remain
outside this local-plan forecast population.

The five-second history, ten-second future horizon and descriptive splits were
declared before aggregating the recordings. No thresholds were fitted. Features
use only snapshots and transitions at or before the checkpoint. They are written
and hashed before attaching outcomes:

- Current flight phase/context, native contact and supported feet.
- Native counter increments and terrain revisions in the previous five seconds.
  Incomplete observation/capture history is unknown, not zero.
- Current site-relative geometry and closing velocity. One-second progress
  compares measured endpoints on the same uninterrupted phase, site and revision.
  It does not assert continuous progress or query validity between those samples.
- The native landing-progress clock only during alignment, descent and settling.
  Its stale value during tactical approach/circling is unavailable.

The diagnostic validates each retained transition against its adjacent snapshots
and reconciles the observation count with explicit gaps. It checks source
manifest/evaluation/index hashes, runtime, run and attempt identities, lifecycle
records, derived trace hashes and report hashes. Raw trace hashes are inherited
from the prior audited dense replays; this pass does not reread the raw traces.

## What counts as an interruption

The positive endpoint is a native `replans` increment strictly after the
checkpoint, within ten seconds and before the first physical landing on the
attempt's target. Its preceding observation must have a matching native contact
frame and phase `flying`, `assisted` or `settling`. One or two supported feet do
not by themselves finish landing. This definition includes interruption of an
unfinished settle, so it is called *prelanding*, not necessarily airborne.

A prior native `landed` observation makes a replan a grounded event. Physical
landing stops this forecast's exposure even if controller acknowledgement is
later, the ship subsequently leaves the ground, or the later ground trip fails.
A landing and replan first appearing on the same tick are ambiguous. Site
acquisition without a replan increment is a separate event. Live invalidation
and objective-replan counters overlap and must not be added as independent causes.

An observation gap, changed capture, reset counter, unknown contact/replan or
short recording cannot manufacture a negative. Abandonment and ship loss before
the horizon remain separate attempt-ending endpoints. The source attempt's
exclusive ending boundary also prevents the next mission's event becoming a
positive. A retained gap can start inside the horizon even if its next observed
row is beyond the horizon.

The tables separate positive events, physical landings, completed clear horizons
and unresolved/ended observations. The machine-readable output also reports
extreme fractions treating unresolved labels as zero or one, both per attempt
and with equal world weight. These are coverage sensitivity bounds, not
confidence intervals, survival estimates or calibrated probabilities. In
particular, landing early is a competing endpoint, not ten seconds of exposure.

## Primary results

| Scope / checkpoint | Eligible | Interrupted | Landed first | Clear ten-second horizon | Ended / unresolved |
| --- | ---: | ---: | ---: | ---: | ---: |
| Normal, first choice | 341 | 72 | 5 | 225 | 39 |
| Normal, +15 s | 250 | 42 | 146 | 61 | 1 |
| Controlled, first choice | 71 | 6 | 0 | 55 | 10 |
| Controlled, +15 s | 56 | 0 | 31 | 25 | 0 |

At +15 seconds, another fifteen normal and four controlled observations lack an
active plan, and 76 normal/eleven controlled checkpoints are not observed. These
remain separate from the 194 normal/eighteen controlled attempts with no choice.
At first choice, most captures are too young for a complete five-second history:
334/341 normal and 66/71 controlled observations have unknown recent counts.
This history cannot supply a reliable initial-arrival forecast by itself.

At normal +15-second checkpoints:

| Recent prelanding replan | Attempts / worlds | Interrupted | Landed | Clear horizon | Ended |
| --- | ---: | ---: | ---: | ---: | ---: |
| None in five complete seconds | 217 / 24 | 23 | 142 | 52 | 0 |
| At least one | 33 / 16 | 19 | 4 | 9 | 1 |

That is 10.6% observed interruptions without a recent replan versus 57.6% with
one, with one unresolved observation in the latter group. Equal-world fractions
are 11.2% and 61.9–64.0%. Among sixteen worlds containing both groups, thirteen
have a strictly higher observed interruption fraction with recent replans, two
have a lower fraction and one is unresolved. These groups still differ in their
physical state and exposure; this is not a causal intervention comparison.

The association appears under both asteroid configurations: **8/12 versus 8/120**
with asteroids disabled, and **11/21 versus 15/97** at the three-second interval
(recent versus no recent replan). The pressured recent group contains the one
ended observation. It is not solely a difference between asteroid settings.

Flight phase matters. During circling the counts are 14/20 with a recent replan
versus 1/4 without; during approach, 4/7 versus 9/60. During descent, however,
the recent group has just four observations, none interrupted, versus 9/114
without a recent replan. Three of those four recent descents land within the
horizon. A pooled retry penalty would overstate what these sparse phase groups
establish. The recent-replan split covers only 19 of the 42 positive endpoints;
the other 23 have no recent native replan.

Recent live invalidations alone give 7/17 positive endpoints versus 35/233 without
one. Recent cover replans give 11/14. These overlapping categories have limited
world support and distinct controller mechanisms; they are not additive risks.

Progress and contact provide useful context but little calibration support here:

- At normal +15 seconds, four of six observations with no net one-second closure
  are interrupted, compared with 26/188 with net closure. One of the six lands.
  At +30 seconds the corresponding small group is only 2/6 interrupted. A
  universal stalled-progress rule is not supported by this sample.
- No eligible +15-second observation has a native landing-progress age of five
  seconds or more. Only two secondary +30-second observations do; both are
  interrupted. The clock is unavailable during tactical approach/circling.
- All twelve normal +15-second observations with one or two feet supported
  land before a replan. There are too few to infer zero risk from contact.
- Controlled +15-second observations contain no positive endpoints. They cannot
  establish how well a risk model separates successful and interrupted states.
  Secondary populations shrink to four controlled and 44 normal observations at
  +30 seconds, five normal at +60 and none at +120.

## Native calls and site acquisition are different populations

Across the active-attempt traces, normal runs contain 3,222 native replan calls:
3,175 while not physically landed and 47 already grounded. Controlled runs
contain 359: 344 not landed and fifteen grounded. Each confirmed increment is
one in these recordings. The diagnostic also records 555 normal and seventy
controlled site acquisitions with an observed zero replan delta; 174 additional
normal acquisitions have unavailable/reset counter scope.

Those totals must not become independent training examples or a per-approach
probability. **2,021 normal calls occur in 45 attempts that never select a site.**
Another 51 precede the first choice, 1,103 occur after choice and before first
physical landing, and 47 occur after physical landing. Controlled counts are
two, one, 341 and fifteen respectively. All repeated calls stay within their
original attempt in the fixed-checkpoint analysis.

The tactical controller can replan on stale objective work before obtaining a
site. Site acquisition needs its own availability/failure evidence. This local
landing-risk model must remain unknown before it has a plan; it cannot account
for those calls by assigning a large descent delay. The archive retains five
high-count no-choice cases with source references and first/last transitions.

## Counterexamples and next experiment

The saved cases retain positive warnings and ways they fail:

- `training/seed0-asteroids0-seat1-candidate`, selection 1988, tick 3660:
  a recent cover replan is followed by another at 3706, then repeated cover
  replans. Net site-distance closure was still positive at the checkpoint.
- `approach-expiry-normal/world0-asteroids3-seat0`, selection 13066, tick 15073:
  a recent live invalidation is followed by physical landing at 15439. The next
  native calls at 15440–15443 are grounded. A blanket retry penalty would also
  warn on this successful descent.
- The prior clearance miss, `descent-regimes-normal/world0-asteroids3-seat0`,
  selection 2813, saved forecast tick 5713: five seconds of quiet history,
  1.75 units of net distance closure in one second and a 0.18-second native
  progress age still precede a live-route invalidation at 5974, 4.35 seconds
  later. Physical landing first appears at 6497; acknowledgement is 6504.
  This saved forecast is a separate post-hoc case, outside the fixed-checkpoint
  aggregates. At its actual +15-second checkpoint, tick 5233, the replan is
  beyond the ten-second horizon and the correct label is a clear horizon.
- `descent-regimes-controlled/world0-band20-40-dir-1-seat1`, selection 345:
  physical landing at 1808 precedes the native replan at 1820. That call must
  not be charged as an airborne interruption of the earlier forecast.

The next bounded experiment is an **offline, phase-conditioned interruption
estimate with a recent-native-replan feature**. Start with an explicit ten-second
endpoint, known support counts and unknown outputs for unsupported conditions.
Compare a phase-only baseline with the additional recency feature on disjoint
worlds, keeping successful landing, failed attempts and censored endpoints
separate. Report probability quality and coverage as well as discrimination;
an always-high warning rate is not an improvement. Preserve the current time
estimate and native controller decisions during that comparison.

This is one prerequisite for time/risk-aware mission selection. Initial site
acquisition and remote transfer still need separate estimates. Multiplying the
observed replan fraction by an arbitrary retry delay would not produce a
defensible expected trip cost.

## Reproduction and retained evidence

The working evidence is under `target/bot-landing-risk/`. The portable archive is
`/home/oldman/.codex/visualizations/2026/09/22/bot-landing-risk/`, with an adjacent
manifest, archive hash and commit binding. It includes source manifests,
evaluations, derived event traces and reports, plus the diagnostic, tests,
analysis drivers, declared design, output hashes and selected cases. Prior
landing-tail/clearance/composition archives retain the original raw recordings.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/inspect-landing-risk.py \
  --manifest target/bot-landing-risk/inputs.json \
  --out target/bot-landing-risk/reproduced
```

All **293 Python tests pass**, including future-data isolation, gaps crossing a
horizon, reset/unknown counters, two-foot settling, simultaneous landing/replan,
exclusive attempt boundaries, later ground failure, hash rejection and
world-weighted accounting. Added boundary guards reproduce all four corrected
diagnostic output files byte for byte.

`metadata-correction.json` records an implementation correction: the first pass
retained the whole asteroid report as run metadata instead of extracting only
the configured interval. The corrected pass discards the report's final
statistics from the causal inputs and uses the declared configuration strata.
All checkpoint features, populations and outcomes are verified unchanged; no
threshold was retuned. `analysis-corrected/` and its exact `analysis-verified/`
replay are authoritative. The earlier output is retained only for the correction
audit. This evidence remains known development data for any later model.
