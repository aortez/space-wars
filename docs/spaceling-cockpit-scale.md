# Spaceling scale and the ship's escape cockpit

This is the first integration of
[#95](https://github.com/aortez/space-wars/issues/95), taken before adding
prospective jetpack crossings to the landing planner. The escape pod is the
visible cockpit of the full ship. Destruction leaves that cabin at its existing
world position and orientation; rebuilding uses the same cabin drawing.

## Dimensions and authority

`scenario_spacewars::spaceling_geometry` supplies the Spacewars suit scale,
capsule radius, half-segment and standing half-height. The capsule is now
0.9 units tall and 0.3 wide, down from 1.8 and 0.6. Physics, render geometry,
ground/clearance queries, bot foot positions and candidate flag positions use
these dimensions. Standalone engine labs retain their larger default capsule.

The cockpit window is one unit in diameter. Its housing now encloses the
window, within the pod's existing triangular collision hull. The attached pod
also fits inside the full ship's fuselage. There is still one physical vehicle
body, and an external pilot body exists only while on foot. No nested collider
or second gravity solve is introduced. Hull and pod masses, landing feet and
flight controls retain their previous values.

Walk, jump and jetpack speeds, fuel, mining reach and claim/boarding rules are
unchanged. At constant density, the resized pilot's mass is one quarter of its
previous mass. Its angular knockdown threshold scales inversely with size:
16 rad/s instead of 8, preserving the former speed at the capsule extremity.
Linear knockdown and damage thresholds stay unchanged. This avoids new false
tumbles while walking uneven contour ground at the same travel speed.

The flight-corridor endpoint height now derives from the smaller capsule,
including the existing 0.20-unit flight margin and allowed support slope.
Planning quotas, graph sampling counts and policy selection are unchanged.
Both retained v9 and candidate v10 use the new world geometry. Pre-resize
physical traces remain evidence for that older world, not expected byte-for-byte
trajectories for the new one. A future runtime actor-size option would also
need to make dimensions part of retained measurement identity.

## What testing exposed

An initial cabin extending below the former pod hull increased the speed
imparted by the supplied missile fixture enough to make ordinary braking fail
before hitting the world boundary. Fitting the cabin inside the existing hull
retains the established physical survival opportunity without changing missile
mass, damage or brake strength.

The low-roof fixture now scales its ceiling and prone spawn to the actual
capsule. It exercises a failed get-up attempt, measured crawling and a fresh
get-up press once clear, as the existing ground controller does. A blocked
press remains unbuffered. It checks that the pilot stands without an ordinary
jump; continuous crawling farther reaches a separate block-terrain step.

The complete lost-ship recovery regression exposed two existing jetpack
assumptions. Getting up consumes its button press and disarms the pack; a
grounded lift now releases a held press before trying to launch again. The
descent target also derives from actual standing height. A rocking pod can
alternately obstruct adjacent walk samples with the smaller capsule. A freshly
surveyed gap may replace the in-progress corridor when each gap boundary shifts
at most one sample, endpoints remain within the existing one-unit landing
window and cruise height differs by at most 0.25 units. Planet and direction
still have to match, and the refreshed plan must use the live terrain revision.
Missing fresh evidence, a different gap, excessive vehicle motion or larger
changes still interrupt the flight. This uses the existing
surveys; it does not add the deferred prospective jetpack planner.

The jetpack artwork and support indicator scale with the suit. Camera framing
is retained; pixel checks account for the suit's quarter-size projected area
and still require visible colored fill at the actual actor position.

## Reproduction

```sh
cargo +1.89.0 test --locked --release \
  -p scenario-spacewars -p spacewars-ai --lib --tests
SPACEWARS_SORTIE_ARTIFACTS=/tmp/cockpit-render \
  cargo +1.89.0 test --locked --release -p engine-client \
  client_scenarios::surface_sortie::tests
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_flag_soak --example surface_mission_soak \
  --example surface_recovery_soak --example jetpack_crossing_soak
```

The focused regressions cover window/capsule containment, an actual pilot
collider fitting a one-unit passage that rejects the former capsule, visible
cockpit continuity through rotated destruction/rebuild, and shared physical
hull mass. Existing suites exercise landing, transfer, mining, support loss,
claims, knockdown, get-up, recovery, damage, both bot policies and bounded live
survey jobs. Render fixtures capture both players aboard and on foot at
1280×720, 800×480 and 800×1280, plus pod/rebuild stages.

## Retained results and follow-up

Final checks: 597 scenario/AI release tests and 326 client tests pass (one
unrelated opt-in client test remains ignored). Formatting and diff checks pass.
Clippy completes with the repository's existing warnings; none are on the
added/changed lines. Both deterministic baseline suites pass, with the one
intentional strategy fingerprint update explained below.

The release trial matrix uses seed 42 and runs each case for 180 seconds.
Raw reports, logs, screenshots, reference binaries and the exact runner are
retained locally under `target/issue-95/`; `run-soaks.py` records every command.
Both ordinary v9/v10 and opt-in live v3 are tested against the resized world.

| Trial | Seats/cases | Result |
| --- | --- | --- |
| Parked ship: cross both ways, claim, board | Both seats | 2/2 complete |
| Airborne ship loss: pod landing, reclaim, rebuild, depart | Both seats | 2/2 complete |
| Contested flag: v9, v10, live v3; approach offset −0.8 | Both seats for each policy | 6/6 complete |
| Generated duel, v9/v10 seats swapped, mixed asteroid every 15 s | 2 current + 2 reference | All physical audits pass |
| Passive diagonal arrival, bearing 0.7853982 | Both seats, current and reference | Same existing landing stall |

All current physical audits pass. The diagonal starts never reach an on-foot
crossing: the full ship rests on one foot, so the trial times out after 90 s.
Both telemetry and all 180 per-second sample records match the pre-resize
binary exactly in both seats. This is a landing-assist/controller follow-up,
not evidence that a jetpack crossing was attempted successfully.

The generated duels cover different trajectories after resizing. Their measured
seat departs from one distinct planet in the new world versus two in the
reference; this tiny sample is not a policy-strength comparison. Seed 3's quiet
P2 route identifies a concrete timing change: the first two departures remain
close to their reference times, but the third landing needs a second retry.
It departs at tick 11745 (195.75 s), versus 10564 (176.07 s) before resizing.
The 180-second regression now requires two completed sorties for that seed;
the other four generated starts still require three. A separate 240-second
diagnostic confirms the third completion. Reproduce and inspect the landing
events around ticks 10039–11320 with:

```sh
target/release/examples/surface_mission_soak \
  --world generated --seed 3 --seat 1 --mode quiet --seconds 240 \
  --frames true --out /tmp/small-pilot-seed3
```

The classic navigation baseline matches all six episodes unchanged. Of the
twelve strategy episodes, only episode 7's fingerprint changes. The retained
reference binary matches the old manifest; comparing its full summaries with
the new binary finds only that hash different (apart from wall-clock timings).
The same player wins on tick 12359 with the same metrics. The changed terminal
state keeps the surviving pod at the destroyed ship's cockpit instead of
shifting it by the legacy pivot offset. Only that manifest hash is updated.
