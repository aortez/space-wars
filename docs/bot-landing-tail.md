# Landing-tail and interruption diagnosis

Ordinary touchdown time and interruption time need separate evidence. In the
saved recordings, the final uninterrupted tail usually contains about one second
of alignment, six seconds of descent and a few tenths of a second of settling.
Long tails also include severe heading errors and one-foot contact. Meanwhile,
phase-clock breaks mix native replans, reacquiring a site and unavailable phases.
Some native replans happen after the ship is already physically landed.

The next bounded model should start with **descent conditioned on current foot
clearance**, while keeping alignment, contact and interruption evidence separate.
This investigation changes no controller, timing profile, landing permission or
Pi deployment. It does not fit an ETA or estimate success probability.

## Evidence and accounting

`tools/inspect-landing-tail.py` replays **136 existing recordings on 25 worlds**:
16 original phase-training runs, 40 walking-regime runs, 40 strict approach-state
runs and 40 duration-first expiry runs. These are now known recordings, not new
validation. Both seats, asteroid conditions, failed attempts and failed controlled
setups are retained.

The diagnostic checks every report/trace against its source evaluation and
reuses the existing normal/controlled replay lifecycle and authoritative phase
clock. All selections, phase episodes and break records match their source
exactly. It processes **695,682 observations inside active prelanding attempts**,
not every frame or actor in the source recordings.

There are **424 observed attempts**: 188 complete, 197 abandoned, eleven ending
with the match, thirteen ship losses and fifteen controlled-frame departures.
Of these, 154 have no observed landing choice. Among the remaining attempts,
198 have an exact observed landing: all 188 completed trips, eight later
abandoned trips and two later ending with the match. The other 72 remain
unfinished/censored. A later ground failure does not erase a successful landing.
Trial-level setup outcomes remain separate from these attempt counts.

Snapshots record current site-relative position/velocity, heading error, relative
spin, native radial landing measurements, contact, controller progress and retry
counters. Snapshots use current/past observations only. Final tail selection and
elapsed partitions use outcomes and are explicitly retrospective.

Native foot rays use 26 as a no-hit value; it is never treated as measured high
altitude or as contact. Site-relative heading is distinct from the native radial
landing angle. Missing/stale site evidence leaves pose features unavailable.
Observed gaps and unavailable phases retain their own unclassified time.

## Final uninterrupted tails

Take at most one final uninterrupted approach-to-surface handoff per original
attempt, with an observed approach entry and exact later landing. This gives
144 normal and 25 controlled tails; it does not cover every landed attempt.

| Component | Normal median / p90 | Controlled median / p90 |
| --- | ---: | ---: |
| Whole tail | 7.72 / 10.38 s | 7.77 / 9.37 s |
| Alignment | 1.04 / 5.87 s | 1.25 / 2.18 s |
| Descent without supported feet | 6.22 / 7.07 s | 6.40 / 7.30 s |
| Settling with supported feet | 0.34 / 1.25 s | 0.32 / 1.27 s |

These are separate marginal statistics, so component medians do not necessarily
sum to the whole-tail median. They are attempt-weighted descriptive statistics,
not independent-world accuracy estimates or bounds.

Descent-entry foot altitude correlates with measured descent duration at
**r=0.956 on 143 normal tails** and **r=0.943 on 25 controlled tails**. This is
consistent with the existing landing assist's two-units-per-second target sink
rate. It supports investigating clearance as a feature, not deploying a fitted
formula from this cohort. Assist strength, gravity, approach velocity, valid
rays and subsequent contact still matter.

Once both feet are observed supported, median time to the controller's landing
acknowledgement is 0.23 seconds in normal runs and 0.28 seconds in controlled
runs, near the native 0.25-second settle gate. Observation/acknowledgement order
and interrupted settle timers explain why these are not identical constants.
One supported foot is less decisive: the longest normal interval from first
contact to acknowledgement is 10.12 seconds.

Alignment cannot be folded into a fixed touchdown delay. In
`training/seed3-asteroids0-seat1-candidate`, selection **5247**, the final handoff
has a **167° site-relative heading error** and a tail of **18.15 seconds**:
9.78 alignment, 3.43 descent and 4.93 settling. The controlled maximum is
14.72 seconds, including 9.70 alignment. The current tactical handoff tests
height, lateral offset and relative speed, but does not require heading alignment.
This provides a concrete mechanism to investigate; the pooled association alone
does not prove that changing the handoff rule improves gameplay.

Current attitude can help at the actual handoff. Its future value is not available
to a prediction made earlier during approach. Do not use a later observed
handoff pose as if the earlier estimator knew it.

## Clock breaks are not native retry counts

For exact landings, the source phase-clock markers decompose as follows:

| Marker evidence | Normal | Controlled |
| --- | ---: | ---: |
| Native replan counter increment | 178 | 7 |
| Site acquired without another native replan | 117 | 1 |
| Phase unavailable | 14 | 0 |
| Total phase-clock break markers | 309 | 8 |

Native tags may overlap: a live-route invalidation also increments the objective
replan counter. They are evidence categories, not additive independent causes.
Missing observations or a changed capture scope cannot manufacture counter deltas.
The existing phase clock remains unchanged, including its conservative calibration
exclusions; this diagnostic does not silently reinterpret old fitted profiles.

Of the native replan markers above, **39 normal and five controlled** already
have physical `landing.phase == landed` in the preceding observation. Those may
still represent real route, clearance or access problems. They should be kept
separate from an airborne failure to descend, rather than all being assigned the
same flight-retry penalty.

For exact normal landings, 111 have no later phase break after the first landing
choice (median 17.37 seconds), while 59 have a break (median 26.07 seconds).
In the latter group, median elapsed time before the final break is 16.57 seconds,
and median final segment is 9.05 seconds. These are different physical states,
not matched counterfactuals: **time before a break is not proven avoidable delay**.
A tiny final segment can simply mean the ship was already on the ground.

## Saved cases and what they establish

`selected-cases.json` retains full phase/counter timelines and pose/contact
snapshots. `native-retry-cases.json` retains selected raw controller/site evidence.

- **Large interrupted miss:** expiry `normal/world0-asteroids3-seat1`, selection
  11253, first choice **12808**, physical landed **17516**, controller acknowledgement
  **17518**. The old forecast's whole-trip error remains −53.98 seconds. Actual
  time from first choice to acknowledgement is 78.50 seconds: 4.78 circling,
  24.78 approach, 37.98 alignment, 0.27 descent, 6.48 supported settling and
  4.20 without an eligible phase. Its nine break markers contain **five native
  replans and four site acquisitions**. Three native replans are live-route
  invalidations; two are ordinary retries. Time before the final marker is
  57.83 seconds and the final segment takes 20.67 seconds.
- **First ordinary retry in that case:** just before **14976**, the landing
  controller has made no progress for 15 seconds. The ship is supported on one
  foot at a 40.7° radial angle; the other foot has 4.75 units of clearance. This
  matches the alignment no-progress timeout, not a slow ordinary descent.
- **Second ordinary retry:** before **16188**, the ship is at rest on one foot
  with the other about 0.88 units up, and the selected site's
  `hatch_has_settling_margin` is false. This matches the existing rule refusing
  the one-foot correction without hatch clearance. A better time estimate
  cannot grant that physical permission.
- **Late replan after physical landing:** expiry controlled
  `world1-band20-40-dir-1-seat1`, selection 315. Physical landing is observed at
  **1950**, a live-route replan occurs at **1952**, and controller acknowledgement
  follows at **1955**. Counting this as an additional failed airborne approach
  would be misleading. It explains the prior diagnostic's three-frame final
  segment without suggesting a three-frame flight.
- **Ordinary longer descent:** expiry controlled `world1-band8-20-dir-1-seat1`,
  selection 315, handoff **1410**, acknowledgement **1937**. With no restart,
  the tail is 0.97 alignment + 7.45 descent + 0.37 settling = 8.78 seconds.
  The earlier tail prediction was 1.37 seconds short. Extra descent time,
  rather than repeated replanning, explains most of this tail.
- **Previous strict-model regression:** strict normal `world0-asteroids0-seat1`,
  selection 3953, handoff **5386**, acknowledgement **6021**. The uninterrupted
  tail is 1.87 alignment + 7.12 descent + 1.60 settling = 10.58 seconds. At the
  handoff, heading error is 52.6° and relative spin is 1.78 rad/s. This is a useful
  retained test for alignment, descent and one-foot settling components.

## Next bounded implementation

Keep the duration-first approach rule. Add an explicit offline descent component
using **current measured foot clearance and motion**, with valid-ray, assist,
frame and support guards. Preserve alignment/contact states separately and
compare against the current phase-duration model. Start predictions at the
observed phase entry; do not substitute future handoff geometry into earlier
approach forecasts. Freeze the rule before generating new validation worlds.

Keep a separate interruption record distinguishing
native airborne replans, site reacquisition and replans while physically landed.
Use observed recent invalidations, progress and contact evidence as candidate
risk features. Failed and censored attempts must participate before claiming
risk calibration; the successful-tail tables above are insufficient. Neither
clearance correlation nor a shorter conditional ETA establishes a better mission
choice. Remote-transfer costs and unsupported powered routes remain later gaps.

No controller relaxation is proposed here. If changing the handoff attitude or
one-foot behavior later, use the saved seeds/checkpoints for physical comparison,
including the hatch-clearance rejection, before tuning or deployment.

## Verification and reproduction

All **239 Python tests pass**, including fourteen new tests for moving-frame
attitude, invalid/missing pose, no-hit rays, native retries versus reacquisition,
gaps/resets, immutable past samples, interval accounting, uncertain/censored
landings, source binding and replay lifecycle equivalence.

The original phase-calibration evaluation lacks a top-level runtime field.
The first read of that dataset failed explicitly. Compatibility now requires
its hash-bound manifest and every run's runtime, while other schemas still
require their normal metadata. The retry passed; the initial log/source and
compatibility correction are retained. Snapshot and duration calculations were
unchanged by that correction.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/inspect-landing-tail.py \
  --manifest target/bot-landing-tail/inputs.json \
  --out target/bot-landing-tail/reproduced
```

The retained `run.py` runs the same seven datasets in parallel; `analyze.py`,
`cases.py` and `case-native.py` produce the summaries and case notes. Evidence is
under `target/bot-landing-tail/`, archived under
`/home/oldman/.codex/visualizations/2026/09/21/bot-landing-tail/`. The archive
contains derived traces, source manifests/evaluations, scripts, tests and source
patch, with hash-bound dependencies on the four earlier raw-recording archives.
It does not duplicate those raw recordings or claim new independent validation.
