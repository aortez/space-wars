# Projectile warnings and responses during ordinary transfers

## Read-only survey plan

Use all 15 observe controls from the [broader response suite](projectile-response-sweep.md),
summary SHA-256 `aaeea92ed3c022db0c0ffd95091171e8b63496a55c0f00601120baa5f663a483`.
Read the evaluated seat's complete capture, projectile and response streams;
retain both-seat projectile identities to detect ambiguous launch metadata.
Keep eight retained primary, three health and four fresh primary cases separate.
Do not select additional seeds or omit cases based on warnings or outcomes.

Screen native `Transfer` frames with a target, no capture/recovery task, armed
controls, ready queries, an available full ship aboard, flying with no supported
feet, and an unfinished match. Include ordinary and committed post-escape
transfers. The old dense streams do not expose the flight-enabled flag on every
ordinary tick, so this is a candidate-state survey; any live extension must also
retain that consumed-observation safety check.

Keep the existing two-second relative-velocity circle screen, all projectile
owners, sensor bounds and earliest-entry/ID tie break. Record eligible ticks,
warning and overlap ticks, first warnings, and contiguous episodes keyed by
projectile, vehicle, target and native Transfer episode (`goal_since`). Record
own-projectile episodes separately. Do not tune the screen to the observed hits.

Associate contact receipts during an episode or within 120 ticks after its last
flag; report later receipts separately. Lead time starts at the episode's first
warning. Multiple observed IDs with the same launch tick make attribution
ambiguous. Last-contact metadata can omit simultaneous contacts; retain the
largest per-tick contact-counter increment. Absence of a receipt does not prove
a false positive. Save first-warning physical witnesses, episodes and receipts,
with input and tool hashes. Freeze this survey before reading its results.

After the survey, define an ordinary-transfer eligibility extension and freeze
its implementation, tests and comparison plan **before intervention outcomes**.
Keep the current half-second brake and left responses unchanged. Test all 15
cases with matched observe controls through complete matches, including physical
claims, departures, recovery and pilot loss. Native launch, safety, capture and
recovery priorities must still cancel a pulse. Defaults and deployment remain
unchanged; all work and commits stay local.

## Survey findings

The survey, frozen in `67c46a9`, covers **70,299 candidate transfer ticks**.
Only 1,032 were admitted by the previous post-escape gate. Four configurations
have warnings, all from opponent missiles; there are no own-projectile warning
episodes and no ambiguous observed launch identities. Across them are nine
episodes and 175 warning ticks. Every evaluated P2 case has ordinary-transfer
coverage but no warning under this unchanged screen.

| Case | Candidate ticks | Warning ticks | Episodes | First warning | First episode's contact |
| --- | ---: | ---: | ---: | ---: | --- |
| World 0 P1 walking | 5928 | 0 | 0 | — | — |
| World 0 P1 powered | 7914 | 0 | 0 | — | — |
| World 0 P2 powered | 5798 | 0 | 0 | — | — |
| World 0 P2 walking | 6827 | 0 | 0 | — | — |
| World 1 P1 powered | 2137 | 55 | 1 | 8837 | 8892, fatal |
| World 1 P1 walking | 1648 | 0 | 0 | — | — |
| World 1 P2 walking | 6923 | 0 | 0 | — | — |
| World 1 P2 powered | 8900 | 0 | 0 | — | — |
| Health world 0 P1 walking | 4992 | 0 | 0 | — | — |
| Health world 0 P1 powered | 4793 | 28 | 1 | 13515 | 13543, survives |
| Health world 1 P1 powered | 2137 | 55 | 1 | 8837 | 8892, fatal |
| Fresh world 2 P1 powered | 1784 | 0 | 0 | — | — |
| Fresh world 2 P2 powered | 3236 | 0 | 0 | — | — |
| Fresh world 3 P1 powered | 3113 | 37 | 6 | 12087 | No matching receipt |
| Fresh world 3 P2 powered | 4169 | 0 | 0 | — | — |

The health world-0 case supplies a new actual-hit example: missile 100001 is
launched at 13514, flags at 13515 with a 0.442-second entry estimate, and contacts
the ship 28 ticks / 0.467 seconds later for 51.049 hull damage. It is not initially
fatal. The ordinary Transfer episode began at 12747 and targets planet 2.

Fresh world-3 P1 supplies a different test: missile 100022, launched at 12086,
flags at 12087 with a 0.041-second entry estimate, but no matching contact receipt
follows in the frozen window. Five more episodes follow (including three
episodes for missile 100028 across interrupted Transfer frames); none has a
matching receipt in its window. Do not treat a circle-screen flag as a guaranteed
hit or suppress this first warning using knowledge of its outcome.

The old missile at 8837 remains the same correlated primary/health example.
The [survey manifest](data/transfer-projectile-survey-v1.json) and
[compressed witnesses](data/transfer-projectile-survey-v1.json.gz) preserve all
15 cases, first-warning states, episode boundaries and contact receipts, with
45 source-file hashes. Contact counters increase by at most one per tick in
these runs; the terminal increments were checked separately and do not increase
that maximum. The surveyor now also folds the terminal increment directly into
future reports. This reporting correction does not change the saved results,
screening rule or any physical continuation.

## Eligibility and intervention plan frozen before outcomes

Add `--probe-projectile-response-scope escape|transfer`, default **escape**.
`transfer` admits any current native Transfer goal with a target and the existing
physical gates: match active, controls/queries/flight enabled, full available
ship aboard, flying without supported feet, no capture or recovery task. It
therefore includes the already eligible post-escape transfers. Native Launch,
AvoidSun, Capture and Recover goals cancel the attempt immediately. Normal
waypoint guidance remains subject to the bounded response while Transfer is
active; this is not a certificate of obstacle clearance.

An eligible active escape transfer retains its exact original identity and
deadline. Otherwise bind the attempt to **vehicle, destination and native
`goal_since`**. That identifies the continuous Transfer episode, not a newly
invented mission deadline. Any change ends the pulse permanently. Do not reset
mission memory, original progress budgets, capture eligibility or recovery.
The original 30-wall-tick, one-attempt-per-match policy, warning rule, earliest
entry/ID tie break and brake/left action semantics remain unchanged. Do not add
a minimum warning time, owner filter, second attempt or alternative direction
after reading the new states.

Use all 15 unchanged case configurations and complete match limits from the
49-run source suite. Run **observe, brake and left for each: 45 matches**.
Add **two legacy-scope observe replays**: retained world-1 P1 powered (previously
active) and world-0 P2 powered (inactive). Total: **47 matches**, at most two
concurrent. Freeze code, tests, this plan and a hashed binary before any new
simulation. Run legacy controls, expanded observe, brake, then left.

Require exact native replay for every observe run. Legacy controls must also
retain the complete old response log byte-for-byte. Independently reconstruct
expanded eligibility, consumed physical observations, source identity, warning,
fixed pulse end, priority cancellation and action bytes. Require exact two-seat
native prefixes through intervention and complete parity for inactive cases.
All earlier physical and planner-budget audits remain active. Compare live
observe coverage with the offline survey, explicitly checking the additional
flight-enabled condition. Keep the previous short-warning clear-entry braking
regression and stronger no-escape reference visible.

Report all eight retained primary, three health and four fresh primary outcomes
separately, including both new warning cases regardless of benefit. Measure
initial and later contacts, ship destruction while aboard or on foot, native
capture handoff, actual claims/departures, recovery and complete match outcomes.
Do not retune or broaden again from this experiment's intervention results.
The extension remains an opt-in laboratory tool; defaults, deployment and remote
state remain unchanged.

## Live observe and compatibility checks

Both legacy-scope replays retain the previous response logs byte-for-byte, as
well as native action/observation streams and physical outcomes. All 15 expanded
observe controls also retain full native replay parity. Their **70,299 eligible
ticks and all four first warnings match the offline survey exactly**, including
the additional live flight-enabled check. The new scope increases eligible
coverage from 1,032 ticks to 70,299 and warning-bearing configurations from two
to four. Two configurations are the correlated old world-1 example; the other
two provide additional response states. P2 receives broader coverage
but still has no physical response opportunity in these cases.

The new native identities are `(vehicle 0, destination 2, goal_since 12747)` for
health world-0 P1, and `(vehicle 0, destination 0, goal_since 11836)` for fresh
world-3 P1. The observe windows end at 13545 and 12117 respectively. These are
probe windows, not renewed mission deadlines. Original native mission state,
progress accounting and physical arrival/capture gates remain authoritative.

## Complete match results

All **47 matches pass** their physical, planner-budget and response audits.
There are 39 complete native parity checks: two legacy controls, 15 expanded
observe controls and 22 inactive brake/left cases. The remaining eight runs
change controls at the independently reconstructed first warning, with exact
two-seat native prefixes through that decision. Eleven of the 15 configurations
have eligible ordinary transfers but no warning; all 33 of their mode runs are
inactive. No case was dropped or rerun to replace an outcome.

Each cell below is the evaluated seat's outcome, followed by **physical
captures/departures**. Legacy controls are compatibility checks and are excluded
from these cohort totals.

| Case | Observe | Brake | Left |
| --- | --- | --- | --- |
| World 0 P1 walking | Loss, 4/4 | Loss, 4/4 | Loss, 4/4 |
| World 0 P1 powered | Win, 5/5 | Win, 5/5 | Win, 5/5 |
| World 0 P2 powered | Loss, 4/3 | Loss, 4/3 | Loss, 4/3 |
| World 0 P2 walking | Loss, 4/4 | Loss, 4/4 | Loss, 4/4 |
| World 1 P1 powered | Loss, 1/1 | Win, 7/6 | Win, 3/3 |
| World 1 P1 walking | Loss, 1/1 | Loss, 1/1 | Loss, 1/1 |
| World 1 P2 walking | Win, 4/4 | Win, 4/4 | Win, 4/4 |
| World 1 P2 powered | Loss, 3/3 | Loss, 3/3 | Loss, 3/3 |
| Health world 0 P1 walking | Loss, 3/3 | Loss, 3/3 | Loss, 3/3 |
| Health world 0 P1 powered | Loss, 3/3 | Loss, 6/6 | Loss, 3/3 |
| Health world 1 P1 powered | Loss, 1/1 | Loss, 6/5 | Win, 3/3 |
| Fresh world 2 P1 powered | Loss, 1/1 | Loss, 1/1 | Loss, 1/1 |
| Fresh world 2 P2 powered | Win, 3/3 | Win, 3/3 | Win, 3/3 |
| Fresh world 3 P1 powered | Loss, 1/1 | Win, 1/1 | Win, 1/1 |
| Fresh world 3 P2 powered | Win, 4/4 | Win, 4/4 | Win, 4/4 |

| Cohort | Mode | Wins | Captures | Departures | Evaluated pilot deaths |
| --- | --- | ---: | ---: | ---: | ---: |
| Retained primary (8) | Observe | 2 | 26 | 25 | 4 |
| Retained primary (8) | Brake | 3 | 32 | 30 | 3 |
| Retained primary (8) | Left | 3 | 28 | 27 | 3 |
| Health (3) | Observe | 0 | 7 | 7 | 3 |
| Health (3) | Brake | 0 | 15 | 14 | 2 |
| Health (3) | Left | 1 | 9 | 9 | 2 |
| Fresh primary (4) | Observe | 2 | 9 | 9 | 2 |
| Fresh primary (4) | Brake | 3 | 9 | 9 | 1 |
| Fresh primary (4) | Left | 3 | 9 | 9 | 1 |

The retained primary outcomes and all six previously activated world-1
primary/health continuations reproduce the earlier suite exactly, including
native/projectile streams, round results, visits and allocation accounting.
The stronger earlier no-escape reference remains **2 wins, 28 captures and
27 departures** across the eight primary cases, versus this immediate observe
control's 2/26/25. These small, related cohorts do not establish general bot
strength or an optimal response choice.

## New health world-0 response

Both interventions begin at 13515 and apply 29 ticks. At **13544**, native
mission control changes from Transfer to Hunt, pausing travel for the nearby
opponent. Both pulses cancel on that same tick and never rearm. The observe
trajectory makes that native change at 13552, after its unchanged probe window.

| Mode | Triggering missile contact | Recorded hull damage | First ship loss after warning | Match end | Captures/departures |
| --- | ---: | ---: | --- | --- | ---: |
| Observe | 13543 | 51.049 | 14252, laser | 15014, pilot world-boundary impact | 3/3 |
| Brake | 13544 | 19.883 | 31080, cannon | 31358, pilot planet impact | 6/6 |
| Left | 13542 | 43.455 | 14282, cannon | 18538, pilot planet impact | 3/3 |

All three initially survive the same missile. Braking mitigates its damage;
neither response avoids contact. All eventually lose the ship while aboard,
eject once, fail to rebuild and lose the match. Left extends pilot survival
without adding a capture. Brake permits three later native capture cycles.

The interrupted destination-2 visit itself never reaches capture in any mode.
With brake, a new visit starts at 15344, hands off to native capture at 16999,
physically lands at 18557, exits at 18561, claims at 18964, boards at 18965 and
departs at 19193. Subsequent physical claims/departures occur on planet 0 at
22794/23025 and planet 2 at 29821/30047. The intervening failed planet-1
approaches remain in the archive. Thus the added captures are complete later
physical cycles, rather than a response flag being counted as arrival.

## New fresh world-3 response

Both interventions apply all 30 ticks from 12087 through 12116 and finish at
12117. **No mode records contact from the triggering missile**, including the
observe control, throughout its complete match. Laser hull damage begins at
12092 in all three continuations. The outcomes therefore measure a change to
the later battle and trajectory, not demonstrated avoidance of this missile.

| Mode | First ship loss after warning | Match end | Evaluated pilot | Captures/departures |
| --- | --- | --- | --- | ---: |
| Observe | 13308, cannon, aboard | 18452, P1 world-boundary impact | Dies; loss | 1/1 |
| Brake | None | 15574, P2 world-boundary impact | Survives; win | 1/1 |
| Left | None | 15940, P2 world-boundary impact | Survives; win | 1/1 |

All modes had already lost and rebuilt one ship before the warning. Observe
finishes with two ship losses, two ejections and one rebuild; brake and left
each finish with one loss, one ejection and one rebuild. The candidates prevent
a further ship loss in these continuations, not every ship loss in the match.

Brake and left reach the native destination-0 arrival handoff at 12503 and
12492 respectively, but both abandon it at 12511 when cover search exhausts
its observed candidates. Neither physically lands, exits or claims there.
Later planet-2 attempts also stop for a nearby opponent. Observe abandons its
original visit at 12556 for that opponent without arrival. No continuation adds
a physical capture after the warning.

## Verification and retained evidence

The eligibility extension, independent auditor, runner, tests and intervention
plan were frozen in **`b1bcb69`** before these simulations. All **1,096 Rust
tests** and **768 Python tests** pass. Formatting, strict AI Clippy and both
profiled and ordinary release builds pass. Scenario Clippy retains the same
seven pre-existing findings at the same locations. Tests cover native/P2
eligibility and priority guards, episode cancellation without rearming, legacy
identity serialization and preservation of an active visit selected before
the native episode began.

All **2,386,862 projectile rows** pass their physical-frame, identity, ordering
and capacity audits. The observed maxima are four retained/in-range projectiles
and 23 scanned debris entries, with no unavailable shell or capacity truncation.
These are observed values, not general runtime bounds or deployment performance
measurements. All native physical capture and planner-budget audits pass.

The frozen binary is
`target/ordinary-transfer-response/surface_mission_soak-b1bcb69`, SHA-256
`32086075e2cce09f13aacf718c6b3a98aa9c7994229bee74cff504c1201f7912`.
The completed summary is `target/ordinary-transfer-response/v1/summary.json`,
SHA-256 `fbb2f4d04d39e16ba19b3162d3237178694fdf62617f87b54ae707626bb122aa`.

The [manifest](data/ordinary-transfer-response-v1.json) records all 47 outcomes,
separate cohort totals, paired changes, exact survey comparisons, legacy and
retained-response parity, input/tool/binary hashes and **752 raw-file hashes**.
The [compressed archive](data/ordinary-transfer-response-v1.json.gz) preserves
the original runner summary, physical visit audits, response witnesses,
triggering-projectile tracks, report checkpoints and final reports excluding
their large samples/events arrays. Contact, loss and recovery evidence remains
available for both new states and all previous active cases. Complete raw
reports and streams remain at their hashed local paths.

```sh
python3 tools/validate-ordinary-transfer-response.py \
  --prior target/projectile-response-sweep/v1/summary.json \
  --survey docs/data/transfer-projectile-survey-v1.json \
  --binary target/ordinary-transfer-response/surface_mission_soak-b1bcb69 \
  --out /tmp/ordinary-transfer-response
python3 tools/analyze-ordinary-transfer-response.py \
  --summary /tmp/ordinary-transfer-response/summary.json \
  --out /tmp/ordinary-transfer-response-results.json
```

Use fresh output paths and a clean checkout for the runner.

## Decision

Keep the expanded scope and both responses opt-in. The survey supplied two
additional warning states; the frozen intervention experiment found a damage
and capture benefit for brake in the new health case, and wins without capture
gains for both modes in the new fresh case. There is still no activated physical
P2 response, and the fresh case does not demonstrate avoidance of its triggering
missile. The two old world-1 configurations remain one correlated trajectory.

The earlier [clear-entry braking regression](projectile-response-sweep.md)
also remains: its short-warning pulse ends after 13 ticks with ship destruction
at 9173 instead of observe's 9185, with no extra capture. It is a historical
counterexample, outside these 47 runs and cohort totals. This extension does
not resolve response selection or make the warning screen a collision predictor.
Broader independent activated cases and a separately specified response-choice
rule would be needed before considering a default change. Probe mode remains
`none`, default scope remains `escape`, and nothing was pushed or deployed.

The closing [selector comparison](projectile-response-selection.md) diagnoses
the retained braking regression, freezes one brake-or-native rule and tests new
worlds in both seats. The [branch review guide](bot-route-evidence-checkpoint.md)
places these experiments within the complete mission-execution checkpoint.
