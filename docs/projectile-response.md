# Brief physical responses to a projectile warning

## Frozen laboratory comparison

The [projectile diagnostic](projectile-diagnostics.md) flags the old missile
55 ticks before impact in the two speed-limited transfers. Test one fixed
half-second response, then let the original policies finish the match. This is
a controlled laboratory intervention, not a promoted mission policy.

`--probe-projectile-response none|observe|brake|left|right` defaults to `none`.
The probe runs only for the explicitly evaluated seat in a duel with capture
and projectile tracing enabled. It cannot be combined with the impact or
successor-control probes. It changes no physical state directly, and performs
no future simulation to choose a response.

Use the existing 600-unit / 64-projectile diagnostic and unchanged two-second
circle-entry screen. At the first warning during an eligible committed escape
transfer, choose the earliest projected entry, breaking exact ties by stable
projectile ID. Include all owners. No tick, missile identity, launch time or
outcome is hard-coded into the trigger. The older baseline has a *different*
missile warning later in the transfer, so preserve and test those cases too.

Eligibility requires the current native `Transfer` goal, active original
transfer identity/deadline, an aboard full ship, ready controls/queries/flight,
flying with no supported feet, and no capture or recovery task. Match completion,
any native priority change or transfer identity change permanently ends the
attempt. These checks run every tick, including during the pulse. The original
transfer deadline and progress controller remain authoritative.

Freeze **30 wall ticks**, with at most one attempt per match:

| Mode | Physical input during the pulse |
| --- | --- |
| Observe | Keep all original actions; record the warning and window |
| Brake | Retain native turn; release thrust, request braking and open wings |
| Left | Request turn −1 and open wings; retain native braking and thrust only when not braking |
| Right | Request turn +1 and open wings; retain native braking and thrust only when not braking |

Weapons and interaction remain unchanged. Lateral steering uses the real ship's
turn/thrust dynamics, with no instantaneous sideways force. The warning need not
remain present throughout the pulse; the fixed deadline cannot renew or restart.
Normal controls resume at the first tick outside the window, or earlier when a
native priority/identity check fails. The probe does not reset bot memory.

## Retained trials and checks

Use the four enabled cases from
`target/projectile-diagnostics/v1/summary.json`, SHA-256
`70ba8a21f883aaeedb935c9e0e06b18a7e46227ca92175f237a2dd2fd76a50dc`:
clear-entry and speed-limited world 1 P1 powered, each primary and health-enabled.
Run all four modes above on every case: **16 full-match trials**. Add **one
disabled control** for the primary speed-limited case. Preserve seeds, opponents,
physics, options, full match lengths and planner budgets. At most two concurrent
simulations. Freeze implementation, tests and this plan before outcomes; retain
the frozen binary and its hash. Do not select a duration or retune from results.

Require exact original observations/actions for both seats through the trigger
(substituting the recorded native action on the trigger tick). Observe and
disabled cases must retain full-match parity, including projectile traces.
Independently reconstruct every trigger, source identity, deadline, priority
stop and action byte. After the pulse, require the applied action to equal the
recorded native action. Save dense damage/contact receipts, physical losses,
source observations, all pulse actions and checks. Existing native physical
capture and planner-budget audits remain active.

Report the initial missile's eventual contacts, damage from other sources,
ship/pilot survival, native arrival/capture handoff, physical claims/departures
and complete match outcomes. Compare all three interventions to their immediate
observe control and keep the health cases separate. Preserving the stronger
earlier no-escape objective reference remains necessary. Avoiding one contact
alone does not establish a useful or generally safe policy.

Defaults, deployment and ordinary bot behavior remain unchanged. All work and
commits remain local.

## Results

All **17 full-match trials pass** their native physical, planner-budget,
projectile-stream and response-control audits. Four observe runs and the
disabled control reproduce their original full matches exactly. Both seats'
complete consumed observations and original actions agree through each trigger:
18,322 rows for clear-entry and 17,676 for speed-limited cases. Applied controls
change first on the trigger tick, and return to the native actions when the
bounded pulse ends. No original transfer deadline changes.

### Trigger and contact

The speed-limited cases trigger at **8837** on missile **100025**, launched at
7977, with a projected circle entry in **0.900 seconds**. Every active response
lasts the full 30 ticks and ends at 8867. The clear-entry cases trigger at
**9160** on a different missile, **100029**, launched at 9159, with projected
entry in **0.185 seconds**. Their steering pulses end at 9190. Braking ends
early at 9173 after 13 ticks because the ship is destroyed and native recovery
takes priority. The observe window also stops when native recovery begins.

| Path | Response | Triggered missile contact receipts | First ship loss |
| --- | --- | --- | --- |
| Speed-limited | Observe | 8892, fatal | 8892, cannon |
| Speed-limited | Brake | None through match end | 10818, later cannon |
| Speed-limited | Left | 8891, survives | 10711, later cannon while pilot is on foot |
| Speed-limited | Right | 8890, fatal | 8890, cannon |
| Clear-entry | Observe | 9174, survives initial contact | 9185, laser |
| Clear-entry | Brake | 9173, fatal | 9173, cannon |
| Clear-entry | Left | 9174 and 9175, survives | 9311, later cannon |
| Clear-entry | Right | None through match end | 9563, later cannon |

Primary and health-enabled cases agree on all entries in this table. Braking
avoids a recorded contact with the old missile on the speed-limited path. Left
steering **does not avoid that missile**: it changes the hit to 18.769 hull
damage, leaving the ship alive instead of losing its remaining 31.174 hull.
Right steering moves the fatal hit two ticks earlier. On the clear-entry path,
right steering avoids the newly fired missile but still loses the ship later.
Braking there loses the ship twelve ticks earlier than the observe control.

Contact receipts retain launch ticks; diagnostic tracks corroborate the unique
observed projectile identity for each launch. A receipt is the last recorded
contact/source in a physical tick, not a complete list of simultaneous contacts
or a proof that a projectile ceased to exist outside diagnostic range. The
archive retains damage counters, physical observations and tracks alongside
these summaries.

### Objective and full-match follow-through

Counts below are physically audited captures and departures over the **whole
match**, including the one capture/departure completed before the intervention.
They are not the number of planets owned at match end. All evaluated seats are
P1. Primary and health cases are reported separately because later behavior can
diverge even when the initial response agrees.

| Path | Case | Mode | Captures / departures | P1 outcome | Match ends at tick |
| --- | --- | --- | ---: | --- | ---: |
| Speed-limited | Primary | Observe | 1 / 1 | Loss, pilot death | 10686 |
| Speed-limited | Health | Observe | 1 / 1 | Loss, pilot death | 10686 |
| Speed-limited | Primary | Brake | 7 / 6 | Win, time limit | 36000 |
| Speed-limited | Health | Brake | 6 / 5 | Loss, time limit | 36000 |
| Speed-limited | Primary | Left | 3 / 3 | Win, opponent pilot death | 26830 |
| Speed-limited | Health | Left | 3 / 3 | Win, opponent pilot death | 26830 |
| Speed-limited | Primary | Right | 1 / 1 | Loss, pilot death | 9357 |
| Speed-limited | Health | Right | 1 / 1 | Loss, pilot death | 9357 |
| Clear-entry | Primary | Observe | 1 / 1 | Loss, pilot death | 9407 |
| Clear-entry | Health | Observe | 1 / 1 | Loss, pilot death | 9407 |
| Clear-entry | Primary | Brake | 1 / 1 | Loss, pilot death | 9560 |
| Clear-entry | Health | Brake | 1 / 1 | Loss, pilot death | 9560 |
| Clear-entry | Primary | Left | 1 / 1 | Loss, pilot death | 10039 |
| Clear-entry | Health | Left | 1 / 1 | Loss, pilot death | 10039 |
| Clear-entry | Primary | Right | 1 / 1 | Loss, pilot death | 9980 |
| Clear-entry | Health | Right | 1 / 1 | Loss, pilot death | 9980 |

The disabled speed control retains 1 / 1, P1 pilot death at 10686 and full replay
parity. The original transfer remains bound to destination 2 and deadline
11976 in every enabled case.

Speed-limited braking hands off to the native capture task at **9182**, lands
at 10609 and claims destination 2 at **10795**. A different missile launched at
10796 destroys the ship at 10818, before departure. Recovery and later native
sorties produce the remaining captures. Both pilots survive to the time limit;
the primary case ends owning 2 planets against 1, while the health case ends
owning 1 against 2. More capture events do not guarantee a win.

Speed-limited left steering hands off at **9145**, lands at 10664 and exits at
10665. The ship is destroyed at 10711 by a missile launched at 10676 while the
pilot is on foot, before that destination's claim. The pilot later rebuilds;
native sorties claim planets 0 and 1 at 23309 and 26416 and depart at 23532 and
26647. Both cases end with P1 alive, owning all three planets, when P2 dies from
a missile at 26830. These are recovery successes after a surviving initial hit,
not uninterrupted ship survival.

Clear-entry right steering reaches a native capture handoff at **9437**, but
the later cannon hit destroys the ship at 9563 before landing or claiming.
The other clear-entry responses and speed-limited right steering never hand
off to capture. No clear-entry response adds a physical capture or departure.

P1's eventual fatal collisions remain visible in the complete results:
clear-entry observe/left/right end in planet impacts at 9407/10039/9980;
clear-entry braking ends in a world-boundary impact at 9560. Speed-limited
observe/right end in world-boundary impacts at 10686/9357. Extending survival
alone does not establish a safe maneuver.

The stronger [earlier no-escape reference](transfer-speed.md) had **3 captures
and 3 departures in each affected case**. Left steering restores those counts
here, and braking exceeds them, but this is still one retained world/seat with
correlated primary/health runs. The earlier eight-case primary reference had
2 wins, 28 captures and 27 departures, versus 2 / 26 / 25 before this experiment.
That broader suite was not rerun here; these selected continuations do not
establish a suite-wide improvement.

### Verification and evidence

Implementation, tests and the plan above were frozen in **`d97756d`** before
any trial. There was no outcome-driven change to pulse duration, trigger,
controls, physics or bot options, and no completed simulation was rerun.

Post-run checking caught a loss-summary omission: the initial runner watched
for ship-form or vehicle changes, but a ship destroyed while its pilot is on
foot can retain `form=ship`. The archive now independently scans the dense
physical `ships_lost` counter, preserving before/after pilot observations and
damage receipts for every increment. It correctly records the left-steering
loss at 10711. The runner also checks ship availability for future runs, including
the terminal step. Original raw results and their original hashes are retained;
the manifest distinguishes frozen and current runner hashes. This correction
changes reporting only.

All **1,091 Rust tests** and **757 Python tests** pass. Formatting, strict AI
Clippy and profiled/ordinary release builds pass. Scenario Clippy retains the
same seven pre-existing findings at the same locations. Across all trials,
**508,808 projectile rows** pass the diagnostic audit; at most four projectiles
are retained, at most 15 debris entries are scanned, and no shell is reported
physically unavailable or exceeds the 64-entry capacity. These cases do not
establish a general runtime bound.

The frozen binary is
`target/projectile-response/surface_mission_soak-d97756d`, SHA-256
`2b3a3db1b83475c7ed746a889c7fa02f093aa31cffb8d99546f7feeab34fac9a`.
The completed runner summary is `target/projectile-response/v1/summary.json`,
SHA-256 `925e00db0de04e329c51273fb64722a3b94d345c86dd1b2172aa9b8e63cba600`.
The [manifest](data/projectile-response-v1.json) records all 17 outcomes, contact
and loss receipts, input/tool/binary hashes, and **270 raw-file hashes**. The
[compressed archive](data/projectile-response-v1.json.gz) preserves the original
runner summary, pulse witnesses, dense damage changes, projectile tracks,
physical visit audits, report checkpoints and final reports excluding their
large samples/events arrays. Complete raw reports and streams remain at their
hashed local paths.

```sh
python3 tools/validate-projectile-response.py \
  --prior target/projectile-diagnostics/v1/summary.json \
  --binary target/projectile-response/surface_mission_soak-d97756d \
  --out /tmp/projectile-response-replay
python3 tools/analyze-projectile-response.py \
  --summary /tmp/projectile-response-replay/summary.json \
  --out /tmp/projectile-response-results.json
```

Use fresh output paths and a clean checkout for the runner. The archived
summary is from the original frozen runner; rerunning the corrected reporter
will intentionally change its loss-summary and witness hashes.

## Decision

Keep braking and left steering as laboratory candidates. The short response
can materially improve these speed-limited continuations, while the older
approach warns too late for braking to help and supplies a concrete regression.
No one fixed direction wins across both trajectories. Before any policy
integration, test the unchanged candidates across additional worlds and seats
with matched controls, including short-warning cases and full physical recovery.
Defaults and deployment remain unchanged; all work stays local.

The [broader 49-match comparison](projectile-response-sweep.md) now reproduces
these results across the retained suite and adds two new worlds in both seats.
Only the original trajectory activates; all fresh cases remain outside the
post-escape gate. It adds parity coverage, but no new active-avoidance example.
