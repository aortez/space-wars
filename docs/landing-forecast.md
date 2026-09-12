# Landing forecasts must move their own vehicle

Follow-up: [transfer detours and pursuit climbs](mission-route-clearance.md)
addresses the two mission failures retained below. This report preserves the
landing fix's original evidence and results at `19dfb4d`.

This follows the [rounded-match comparison](rounder-planets-matches.md).
The mirrored seed-42/P1 delay is a clearance-forecast bug: the bot's approaching
ship obstructs its own proposed hatch. The survey tests that hatch against the
ship's current airborne pose, and the tactical controller responds to the
temporarily missing site by lifting away from it.

## Evidence

The original one-second samples could not distinguish hovering from transient
collisions. The mission runner now accepts `--trace-start-tick` and
`--trace-end-tick` to record every observation/action in a half-open tick window.
It also writes decoded turn, thrust and brake commands. These are read-only
measurements, outside the timed sensor/policy/physics operations. All forty
per-second baseline samples match the archived sparse-trace replay exactly.

A replay of the rounded baseline records ticks 1,440–2,399. Throughout the
540 consecutive ticks from 30 to 39 seconds, the hull and both feet have **zero
solver contacts**. The selected site is missing on 255 ticks; all 255 issue
braking, including 150 with thrust. The other 285 ticks retain the site and
release both controls. The first missing-site observation is at tick 1,802.
The local landing telemetry still says it is settling, because the tactical
clearance hold interrupts it before invoking the landing controller.

At tick 1,890, one forecast hatch capsule is blocked in the live world and clear
when only the subject ship is excluded. The other obstacles and planet remain
in that diagnostic query. The actual hatch beside the live ship is clear; it
is the *proposed* hatch's overlap with the approaching hull that matters.
The ship repeatedly descends, interrupts descent, rises, and resurveying restores
the site. Its approach eventually exhausts its existing progress budget.

This identifies forecast-driven controller interruptions; no completed tick's
solver-contact manifold contains hull or foot support during the recorded stall.

## Change

Landing surveys now exclude the subject's current ship and actor from the world
clearance query, then explicitly test the real vehicle geometry at the proposed
landing pose. Hull, feet, collision groups, terrain and other solid obstacles
remain part of the prediction. The preview reuses the same assembly geometry
already used for rebuild and flag-route forecasts.

Live and proposed hatch searches share the same candidate order, first clear
floor selection and blocked-floor fallback. The forecast additionally requires
room for radial standing and the existing bounded slide/tilt margins. It cannot
choose a different search rule from the real exit to hide a blocked hatch.

The physical landing gates, transfer permissions, controller inputs, retry
budgets, mission deadlines and terrain geometry are unchanged. This corrects a
shared survey, including pods, without moving any live body during measurement.

The focused regression places an arriving ship across its proposed exit. It
checks that the live capsule really is blocked, the proposed site remains valid,
the survey leaves the physics snapshot unchanged, and no landing/exit permission
is granted. Adding a separate solid obstacle must still invalidate the site.
The physics assembly-preview regression independently checks proposed clearance
against inserted, rotated collider geometry.

## Results and remaining work

The mirrored seed-42 replay reaches its first planet at the same tick, 977.
With the corrected survey it lands at tick 1,883 (31.38 seconds), claims,
boards and departs at tick 2,301 (38.35 seconds). The baseline landed at 6,077
(101.28 seconds) and departed at 6,490 (108.17 seconds), after three approach
retries. The corrected first approach needs none.

The replay completes all three trips by tick 7,944 (132.40 seconds), starts
pursuit on the next tick and records physical weapon hits within the original
90-second chase window. Neither the 180-second preparation limit nor the
required captures, boarding, departure and hits are relaxed.

Seed 3/P2 still misses the three-trip deadline. Its first two departures and
third arrival are unchanged at ticks 2,520, 5,911 and 8,838. The travel segment
contains solar escapes at 112.43–115.43 and 118.28–121.00 seconds. At 112 seconds
its selected planet-1 detour points across the sun's vicinity; this is a useful
next routing investigation. Inspect the ordering of intervening obstacles and
the velocity frame used to follow each waypoint, starting at tick 5,912. The
current waypoint-order/frame protections are partly limited to pursuit, and
that distinction should be tested before extending them to transfer.

The changed ordinary seed-42 duel captures ground and starts a real opportunity
pursuit, but records no shots or hits within three minutes. P2's pursuit begins
at tick 2,504 and expires after its existing 1,800-tick budget; it repeatedly
switches between clearing ground and climbing for a firing pass. This is a new
failure of `generated_match_captures_and_engages` relative to the rounded
checkpoint. Preserve it as a combat/approach regression, not a successful duel
or a reason to restore the self-obstruction bug. No combat tuning is included
in this change.

Start that investigation in `MaterialMissionPilot::hunt`: during the pursuit,
the nearest-planet observation repeatedly switches between planets 1 and 2.
The committed firing-pass climb is cleared when its planet differs from that
observation. Compare this with the existing departure logic that clears both
nearby planets, and measure actual radial clearances and relative velocities.
The landing ray's reported altitude of 26 outside its range is a sentinel,
not a measurement of the true flight height. Distinguish time spent climbing,
time spent aiming, and actual firing eligibility before changing combat policy.

The complete release workspace suite, including all targets and examples,
finishes with **1,280 passed, two failed and 41 normally ignored**. Only the
mission integration target fails: 18/20 pass, including all three original
capture-preparation/pursuit cases. Its failures are the seed-3 route and seed-42
duel described above. All 356 scenario-library tests pass, including the new
forecast regression; the recovery, terrain, ground-navigation and other AI
contracts also pass. Formatting and diff checks pass. Selected-library/example
Clippy completes with the same seven existing scenario warnings.

```sh
cargo test --locked --release --workspace --all-targets --no-fail-fast
```

This command correctly exits nonzero for those two preserved acceptance failures.
The branch remains experimental; the performance cost from the rounded-match
comparison also remains unresolved. No Pi deployment is part of this follow-up.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --surface round --seed 42 --seat 0 --mirror true \
  --mode pursuit --prepare-seconds 180 --seconds 90 --trace true \
  --trace-start-tick 1440 --trace-end-tick 2400 --out /tmp/landing-forecast-42
target/release/examples/surface_mission_soak \
  --world generated --surface round --seed 3 --seat 1 --mirror false \
  --mode quiet --seconds 180 --require-route true --trace true \
  --out /tmp/landing-forecast-3
target/release/examples/surface_mission_soak \
  --world generated --surface round --seed 42 --seat 0 \
  --mode duel --match true --seconds 180 --trace true \
  --out /tmp/landing-forecast-duel
```

The seed-3 command exits nonzero after saving its failed route report. The duel
runner audits physics but does not assert weapon contact; inspect `final_combat`
or run `generated_match_captures_and_engages` for that acceptance requirement.
Use new output directories to preserve earlier evidence.

Local reports, executables, source patches, diagnostic logs and test results:

```text
/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/landing-followup/
```
