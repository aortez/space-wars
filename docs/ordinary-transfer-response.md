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
