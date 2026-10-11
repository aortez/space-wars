# Airborne acquisition defense: frozen qualification and fresh screen

This opt-in `airborne_acquisition_defense_v1` experiment follows the
[climb-laser lost-win diagnosis](pursuit-climb-laser-loss.md). It changes only
v13's response to a fresh laser/cannon hit while airborne, before its capture
task has ever selected a site or touched the surface. It does not change the
separate stale-recovery defect, opponents, physics, host budgets or defaults.

## Fixed behavior

After the native capture update, require a current, unrejected acquisition
receipt with no selected site or objective route: `scan_deferred` or
`candidates_rejected`. The ship must be available, aboard, flying with no
supported feet, armed and query-ready, on the mission's target planet. Require
a visible, unoccluded, living enemy ship within 300 units and a weapon-hit
receipt for this exact tick. A newly selected site takes priority.

Save that native capture and its three actions. Abandon/defer this acquisition
using the existing 30-second source-planet cooldown. Immediately use the existing
boundary-aware escape direction and flight guidance, with weapons from the
existing combat controller, including readiness, aim, visibility and scheduled
breaks. Solar safety, recovery and new ground contact retain priority.

The deadline is fixed at trigger + 720 ticks (12 seconds); later hits cannot
extend it, and higher-priority/disabled-control ticks consume it. Sustained
separation requires 60 ticks of positive target evidence, source clearance over
70 units and stopping clearance over 20 units: either terrain occlusion or at
least 350 units of non-closing range. Missing target evidence is not cover.
Resume ordinary mission selection on completion, still deferring pursuit until
the original deadline. Keep only bounded counters and the last attempt.

## Frozen cases

Run **24 complete games / 12 pairs**, ending naturally or at 600 seconds:

1. Four known qualification pairs. Replay the previous option-off behavior
   exactly before executing any defense-enabled qualification game:
   - Fresh world 2, v10, no asteroids, P1 integrated candidate, climb laser on:
     the diagnosed loss, seed 11223442104665788832.
   - The same configuration with climb laser off: the retained win.
   - Earlier qualification `candidate-laser`: the rescued draw, seed
     4180701290234409703, P1 integrated candidate, v10, asteroids every 3 seconds.
   - Earlier qualification `control-laser`: the retained ordinary-v13 win in
     that same world. Report this identity separately from the candidate.
2. Eight fresh pairs: two new generated worlds, integrated v13 against v10,
   each evaluated seat, asteroids every 0 or 3 seconds. Climb laser stays enabled
   for the evaluated seat in both arms. Seeds are the first eight SHA-256 bytes,
   little-endian, of `airborne_acquisition_defense_v1:held-out:2026-10:{0,1}`.
   Alternate pair submission order; at most two games run concurrently.

Within each pair only `--acquisition-defense-seats none` versus the evaluated
seat differs. Keep the shared execution-routes host, four graph operations,
384 physics queries, 4 Hz survey, integrated options, and 15/4-second combat
breaks from the preceding comparison. Other experiments remain disabled.
Preserve impact observers in the two earlier qualification commands; do not
add them to the other games. No live controller overrides are allowed.

## Evidence and predetermined decision

Commit runtime, this plan, runner and tests before games. Freeze the binary,
commands, source records and all imported audit tools. Require exact disabled
replays, non-timing report/sensor/planning agreement, dense per-tick action and
observation joins, configuration and physical visit/route/budget audits.
Audit each trigger against its saved native receipt and previous capture
history; audit immutable deadlines, separation evidence, control/weapon gates,
counter chronology, safety preemption and pursuit deferral. Require identical
pair state before the first handoff and identical source observation, native
capture and native actions at that handoff. With no handoff, require complete
gameplay parity after removing only the new option telemetry.

Advance only to a broader comparison if all of these hold:

- All 24 games and 12 pairs are complete and valid.
- The diagnosed loss improves points, ship losses or pilot survival after an
  actual handoff; no known pair loses points or adds ship losses/pilot deaths.
- At least one fresh pair improves points, ship losses, pilot survival or an
  actually completed departure after changed behavior.
- Fresh aggregate points do not decrease overall or within either seat,
  asteroid condition or world cluster. Fresh ship losses and pilot deaths do
  not increase overall; list every individually regressed pair as well.

Report completed visits, interrupted/unfinished visits, objective return,
recoveries, mission-progress metrics and all pair outcomes. Departure counts
are descriptive in this small screen: a shorter winning match can remove later
opportunities. Show the common-horizon completed visits and separately identify
baseline visits later than an enabled early victory; never call these observed
failed completions. Do not turn this into a post-hoc gate.

Any failed gate retains the experiment with its counterexamples. Complete the
matrix even if an outcome regresses; stop new submissions only for invalid
evidence or execution failure. No runtime retuning, rerunning selected losses,
default promotion, deployment, push or PR is part of this experiment. Two fresh
world clusters against v10 do not establish general strength or device speed.

Archive each generated pair losslessly after auditing; verify every member's
hash and size before removing its uncompressed copy. Keep prior evidence
intact, plus commands, logs, reports, full traces and compact review artifacts.

```sh
python3 tools/validate-acquisition-defense.py plan --out target/acquisition-defense/v1
python3 tools/validate-acquisition-defense.py run --plan target/acquisition-defense/v1/plan.json
```
