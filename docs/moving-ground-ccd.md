# Moving-ground CCD velocity correction

This follows the [return-walking investigation](bot-return-walking.md). Rapier
0.34.0 was discarding the CCD velocity of kinematic ground because that ground
does not itself have CCD enabled. Retaining that velocity fixes the reproduced
small-actor trapping without changing bot controls, terrain colliders, solver
settings, prescribed planet motion or capture deadlines.

The controlled return now reaches the final walking waypoint without emergency
jumps. It still expires before boarding. This checkpoint fixes the contact
defect; waypoint speed and the remaining boarding approach are the next task.

The subsequent [sustained-walking change](bot-return-completion.md) completes
that controlled return, including real boarding and departure within the same
deadline. The measurements below remain the physics-only checkpoint.

## Cause and correction

`velocity_solver::writeback_bodies` records interpolated motion in `ccd_vels`
only for CCD-enabled bodies and otherwise writes zero. However,
`TOIEntry::try_from_colliders` also reads the other body's `ccd_vels` when
rejecting pairs whose relative speed is too low to need a sweep. Thus a small
passenger moving with a planet can appear to be approaching stationary ground
at the speed of the entire system. An unnecessary sweep against the ground's
prescribed future pose can clamp the actor into the surface.

The focused regression uses two floor tiles moving at 24 units/second and a
0.15-radius ball with the same normal velocity, a 0.01-unit gap and 2.5 or 5
units/second of tangential movement. Its relative movement per frame is safely
below the combined CCD thickness. The unpatched test fails on the second step.
With retained ground velocity it crosses the seam without losing motion.

The local correction changes the writeback condition to retain velocity when
`ccd_enabled || is_kinematic()`. It uses the existing interpolation, including
rotation about the center of mass. It does not enable CCD or motion clamping on
the prescribed ground. The earlier [kinematic subdivision correction](terrain-chunk-compounds.md#prescribed-motion-during-ccd)
remains in place.

The workspace vendors and pins the existing Rapier 0.34.0 release. Only this
writeback predicate and its comment differ from the published Rust source.
[Vendor notes](../vendor/rapier2d/SPACEWARS_PATCH.md) record the original package
checksum, license, tests and removal condition.

An initial experiment changed `TOIEntry::body_motion` instead. It passed the
walking reduction but delayed genuine impacts against a receding wall, because
the kinematic `ccd_vels` was still zero. That experiment is rejected. The final
regression also requires a fast ball to catch a moving wall, emit a real impact
and match its velocity while the wall continues moving. The production sweep
implementation is unchanged. This is not a general replacement for Rapier's
CCD approximation at high relative speeds.

## Saved terrain and controlled return

The expanded two-body comparison starts from both exported states: 18300 before
the original stall and 18330 during it. Each runtime runs 192 eight-second cases:

- Separate/compound colliders and original/recentered/stationary frames.
- Both walking directions at half and full input.
- Position/velocity-driven kinematics.
- Engine defaults and Spacewars' length unit 10, eight solver iterations and
  two stabilization iterations; both use four CCD substeps.

Both runtimes use the same saved material, initial poses, motion and gravity.
These are reductions: gravity rotates with constant planet spin, and the
original other bodies and controller cache are absent. They are not full-match
replays. The 18330 cases intentionally start from an already embedded pose.

After the initial half-second settling window, the baseline has 63 cases with
projected clearance below -0.03 units and 43 with less than 85% of expected
eight-second travel. The correction has neither failure in all 192 cases; its
worst sampled clearance is -0.019. Clearance is a ray projection of the capsule,
not an exact penetration volume. Half/full input nominally travels 20/40 units.

Changing physics from match startup changes earlier events. The production
runtime no longer has a successful handoff at the nominated source tick 11153;
the replay guard correctly rejects that request. To compare the actual saved
return, a temporary diagnostic runtime retains old physics until tick 18300,
then enables the final writeback correction. Its 5,017 recorded prefix rows are
exactly identical to the retained baseline, including the same landing, exit
and claim at 13318, 13319 and 18228. The first changed trace row is at 18314.
This diagnostic switch is archived, not included in production.

From tick 18300 through 21167:

| Measurement | Before | Corrected |
| --- | ---: | ---: |
| Emergency jump commands / physical jumps | 19 / 19 | 0 / 0 |
| Worst projected clearance | -0.218 | -0.0032 |
| Last waypoint index, in a 90-node path | 41 | 89 |
| Local center displacement | 49.63 | 107.79 |
| Center distance to the final route target | 65.02 | 3.27 |

Displacement is the straight-line change in planet-local center position, not
path length. The route is 115.3 units long. Both runs still hit the unchanged
capture deadline at 21168 with no boarding/departure. The correction removes
the repeated contact stalls and exposes a much smaller remaining execution gap;
it does not establish that the complete nominated sortie succeeds.

Next, retry the previously rejected walking-speed experiment on this corrected
physics: avoid braking at every ordinary walking waypoint, retain careful
control at jumps and the final hatch, then measure actual boarding/departure.
Compare measured ground time with the landing scorer's nominal five-unit speed.
Keep the clocks unchanged while assessing that behavior.

## Shared-world acceptance and timing

669 release tests pass: 92 engine physics, 461 Spacewars scenario and 116 AI
tests. These include moving support, rotated/offset kinematics, snapshot replay,
mining/contact identity, three-minute round-planet running, flight, combat,
claims, recovery and completed surface missions. The new passenger regression
fails on the published dependency and passes the local patch; its 16 variants
cover both kinematic modes, layouts, speeds and one/four CCD substeps. Four
moving-wall variants retain genuine impact events and response.

Ten paired full-match configurations use retained baseline `96971fc` and the
correction. Eight cover two fresh seeds, both v11 seats against v10 and quiet/
three-second asteroid pressure; two swap historical v9/v10 seats. All twenty
matches finish with clean material/physics audits and recorded planning quotas.
Both players together complete 53 sorties before and 51 after, with 6 versus 9
recoveries. Match lengths and winners change. This tests the shared physics
change across policies; it is not a comparison of bot intelligence or a v11
strength claim.

Sequential desktop timing runs cover 180 seconds each, one fresh seed, v11 in
P1 against v10, with quiet and three-second asteroid conditions. Each cell below
is the median of three runs after an excluded warmup pair. Every run advances
10,800 ticks. Timing excludes rendering and bot observation/planning work.

| Rapier step time | Before mean / p95 | Corrected mean / p95 |
| --- | ---: | ---: |
| Quiet | 0.0526 / 0.0868 ms | 0.0515 / 0.1019 ms |
| Asteroids every 3 seconds | 0.0596 / 0.0871 ms | 0.0725 / 0.1348 ms |

The asteroid case has more work after the correction: mean solver contacts
increase from 0.59 to 4.59 as the physical trajectories diverge. This coincides
with the extra time; these runs do not isolate the cost of the predicate itself.
The absolute desktop costs are small, but this is neither a rendering/FPS
measurement nor evidence of unchanged Raspberry Pi performance.

Client compilation and formatting pass. Engine all-target clippy completes with
one pre-existing `collapsible_else_if` warning in `spaceling.rs:395`; the denied-
warning attempt stops there. No new lint warning is introduced.

## Reproduction and evidence

Use Rust 1.89.0 and the lockfile:

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p engine-rapier -p scenario-spacewars -p spacewars-ai --lib \
  --features spacewars-ai/sensor-profile
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
```

Evidence is in `target/moving-compound-contacts/`, with a verified archive at
`/home/oldman/.codex/visualizations/2026/09/19/moving-ground-ccd/`. It includes
the source patch, baseline/final runtimes, rejected experiment, exact commands,
saved material and traces, comparison programs, profiles and test logs.
`experiment-scopes.json` separates final results from the rejected sweep.

After restoring the archived directory beneath `target/`, `run_isolated.py`
repeats the saved-material matrix using retained binaries. `run_relative_matrix.py`
repeats the final match configurations. The temporary `controlled-replay/` and
`controlled-rapier/` packages reproduce the same old-history comparison:

```sh
cargo +1.89.0 build --release --locked --offline \
  --manifest-path target/moving-compound-contacts/controlled-replay/Cargo.toml \
  --features sensor-profile \
  --target-dir target/moving-compound-contacts/controlled-target
SPACEWARS_SWEEP_TICK=18300 python3 target/moving-compound-contacts/run_replay.py \
  reproduced-controlled-trip \
  target/moving-compound-contacts/controlled-target/release/moving-ground-controlled-replay \
  --trace-ground-contacts true --probe-ground-tick 18300 --seat 1
```

`SPACEWARS_SWEEP_TICK` is the diagnostic switch's historical name; this final
runtime switches velocity retention, not the rejected sweep. The ordinary game
has no switch and always uses the correction. This checkpoint makes no bot
default change or deployment.
