# Native evidence before a landing choice

The [acquisition audit](bot-site-acquisition.md) found long local waits but could
not identify the native checks withholding results or rejecting candidates.
Those checks now emit bounded diagnostics. Seven reruns of retained cases
separate failed publication of negative searches from solar rejection of
otherwise usable ground routes. This checkpoint changes telemetry, not controls,
physical permissions, planner allowances or the waiting policy.

## What is recorded

Two optional fields expose decisions at the checks themselves:

- `observation.local.objective_evidence` records the current objective, examined
  request generation, source objective and original measurement tick. It retains
  that identity even when the request is invalidated and replaced during the
  observation. Route-dependent publication records the source route outcomes,
  whole-region/scalar-gravity/flight-environment checks, expired or changed
  crossings, retained routes and publication decision. Submission deferrals
  remain distinct from invalidated work.
- `mission.capture.acquisition` records the capture controller's observation
  tick, objective/revision, survey age source, candidate query state and branch
  taken. Counts separate site exclusions, solar checks, missing surveys, missing
  or unusable routes, and eligible directions. Joint endpoint and policy checks
  report their own rejection reason before returning neutral controls.

The controlled runner places these fields under `observation` and `capture`.
Neither field is a new physical permission. A missing publication record means
that publication was not examined there; it does not mean success. Detailed
publication checks apply to the route-dependency adapter, including early
positive candidates. They do not reconstruct unexecuted validation checks or
claim complete diagnostics for the strict-region adapter.

These are current records, not growing histories. Publication counts inspect
the existing bounded survey (up to eight proposed sites plus an actual return).
Selection counts follow the existing candidate order and at most two directions
per candidate, without additional world queries or route searches. Solar
approach, parking and departure counts can overlap; the first failed selection
gate determines which later checks are skipped. A direction rejected for solar
safety has not also been tested for ground access at that point.

The capture controller can be bypassed by a higher-priority mission action such
as solar escape. Consumers must require `acquisition.tick == row.tick` before
attributing a current decision. The sortie controller's same-tick guard preserves
its cached result; reset clears it. The diagnostic analysis keeps bypassed observations
separate, and uses the prior audit's arrival/choice/stop boundaries unchanged.

## What the retained cases show

These are known reproductions, not fresh-world validation or failure-probability
samples. The five normal matches use three worlds; the two controlled trials use
two other worlds. The native wait ledger covers 34 arrived normal attempts and
two controlled attempts, including quick acquisitions and later failures.

**Completed negative searches cannot always be republished.** In
`world3-asteroids0-seat1`, selection 14,130 arrives at 14,915 and ends at 21,053.
All 204 completed generations examined before a choice contain eight outbound
`disconnected` results and no usable route. Every publication attempt returns
`no_validated_routes`: 53 fail the whole-region check alone, thirteen fail scalar
gravity alone, and 138 fail both. Flight-environment checks pass throughout.
This explains why completed work never becomes a survey in the 102.30-second
wait. It is neither an unfinished queue nor 204 failed physical landings.

The validation rule is intentional: a positive path can survive unrelated
change through local validation, while a negative search cannot be renewed that
way. These results do not prove the planet is globally unreachable. They do
show repeated unsuccessful acquisition while the current waiting commands move
the ship farther from its destination.

**Published negatives still do not yield a site.** In
`world7-asteroids0-seat0`, selection/arrival 26,421 ends at 35,424. Native evidence
records 287 rejected negative publications: 246 fail region validation alone,
eighteen scalar gravity alone, and 23 both. Whole-survey validation also succeeds
on 414 observations spanning 66 generations; all published routes are negative.
Those generations overlap the rejected set as validity changes, so the counts
must not be added as independent searches. Solar checks reject no directions
here. The capture controller eventually reaches its existing 150-second limit.

**Ground access and safe flight do not necessarily coincide.** In
`world5-asteroids0-seat0` at tick 6,015, the published route is accepted as current
and the candidate has usable ground access. Both approach directions fail solar
safety: both fail departure clearance, and one also fails approach clearance.
Across the 170-tick local wait, 824 candidate directions produce 611 solar
rejections, 141 missing routes for directions that pass solar checks, and 72
missing surveys. None is eligible. Other retained asteroid cases likewise mix
solar rejection with absent routes for the remaining candidates. The native
checks do not support weakening endpoint or solar validation.

**The controlled delay is also a flight-safety issue.** The
`world0-band40-60-dir-1-seat0` trial rejects both directions on approach clearance
for all 646 prechoice ticks (10.77 seconds), then selects and eventually completes
the trip. The `world2-band8-20-dir-1-seat0` trial rejects both directions on
approach clearance for its 111 prechoice ticks before ship loss. Both have usable
ground-route evidence. A universal timeout below the successful trial's delay
would discard a demonstrated completion.

## Verification

The final diagnostic build uses Rust 1.89.0, matching the archived comparison
runtime. The seven reruns match **all 279,289 trace rows** after removing only the
two new fields. Controls, actions, observations and all previous telemetry remain
identical. All **57,366 planner dispatch rows** match after excluding only
`dispatch_ms`; every row stays within its recorded graph/query allowance.
Reports match after removing the new fields and explicitly logged timing fields,
including their timing-threshold counters. Match outcomes, events, physical
audits and mission milestones are unchanged.

All **713 native tests pass** across both crates' targets with all features;
`engine-client` checks successfully and formatting passes. New cases cover
negative-result validity, source-request identity after invalidation, measurement
age, missing actual return, deferred versus empty scans, solar rejection,
same-tick/reset behavior and malformed joint endpoints. Clippy completes with
fourteen existing warnings in unchanged scenario files; the changed code adds
none. This is not a device-performance measurement.

## Next behavior slice

Implement bounded acquisition under an opt-in policy, then compare it against
this unchanged-control baseline:

1. Start the acquisition clock at local capture handoff. Distinguish pending
   work, repeated negative or invalidated results, and solar-blocked candidates.
   A new generation alone is not useful progress.
2. Preserve immediate solar/collision escape. Otherwise use bounded guidance
   near the target while waiting, instead of indefinitely requesting outward
   speed. Deferred scans and missing data must remain unknown, not unreachable.
3. Return an explicit acquisition failure to the mission coordinator when its
   deadline is exhausted, using the existing planet deferral. Keep this separate
   from chosen-site retries and physical landing failures.
4. Replay these cases, including the successful 10.77-second wait, and then use
   fresh paired worlds with swapped seats and both asteroid settings. Judge
   captures, completed trips, survival, waiting time, frame exits and total work;
   fewer invalidations alone is not a gameplay improvement.

No new timeout is selected by this diagnostic checkpoint. Choosing safe sites
near a flag and forecasting complete mission cost remain subsequent work.

## Evidence and reproduction

The working root is `target/bot-acquisition-native`. `design.json` binds the
parent revision, full Rust source hashes, binaries, commands and baseline trace,
report and planner-CSV hashes. `run-cases.py` reuses the original seven commands
and verifies source stability. `analyze.py` checks full parity, binds the prior
acquisition report, and writes compact native wait records and summaries.
Wait intervals are half-open: arrival through the observation before first
choice or attempt end. All 37,280 retained wait ticks are present.

The previous raw inputs are in the
[probability-comparison archive](bot-landing-risk-probability.md), with bound
arrival/choice ledgers in the [acquisition archive](bot-site-acquisition.md).
Runtime patch, final-build outputs and the reproducible comparison scripts are
preserved in the evidence archive recorded below.

Archive:
`/home/oldman/.codex/visualizations/2026/09/23/bot-acquisition-native/evidence.tar.gz`
(676,252,109 bytes, 88 members), SHA-256
`05de8d99a33faeab0f9a7dadf1e806cccbf7eb2c329b55757cbb77cdab0f9c08`.
Manifest SHA-256:
`690a469f71ab6e6346fbc0c71a61ec28953830ce70c5f2b9b67dcdfb15804c06`.
The complete runtime patch applies to parent
`6250bf4dab44ce4cec3e03dbdcdd5bbef815645c`; its SHA-256 is
`e6ab39a153d76a10ebc45897c3214433c2da2d2bd64c52e6c904ea015f062c07`.
