# Combat firing opportunities (#155)

The integration baseline is merged main `44623ecc98d5a689d2ca94cfa402fc7be34c8255`:
the #204 durability rules and #201 physics work together. The local client builds
and all 456 existing AI unit tests pass. This investigation uses the production
Value v13 / Recovery v1 host, its shared planning quota, 15s/4s combat breaks,
Weak Round/Angular dirt with slumping, and native 600-second match endings.
Recovery v1 uses `terrain_recovery_supported_hatch_ordinary_offsets_v1`.

## Retained diagnosis

Four discovery games reuse seed `6365347488789871033` from the durability work,
with both dirt shapes, both Recovery v1 seats and no random asteroids. Neither
health nor controller state is edited during these matches. Every tick records
the actual controls, ship/target motion, visibility, readiness and supply, both
aiming angles, mission/flight safety states, breaks, clearance and native hits.

At tick **4990**, P2 in the Round game is actively engaging an unoccluded pod
123.205 units away. Its current heading error is **-0.00542 radians**, but the
missile intercept error is **0.14223**. The shared 0.08-radian firing gate rejects
both weapons, although the laser is ready. Five steps replayed on copies of the
same physical state, changing only the laser request and preserving the control
run's subsequent inputs, produce **five additional native laser-hit ticks**.
The primary game remains unchanged. These controlled probes include the beam's
50-unit-per-tick growth; target-center visibility alone is not called a hit.

The first four games contain no direct climb → engage → climb sequence with an
engagement shorter than two seconds. This does not disprove the reported
oscillation in other worlds. Clearance and avoidance behavior remain unchanged.
Existing pursuit-climb laser experiments remain disabled; their earlier mixed
results are documented in [the separate comparison](pursuit-climb-laser-results.md).

## Candidate and frozen comparison

`material_direct_laser_window_v1` is an opt-in experiment. It uses the current
target bearing for laser eligibility while retaining the missile intercept for
steering and cannon fire. It changes neither the 0.08-radian tolerance nor the
250-unit laser range. All safety, mission priorities and deliberate breaks run
before this check. Standard bot selections keep their existing behavior.

Before inspecting fresh results, the comparison is fixed as follows:

- Seeds **6603551523184921372** and **3297281283817221183**, derived from the first
  eight SHA-256 bytes of `combat-155-heldout-v1:0` and `:1` in little-endian order.
- Both dirt shapes, asteroid intervals 0/8 seconds, both evaluated seats:
  **16 control/candidate pairs**. Only the Recovery v1 seat receives the candidate
  firing gate. Its recovery profile and the opponent remain identical per pair.
- Keep audits, outcome, survival, ship losses, recovery/mission completions,
  recorded damage/contact provenance, weapon hits, energy readiness, direct-aim
  opportunities and clearance transitions. Report opportunity counts on each
  actual trajectory, not as interchangeable denominators.
- Check the first different input: it must only change the evaluated laser
  request. Separately replay discovery controls with probing disabled and demand
  identical native actions and scenario hashes to establish diagnostic parity.
- Advance the candidate only if it passes physical/work audits and the isolated
  gate checks, improves the demonstrated firing defect, and does not reduce
  total match points or increase evaluated-seat pilot deaths/ship losses.
  Preserve every lost win and failed committed mission for review. Completions
  unavailable after an earlier native victory are censored, not failed missions.
  A failure means retain the current default, without changing this screen.

This is two fresh world clusters, not evidence of a general win-rate increase.
Desktop timings exclude diagnostic probes and trace serialization. Current
device cost and live playtesting are required before a default rollout.

The runner copies/hashes its executable and preserves source changes, case
specifications, commands, per-tick inputs and diagnostic results under a new
output directory:

```sh
cargo +1.89.0 test --locked --profile ci -p spacewars-ai \
  --features sensor-profile --lib --no-run
python3 tools/run-combat-opportunities.py target/combat-155/control-v1 \
  --binary target/ci/deps/spacewars_ai-4e4b7bd406488e9d \
  --seeds 6603551523184921372 3297281283817221183
python3 tools/run-combat-opportunities.py target/combat-155/candidate-v1 \
  --binary target/ci/deps/spacewars_ai-4e4b7bd406488e9d \
  --seeds 6603551523184921372 3297281283817221183 --direct-laser-window
```

Use the executable path printed by the build; Cargo's suffix can differ across
hosts. All full matches run native controllers with no resets or input overrides;
the optional five-step probes are isolated diagnostics, explicitly marked in
their own file. No device is changed by this runner.

## Completed comparison

All **32 games / 16 pairs** passed native physics/material and shared planning-work
audits. The candidate passes the predeclared screen and advances to review.
The current bot selections remain unchanged. These are two fresh world clusters;
the result does not establish a general win rate or device performance.

| Evaluated Recovery v1 seat | Control | Direct laser window |
| --- | ---: | ---: |
| Wins | 8 | 10 |
| Ship losses | 7 | 7 |
| Pilot deaths | 7 | 6 |
| Completed capture departures | 38 | 47 |
| Completed recoveries | 6 | 7 |
| Native laser-hit ticks | 4,598 | 7,074 |
| Eligible direct-aim windows taken / available | 7,862 / 9,478 | 13,908 / 13,908 |

An eligible window requires the present-bearing/range test, target visibility,
query and laser readiness, active combat, no scheduled break, and no cannon
request suppressing the laser. It is an opportunity gate, not guaranteed damage:
the beam still has to grow and intersect the actual moving collision shape.
The waiting-for-a-site patrol delegates to the same combat pilot and is included.
Capture, recovery, clearance and scheduled-break refusals are reported separately.

There are **13 changed pairs** and three with identical complete native input and
scenario-hash streams. Every changed pair has identical preceding scenario hashes;
its first different input changes only the evaluated seat's laser request.
The four discovery controls also match all **102,815 ticks** of input, controller
decision hashes and scenario hashes with the isolated diagnostic probes disabled.

The three gained wins are first-world Round/P1/asteroids, first-world
Angular/P2/asteroids, and second-world Angular/P2/asteroids. One win is lost:
first-world Angular/P1/asteroids. Points by world are 4→5 and 4→5; by dirt shape
4→5 for each; without asteroids 4→4 and with asteroids 4→6. These aggregates
overlap and do not turn sixteen pairs into sixteen independent worlds.

Both match duration and opportunity counts change after the intervention.
Summed laser contacts are therefore not an equal-exposure accuracy comparison.
Recorded debris-contact ticks increase 274→324 across the evaluated seats while
their total elapsed ticks increase 283,656→349,651. Weapon energy averages across
the sixteen per-game means are 94.63%→93.92%; recharge ticks increase 600→1,259.
There is no separate flight-fuel pool in these observations. Recorded damage
amounts/source labels are retained per case, including terminal pilot hits;
simultaneous full-ship damage is attributed to the native last source for that
tick, so those labels are not a causal damage decomposition.

No pair records a direct climb→engage→climb cycle with an engagement shorter than
two seconds. The longest continuous climb is 567 ticks in control and 574 in the
candidate (9.45/9.57 seconds). This narrow screen does not resolve every form of
the reported oscillation, and no clearance policy is changed by this work.

### Retained counterexample and completion review

For seed `6603551523184921372`, Angular, eight-second asteroids, evaluated P1:

- The first added laser is at tick **3258**. Both runs retain capture departures
  at **2671** and **7095**, with identical arrival/departure events through them.
- Control wins at **9154** when P2 dies from an on-foot world-boundary impact.
- Candidate continues. A later flag trip is rejected for lack of a grounded
  hatch at **11808**; recovery completes at **13588**. Further approach-frame
  retries are retained, and P1 exits onto the surface at **17281**.
- P1 dies from a native missile hit while on foot at **19374**, losing the match.
  This is a real lost win. The first changed request identifies the intervention;
  it does not establish a causal chain from one beam to that much later death.

The only pair with fewer evaluated-seat departures is seed
`3297281283817221183`, Round, eight-second asteroids, P1: **2→1**, both wins.
Both complete their first departure at **16102**. Candidate pauses the later
transfer for a vulnerable opponent at **18623** and wins at **19056**. Control's
second departure is **20328**, after the candidate's native ending. That completion
is unavailable in the shorter winning match; the trace does not prove the
interrupted transfer would have completed if play continued.

## Validation and retained evidence

The final source passes **461 AI unit tests**, **21 physical combat/mission
integration tests**, three analysis tests, formatting, required Clippy and the
client build. Standard constructors leave the option disabled; both mission and
combat resets preserve an explicitly selected profile.
Merged main's [eight CI checks](https://github.com/aortez/space-wars/actions/runs/38169193384)
also pass. Validation of this candidate branch is local.

The frozen comparison binary SHA-256 is
`d930f61b9520fb9a80e3dd477428076159f16c80c2181f5064f7a82fd329a89d`.
The [machine-readable report](data/combat-opportunities-v1.json) binds the source
snapshots, original plan, matrices, per-case results and retained trace hashes.
Raw evidence is under `target/combat-155/{discovery-v1,parity-v1,control-v1,candidate-v1}`.
The retained plan is the pre-run copy of this document in each matrix's
`untracked-source/docs/combat-opportunities.md`; later results do not overwrite it.

Desktop sensors + both decisions + planning measure mean/p95/p99/max
**0.202/0.118/4.464/14.601 ms** in control and
**0.198/0.121/4.507/13.728 ms** in the candidate. Timings exclude tracing, probes,
rendering and physics; the two trajectories differ and this is not a device-cost
comparison. The additional gate performs scalar math with no new physics queries.
Existing device measurements remain tied to their earlier binaries. Review,
independent-world coverage and current-device playtesting precede any rollout.

```sh
python3 tools/analyze-combat-opportunities.py target/combat-155/control-v1 \
  --candidate target/combat-155/candidate-v1 --out /tmp/combat-comparison.json
python3 -m unittest discover -s tools/tests -p test_combat_opportunities.py
```
