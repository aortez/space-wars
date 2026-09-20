# Bounded physical capture-trip continuations

This extends the [six-second continuation experiment](bot-successor-continuation.md)
at `e39dda9` through approach, current landing selection, capture and return.
An explicitly nominated site can now execute the ordinary physical controllers
for up to three minutes. The experiment records each actual milestone and why
the attempt ends. It does not choose a successor or change a default bot policy.

## What we learned

Of nine nominated-site trips from the same three contested escape handoffs,
three reach their staging point and one lands and exits. None completes a claim,
boarding and departure. The landed pilot does neutralize the enemy flag and
start raising its own before the existing ground task times out. This is useful
partial progress, but it must not be scored as a captured planet or completed
round trip.

The follow-up [own-flag handoff fix](bot-claim-handoff.md) now completes the raise
in this recorded case and starts a real return task. That return still exceeds
the existing capture-task clock; the follow-up records the remaining traversal
problem and fresh paired comparisons. The measurements below retain the original
pre-fix experiment.

A separate isolated physics test completes every stage, including return to the
ship and departure with ownership retained. Thus the experimental handoff can
execute and recognize a complete trip. That fixture supplies the prior successful
escape history and leaves the physical opponent idle; it is not evidence that
the contested approaches succeed or that the policy is stronger.

The next focused fix is the transition from lowering an enemy flag to
raising one's own. The recorded landing succeeds, the jetpack crossing over the
parked ship succeeds, and the pilot reaches the enemy flag. After neutralization,
the joint ground planner treats the newly emerging own flag as a new objective
and selects another endpoint. A dense replay records repeated movement commands
followed by raising resets. Fixing that interruption is more specific than
increasing the trip timeout or changing strategic weights.

## Experiment contract

`SuccessorContinuation::for_capture_trip` accepts one of the measured site
proposals at an exact successful escape handoff. As before, replay preserves both
bots' memory, the live planning queue, weapons, physics and asteroid sequence.
The source tick's already-emitted control is common to every alternative;
intervention starts on the next tick.

The approach lasts at most 60 seconds, and the entire trip at most 180 seconds,
including that common source tick. During approach, failing to improve distance
by two units within 20 seconds also ends the experiment. This is a simple
bounded progress check, not a proof that a longer detour is impossible.
`best_approach_distance` is the last two-unit progress checkpoint, not the exact
minimum distance. The original escape clock is never extended.

Approach follows the existing site controller toward the point 85 units above
the sampled ship origin in the destination's material frame, transformed by its
current motion. A missing, changed or newly owned destination invalidates that
flight proposal. Ordinary solar escape and ship recovery retain priority.
Arrival requires distance below ten units and relative point speed below 18.
Local capture starts only with the correct current planet frame and ready
queries. Reaching the geometric point is recorded separately from that handoff.

The existing capture task is constrained to the nominated site and immediately
requests a fresh measurement of that ID. A general survey can omit it from the
eight-site objective shortlist, so a direct request is necessary. Selection
still requires current material, solar, cover and objective-route checks. An
unrelated physical landing cannot count as success: before exit, the selected
ID must match, its measurement must have the current material revision, and the
actual ship origin must be within ten units of the measured landing origin.

After entry, the normal capture/ground controllers own landing, footing, claim,
boarding and departure. They revalidate their own current dependencies; the old
approach revision is not imposed on every later ground action. On-foot controls
and the ordinary neutral transfer release are expected parts of the trip.
The coordinator retains its existing cross-frame departure handling.

Milestones come from actual observations and controller telemetry. Success
requires a recorded claim and boarding followed by the coordinator's departure
event, with ownership checked again then. Stopping freezes the report; a later
ordinary capture cannot be credited to the trial. On timeout, an active capture
task loses the experimental site constraint but keeps its actual state, so an
on-foot pilot retains its return task. An unfinished trial at match end is
reported explicitly. The experiment does not renew local task deadlines.

The shared physics step and planning allowance remain intact. Forecast jobs
still use only graph work left after local planning and cover. Historical
forecast tails supply neither landing permission nor long-horizon predictions.
Explicit trial selection and diagnostic serialization are outside policy timing;
the actual controls and delegated capture run inside it.

## Contested results

All nine interventions are P2 v11 against P1 v10. The main seed is
`7725194555774358125`, with quiet and three-second asteroid variants; the second
seed is `2`, also with three-second asteroids. Boundary, cover and successor
experiments are enabled, with the same live objective adapter and shared quota
as the parent. These are three selected source states on two seeds, not held-out
or seat-swapped policy comparisons.

Times below are seconds after the common source command. The complete match
continues under ordinary mission control after a trial stops, up to its normal
600-second timer. A later win does not establish that the nominated trip worked.

| Source / site | Arrival | Landing / exit | Trial ends | P2 result / match end |
| --- | ---: | ---: | --- | --- |
| Quiet, tick 11153 / 0:34 | 12.35s | — | Recovery, 14.90s | Loss / 600s |
| Quiet / 0:0 | — | — | Recovery, 13.78s | Loss / 468.27s |
| Quiet / 1:45 | 16.88s | 36.08s / 36.10s | Ground deadline, 126.13s | Loss / 579.30s |
| Quiet / 1:11 | 38.78s | — | Left destination frame, 44.20s | Loss / 600s |
| Asteroids, tick 6802 / 2:22 | — | — | Material changed, 4.73s | Win / 139.13s |
| Asteroids / 2:56 | — | — | Material changed, 4.73s | Loss / 127.52s |
| Asteroids / 1:48 | — | — | Material changed, 9.32s | Win / 600s |
| Seed 2, tick 4686 / 2:7 | — | — | Material changed, 10.38s | Loss / 537.47s |
| Seed 2 / 0:44 | — | — | Solar avoidance, 29.67s | Loss / 193.20s |

The two planet-2 asteroid trials stop before six seconds and retain the parent's
outcomes. Extending quiet 1:45 and both seed-2 approaches changes prior wins into
losses; extending asteroid 1:48 changes a loss into a timer win despite never
reaching its staging point. These outcomes reject an assumption that longer
commitment necessarily helps. They do not identify a general successor ranking.

### The landed ground leg

Quiet 1:45 lands at tick 13318 with a measured position error of 0.0354 units and
exits at 13319. Its ground task starts at 13320. A leftward jetpack crossing of
the parked ship starts at 13335 and completes at 13808, retaining about 11.1%
charge at its lowest point. The remaining route is around the planet to the
existing enemy flag; the ship crossing itself is not the stall.

At tick 17940 the pilot has reached the planned flag endpoint, is supported and
balanced, and lowering is 40% complete. At 18030 lowering reaches 90%. By 18060
the enemy owner is neutralized and P2's own flag is rising, but the ground goal
has changed from `arrived` to `survey`, with a different endpoint. Subsequent
samples repeatedly show only about 6–7% raising progress. At 18660 the ground
controller is walking toward that new endpoint; at 18720 support and partial
raising progress are lost. The original 90-second ground deadline fires at
18721. Ownership was never secured during the trial.

A second replay records every physical tick from 17700 through 18739, with the
same continuation observations, controls and eventual result. Neutralization
occurs at 18048. It exposes 19 resets to `need_settle`, each immediately after a
nonzero walking command while the pilot remains supported and balanced. The
final reset to `need_support` at 18693 follows a jump emitted at 18692. The
twice-per-second samples had mostly landed between these repeated movement
pulses and therefore understated the control problem.

The code explains why an own flag can become a traversal target:
`LandingObjective::read` tests the planet's completed ownership, not the flag's
player. A partially raised P2 flag on a neutral planet therefore remains an
objective. `planned_flag_target` invalidates the old enemy objective and selects
a new endpoint; `TacticalCapturePilot` lets an unfinished ground action replace
the base capture controls. Together with the dense trace, this identifies a
self-interrupting ground route after neutralization. A controlled fix is still
needed to establish whether preserving the raise completes this contested trip;
successful return and survival must not be assumed.

The next behavior experiment should distinguish an active own claim from travel
to an enemy flag, preserve current support/claim checks, and verify continued
raising followed by a freshly valid return. Test interruption by actual support
loss, flag destruction, ownership change and missing hatch as well as the
positive case. Reuse this exact handoff before a broader paired comparison;
do not lengthen the clocks to hide the failure.

## Verification and reproduction

- 116 AI unit tests, 20 physical mission tests and one physical combat test pass
  (137 total). Eight new tests cover the trip lifecycle, progress/deadline stops,
  fresh nominated-site selection, refusal to exit at an unrelated landing,
  constraint release, and two positive physical trips.
- The full-protocol positive fixture uses real measured sites and unchanged
  subsequent physics, with supplied prior handoff history and an idle opponent.
  It uses ordinary synchronous observations; the contested matrix exercises the
  live shared adapter. These are distinct verification scopes.
- All 17 matrix matches finish with clean material/physics audits. Nine extend
  site trials. Three observation-only, three old six-second interventions and
  two disabled controls match their parent in complete recorded traces, round
  results, cover and local work (excluding only dispatch timing).
- Every extended trial matches the recorded parent prefix through the common
  command and its resulting next observation. The first six seconds' per-tick
  observations and controls before the old deadline match the corresponding
  short trial. Pinned source forecasts remain identical; shared graph and query
  allowances hold. Stopped reports remain immutable.
- One additional complete diagnostic replay provides 1,040 consecutive ground
  samples for quiet 1:45. Its continuation, cover and source forecast logs are
  byte-identical to the matrix run, with the same full match result. The analysis
  verifies all 20 raising resets and the preceding physical commands.
- Formatting, client compilation and AI/core all-target clippy with `--no-deps`
  and denied warnings pass. The final runtime is byte-identical to the one used
  for the matrix. This is not a Pi timing or dependency-wide lint claim.

Use Rust 1.89.0 and `--locked`; tests use `RUST_MIN_STACK=16777216`.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --seed 7725194555774358125 --asteroid-interval 0 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --disengagement-boundary true \
  --probe-disengagement-handoff true --probe-destination-cover true \
  --probe-successors true --successor-step-cap 128 \
  --continue-successor 1:45 --continuation-seat 1 --continuation-tick 11153 \
  --continuation-sortie true --trace true --out /tmp/quiet-successor-trip
```

`--continuation-sortie true` requires an explicit site or `observe`. Escape and
combat retain the old six-second interface and cannot acquire a longer clock
through this flag. Default `false` preserves existing behavior and report shape.
Extended logs sample every tick through six seconds, then twice per second and
on mission phase/milestone changes through one second after stop (at least the
initial six-second window). Observation-only logging continues up to 180 seconds.

Final evidence is in `target/successor-sortie/`, with a durable verified archive
at `/home/oldman/.codex/visualizations/2026/09/19/bot-successor-sortie/`.
Commands, source bindings, final logs, reports, observations, comparison scripts
and required parent evidence are retained. The preliminary three-case pass
predates the direct site request and is excluded from final evidence, as are
superseded test logs and large sensor timing files. No default promotion,
deployment or push is part of this checkpoint.
