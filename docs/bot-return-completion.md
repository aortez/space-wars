# Sustained return walking and actual boarding

This follows the [moving-ground CCD correction](moving-ground-ccd.md). The
experimental v11 capture task now maintains ordinary walking speed between
compatible interior waypoints after claiming a planet. The controlled 115.3-unit
return boards and departs before its existing deadline. Historical v9/v10
controls, physics, landing selection and boarding permissions are unchanged.

## Execution change

The old follower requests `clamp(error * 1.8 / 5, -1, 1)` at every node. On a
dense, smooth route this repeatedly slows the spaceling for points it only needs
to pass. The v11 return task opts into `continuous_walk`, also recorded in ground
telemetry. False is omitted from serialization to preserve historical traces.

Full directional input requires balanced support on the retained planet, more
than 0.25 units of horizontal error, and measured Walk edges both into and out
of an interior node. Both legs must continue in the requested direction, with a
turn of less than 90 degrees. Other cases retain the previous controls:
airborne motion, posture recovery, sharp turns, overhead steps, jumps, powered
crossings, partial-route endpoints and the final hatch approach.

Waypoint consumption still requires physical proximity and support. It still
releases movement for that transition tick; this change removes the repeated
slowdown between nodes, without skipping nodes or steering across unmeasured
ground. Existing contact/revision invalidation, jump fallback, eight-second
progress watchdog, ground-task budget and capture deadline all remain intact.
Boarding still uses the scenario's real clearance, three-unit range, support,
balance and relative-speed checks.

This adds at most two edge lookups and two node lookups in the already bounded
map (512 nodes, at most 6,144 edges) per eligible control tick. It performs no new
physical query, graph search, forecast or world step. Clone/reset retain the
opt-in; repeated observations retain the existing action-cache contract. The
capture composition enables it only for v11's owned-planet return, including
after a destination change; outbound flag handling and recovery policies keep
their existing controls.

## Controlled return

Changing physics from world startup changes the earlier match history, so the
old source handoff at tick 11153 cannot be nominated in the current production
replay. The diagnostic harness preserves the old world and old bot until the
same claim, then enables corrected physics at tick 18228 in both runs. The
candidate also enables its new walking mode then. These temporary switches are
outside production and are retained with the evidence.

Both runs have the same nominated site 1:45, landing at 13318, exit at 13319,
and claim at 18228. All 4,871 recorded trace rows before 18228 are exactly
identical, including both players' observations and controls. Both then plan
the same 90-node, 115.297-unit return.

| Measurement | Corrected physics, old walking | Corrected physics, new walking |
| --- | ---: | ---: |
| Capture deadline | 21168 | 21168 |
| Boarding tick | None | 19891 |
| Departure tick | None | 20115 |
| Emergency jump commands / physical jumps | 0 / 0 | 0 / 0 |
| Worst sampled projected floor clearance | -0.0106 | -0.0113 |
| Last on-foot center distance to a clear hatch | 3.179 | 2.994 |

The candidate returns and boards in 27.72 seconds after capture, then departs
with ownership retained and 17.55 seconds left on the capture clock. The old
follower reaches its last waypoint but remains just outside boarding range at
expiry. No boarding-geometry correction or longer timeout was necessary in
this case. Clearance is a ray projection of the capsule, not exact penetration
volume.

The first, less guarded experiment boards at 19694 and departs at 19918. It is
archived as `full-walk`, but production retains the support and turn guards of
`guarded-walk`. In the final version, graph length divided by the nominal
five-unit walking speed predicts 23.06 seconds; actual return-through-boarding
takes 27.72. Waypoint transitions, contacts, final braking and the real transfer
still add time. This is one calibration point, not a universal speed multiplier.

## Fresh matches and limits

Eighteen paired configurations compare baseline `d6e5534` with the new version:
four seeds, both v11 seats against v10, quiet or three-second asteroid pressure,
plus two historical v9/v10 seat swaps. Matches run to a real result or their
600-second limit. All 36 finish with clean material/physics audits and obey
shared graph/query quotas. Historical comparison traces are byte-identical.

Only one configuration changes actual controls. The other seventeen preserve
their outcomes; the eight additional held-out configurations never exercise
the new walking input, so they cannot establish its strength or robustness.
Both players together complete 77 versus 78 sorties and 15 recoveries in each
version. These totals include subsequent changes in opponents' opportunities.

The changed configuration is a **v11 win-to-loss regression**: seed
`15270103591317955068`, quiet, P1 v11 / P2 v10. The first different input is at
14552 on the same owned-planet return. That return boards at 15350 instead of
16272 (13.43 versus 28.80 seconds after the same claim), and both versions then
complete another capture. The candidate later enters combat, loses a ship,
recovers, loses its ship again, and dies against the sun at 24429 during pod
recovery. The baseline wins at 30127 when its opponent dies.

This sequence does not show a new walking stall, but the worse match result is
real and remains in the comparison. Retain the improvement as an experimental
v11 movement capability; it does not establish a stronger strategic policy or
justify a default promotion. Faster boarding changes later engagements, which
still need their own time/risk evaluation.

## Tests and reproduction

177 release tests pass: 117 AI unit tests, 37 ground-navigation contracts,
two ground/jetpack tests, twenty physical mission tests and one new paired
walking regression. The latter uses the existing interpolated Round lab in
both seats: physically land, exit and claim, walk away, clone that world, then
compare the two returns. Both use actual transfer eligibility; the new controls
must actually activate and board sooner. The ground/jetpack test also exercises
both walking modes with and without a material edit before returning through
the opposite entrance. Synthetic contracts cover both directions, rotated
frames, clone/reset, cached ticks, final braking, jumps, sharp turns, lost
support, overhead targets and invalidated terrain.

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p spacewars-ai --lib --test ground_navigation --test ground_walk \
  --test ground_jetpack --test surface_mission
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
```

Client compilation, formatting and AI all-target/all-feature clippy with denied
warnings pass. No physics or scenario source is changed at this checkpoint.

Evidence lives in `target/bot-return-completion/`. Its verified archive is
retained at
`/home/oldman/.codex/visualizations/2026/09/19/bot-return-completion/`, with source
patches, runtime hashes, exact commands, traces, analysis and a file manifest.
`baseline.json` binds the previous binaries to `d6e5534` and its CCD evidence.
The archive's commit binding records this checkpoint separately from its parent.

Restore the evidence under the same `target/` directory in this checkpoint's
checkout. `run_matrix.py` and `run_holdouts.py` reproduce normal production
comparisons. `analyze.py` checks all 36 runs, both controlled candidates,
unchanged prefixes, milestones, material/physics audits and planning quotas.
The old return uses the separate diagnostic package:

```sh
cargo +1.89.0 build --release --locked --offline \
  --manifest-path target/bot-return-completion/controlled-replay/Cargo.toml \
  --features sensor-profile \
  --target-dir target/bot-return-completion/controlled-target
SPACEWARS_SWEEP_TICK=18228 SPACEWARS_WALK_TICK=18228 \
  python3 target/bot-return-completion/run_replay.py guarded-walk \
  target/bot-return-completion/controlled-target/release/return-walk-controlled-replay \
  --trace-ground-contacts true
```

The diagnostic package contains a copied AI source tree and a copied Rapier
dependency with the two temporary switches. It must not be deployed or used as
a normal match runtime. Its old-history completion must not be reported as an
unchanged production replay from startup.

The next planning step is to calibrate complete-trip time and exposure against
actual approach, claim, return, boarding and departure milestones. Preserve the
changed quiet seed as a risk/recovery investigation, and seek more naturally
activated long returns before making strategic or win-rate claims. Keep these
known inputs and the previous controller as comparators instead of extending
clocks or selecting a successful future trajectory by hindsight.
