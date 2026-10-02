# Destination reconsideration after capture failure

The [qualified-cover experiment](capture-cover-response.md) can end a blocked
approach earlier, but its directed failure returns to the same planet. This
investigation asks which part of destination selection creates those returns,
what failure information survives, and whether current route evidence resolves
the recorded obstruction. It does not change the controller or replay physics.

## Native control paths

`MaterialMissionPilot::reconsider` retains a deferred planet ID and an expiry
30 seconds after failures that request deferral. A terminal capture failure uses
that path. Recovery, leaving the destination's approach frame, and an opportunistic
pursuit use non-deferring paths. The capture task is dropped on reconsideration;
its reason stays in bounded mission event history. Deferral itself retains no
failure category, cover state, material revision or route result.

Both initial selection and the value-switch gate reject a still-deferred planet.
After expiry, initial selection chooses the nearest eligible planet, preferring
another planet over the current local frame when an alternative exists. The
value evaluator needs an existing current target to propose a switch; it does
not make this first selection. An accepted switch also passes the controller's
altitude, descent commitment, recovery and once-per-trip gates.

Pursuit runs before destination selection and has its own 30-second budget.
Those clocks run concurrently. A pursuit starting just after capture failure can
therefore consume almost the entire deferral interval before the next capture
destination is selected.

The v13 value model compares conditional completion time per ownership unit.
Taking an enemy flag contributes two units once the bot has its own foothold;
a neutral capture contributes one. Published route certificates validate their
material/flag dependencies, but their timing scope explicitly excludes native
arrival, acquisition and exposure. A freshly revalidated certificate does not
establish that the exposure which blocked a previous attempt has cleared.

## Frozen audit plan

Use every original run in the completed `target/cover-response/v2/summary.json`:
32 directed runs, 32 fresh-world runs and ten recorded regression replays. Audit
only the tested v13 seat, retaining arms and case groups separately. The same
worlds and control trajectories recur across this source study; event counts are
not independent strength samples. Do not run additional matches or change a
coefficient, timer, policy, default or frozen outcome.

For every abandoned visit:

- Bind its selection and first terminal event to the existing visit audit.
  Missing history remains unverified.
- Classify deferral only from an explicit coordinator call path or a witnessed
  terminal capture failure from that exact visit. Missing failure snapshots
  remain unknown rather than acquiring a guessed cooldown.
- Find the next recorded selection of the same planet. No observed return is
  horizon censoring, not proof of permanent avoidance or success.
- Preserve intervening visits and paired pursuit starts/ends. An unfinished
  pursuit has unknown duration. Check every verified deferred return occurs at
  or after its expiry.
- Distinguish initial-picker selections from actually accepted value/time
  switches. Reconstruct accepted switches from their exact raw source forecasts,
  not from a hypothetical preferred candidate or a later report.
- Retain conditional costs, ownership values, material revisions, source and
  validation clocks, and the return visit's actual outcome. Fresh route evidence
  and matching material revision do not prove unchanged or resolved exposure.

Verify report/evaluation hashes against the frozen study. Pin the analyzed native
source files to the original runtime commit and freeze this auditor, tests and
plan before aggregating results. Retain any failed analysis attempts. No new
hardware-performance or gameplay-strength conclusion follows from this audit.

```sh
python3 tools/analyze-destination-revisits.py \
  --study target/cover-response/v2 \
  --out target/destination-revisits/v1
```
