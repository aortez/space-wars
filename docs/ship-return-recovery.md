# Returning to displaced or inaccessible ships

This follows [claim footing recovery](claim-footing-recovery.md). The shared
ground and recovery tasks are now `ground_navigation_v8` and `recover_ship_v7`.

## Recorded failures and behavior

Two exact desktop replays from `92c72fa` establish the failures. Seed 7/P1,
mirrored duel with 3-second Mixed asteroid arrivals, ends beside a nearly
motionless full ship whose hull rests on a terrain step. Its feet do not qualify
for landing, so there is no grounded hatch. Seed 42/P1, mirrored intercept with
no environmental arrivals, leaves the spaceling on planet 1 while the opponent
displaces its empty ship to planet 0. The old ground task attempts to reach that
foreign hatch as if it were on the spaceling's planet.

Ground navigation now rejects a hatch on a different retained planet once the
ship has landed on both feet and the spaceling has actual support. An active
jetpack crossing finishes first. A missing hatch gets fifteen seconds to settle,
measured from its disappearance rather than from the beginning of prior walking.
The existing ninety-second ground-task deadline remains in force.

These two failures have explicit telemetry types. They allow the containing
recovery task to consider replacing the assigned ship; malformed observations
and unrelated route failures still produce their existing bounded failures.
Replacement requires a balanced, supported spaceling moving at no more than one
unit per second relative to its retained planet. For a nearby missing hatch,
the full ship must also be within 24 units, in the same planet frame, moving at
less than one unit per second radially and tangentially, and spinning at less
than 0.2 radians per second relative to that planet. A landed ship on another
planet remains independently identifiable through its two-foot support.

The bot holds the existing human scuttle chord (primary, interact and brake).
The shared world consumes the inputs and performs the actual loss after three
seconds. Neutral input rearms the surviving spaceling, which then uses the
ordinary claim, eight-second rebuild, hatch access and boarding rules. Fresh
hatch access cancels the hold. Dirty queries, support loss or unstable motion
release it; normal jump/get-up inputs can restore balance. The attempt has a
fifteen-second deadline within the original recovery budget, with at most one
completed replacement per task. Both host policies preserve that task across
the deliberate loss instead of restarting its clock.

There are no new human recovery rules, physics steps, gravity solves or world
writes from the policy. A ship still flying far away, a distant same-planet
hatch without a measured route, and general cave navigation remain outside this
slice. Replacement can require recapturing hostile ground, so starting recovery
near the three-minute cutoff does not imply a completed return by that cutoff.

## Physical return trials

The new `surface_return_soak` isolates the return leg from the preceding flight.
Its fixture places a spaceling and a reachable, tipped or foreign-planet ship
only at construction. Ordinary physics must establish support and raise the
initial flag. The task then emits normal inputs through boarding, and the host
applies normal thrust to verify departure. No ownership, landing or recovery
state is forced during the run.

```sh
cargo build --locked --release -p spacewars-ai --example surface_return_soak
target/release/examples/surface_return_soak \
  --seed 42 --seat 0 --mirror true --bearing 0 --case other-planet \
  --seconds 180 --out /tmp/surface-return
```

The matrix covers three conditions, both seats, both mirrored layouts and two
surface orientations: 24 three-minute runs per platform. Every run must board
and depart, conserve retained plus removed material, retain finite physics and
stay below the existing speed bound. Reachable controls must never require a
replacement. Reports preserve claim, scuttle, boarding and departure ticks,
task events, per-second physical audits and renderer frames.

Decision tests cover support loss, cancellation, moving-ship exclusion, dirty
queries, hatch settling time, replay, the replacement limit and host deadline
preservation. A physical integration test exercises all three return conditions.
The final validation also retains the 52 mission and 36 impact trials per
platform, the release workspace suite and the frozen ordinary-game navigation
and strategy baselines. Physical audit success and mission completion are
reported separately.

Artifacts and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ship-return-recovery-20260909/
```

## Validation at `8283fe1`

The release workspace suite passed 1,082 tests, with zero failures and 24
existing ignored tests. The example suites passed eight more: **1,090 passed
tests** in total, including eight new decision/physical regression tests.
Formatting passed. Clippy completed with advisory warnings in unchanged code;
the two style advisories from the preceding claim-footing slice were resolved.
The frozen ordinary-game baselines matched all six `navigation-v1` and twelve
`strategy-v1` episodes.

Each platform ran 52 missions, 36 impact trials and 24 return trials for three
simulated minutes each: **224 runs / 11.2 simulated hours**. All physical audits
passed. All 72 impact trials recovered and departed, and all 48 dedicated return
trials boarded and departed.

| Return condition | Desktop | Pi | Departure time from trial start |
| --- | ---: | ---: | ---: |
| Reachable assigned ship | 8/8 | 8/8 | 5.65–6.07 seconds |
| Tipped ship without grounded hatch | 8/8 | 8/8 | 32.37–32.52 seconds |
| Ship landed on another planet | 8/8 | 8/8 | 17.35–17.50 seconds |

The reachable controls preserved all sixteen original ships. Each of the other
32 trials completed exactly one replacement. These times include physically
settling and raising the initial flag before starting the return task.

| Mission outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: capture and depart both planets | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate ship loss: recover a replacement | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also finish both capture sorties | 5/8 → 5/8 | 6/8 → 6/8 |
| Combat/asteroids: finish both capture sorties | 17/32 → 17/32 | 15/32 → 15/32 |
| Completed recovery events during combat/asteroids | 9 → 10 | 7 → 7 |
| Final blocked mission subjects | 1 → 0 | 0 → 0 |

Three desktop report trajectories change: the two designated subjects of the
seed-7 mirrored duel world, and P1 in the seed-42 mirrored intercept. All 52 Pi
mission trajectories retain every per-second motion hash from `92c72fa`.
The new dedicated return fixtures establish the Pi behavior independently.

Both final desktop diagnostic replays match all 180 motion hashes of their
respective final matrix runs. The wedged-ship replay preserves the preceding
build's motion through second 178. It starts scuttling at tick 10504 (175.07
seconds), observes the completed loss at tick 10685 (178.08), and begins moving
to reclaim hostile ground. It is still recovering at 180 seconds; this case
does not establish a completed replacement within the original mission window.

The foreign-planet replay preserves the old motion through second 83, rejects
the impossible ground route at tick 5007 (83.45 seconds), then completes
rebuilding and boarding at tick 6671 (111.18). It resumes the mission, captures
planet 0 and departs at tick 9073 (151.22). Reports count this as one recovery
and one completed capture sortie.

The final Pi replay also enables `--require-claim-recovery true`. It matches
all 180 final matrix hashes and preserves the earlier regression fix: measured
claim relocation at tick 9450, followed by real ownership at tick 9777 (162.95
seconds). The three-minute cutoff still leaves that spaceling returning to its
ship. A completed claim is recorded separately from completed departures.

The Pi's largest per-case p95 measurements were 0.307 ms for sensors, 0.019 ms
for policy and 0.419 ms for physics. Recorded maxima were 17.115, 2.106 and
11.106 ms respectively. These headless measurements are separate from live
rendering. The Pi gameplay host was paused during the batch; desktop jobs
overlapped compilation and are not a controlled performance comparison.

The source checkpoint preceded the final runner and Yocto builds. Yocto
completed all 6,608 tasks (21 rerun). The archived image and extracted client
are identified by:

```text
source commit: 8283fe1eb77cdaead9b799eed247faeb488400c5
image SHA256:  1cc3424dd9abebc7841d84320b3477201c1b1c54da4552cb3df5a082b3cf8e59
client SHA256: 94c5f07fa8dbddae7606c30a003ae1994eb4f933f2bab700a8f0b4aec83b9523
Pi mission:   4fb868c9c712ee81f86cadc667ebe4e25b0f666c222c63bd5cb002ac8dad43e1
Pi return:    0facba83fb1be775fcb15ff9226960f4bae02cc1f91fab86542f6f8622756a42
```

## Deployment and live playtest

The image was installed on `spacewars.local`, which booted slot A (`/dev/sda2`).
The installed client hash matches the extracted image binary. The kiosk was
active and running with zero restarts and a zero exit status.

A live two-planet duel ran for three wall-clock minutes with 3-second Mixed
asteroid arrivals and raster scale 2. Captures at 30/75/120/180 seconds recorded
60.1/59.2/58.7/58.5 FPS, with zero kiosk restarts. Both bots captured their first
planets and travelled onward. At 180 seconds, P1 was landing from a jetpack
crossing to resume its ground route; P2 patrolled with both planets owned.
This checks the installed combined loop and rendering. The targeted return
recoveries are established separately by the physical runner fixtures.

The Pi was then returned to a fresh, paused `spacewars-terrain-travel` round:
P1 human, P2 mission bot, 8-second Mixed arrivals. Start or B resumes. Screenshots,
UI state/history, actual capture times and service diagnostics are archived
with the headless reports. This results update is documentation only; the
installed implementation remains `8283fe1`. Merging remains deferred.
