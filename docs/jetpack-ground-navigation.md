# Jetpack routes in ordinary material gameplay

The material combat and duel presets now equip both pilots with the shared
spaceling jetpack. `GroundNavigationTask` (`ground_navigation_v3`) can combine
walking/jumping with one measured crossing of its assigned parked full ship.
Capture and recovery already use this task, so the same route option serves
flags, returning to a hatch, and reachable rebuild destinations.

Tap A/Space to jump or get up, hold in the air for lift, and use left/right to
steer. Stand still on support with jump released to recharge. Boarding preserves
charge. The nozzles, flames and charge HUD use the same equipment for humans and
bots. The dedicated `spacewars-terrain-jetpack` trial remains available.

## Route choice and control

The existing retained-material ground graph still supplies walk/jump routes.
At its staggered 2 Hz survey, an equipped pilot also measures one bidirectional
corridor over the real ship hull. Both endpoints require retained planet footing,
suitable slope and capsule clearance. The corridor retains solid vehicles,
debris and other spacelings; only the observing actor is excluded.

The navigator compares the direct ground route with two alternatives: walk to
one takeoff endpoint, cross, then follow measured ground to the original target.
The complete alternative must be reachable. Flight has a cost for climb, descent
and recharging, so a short walk wins. Planning waits for a completed joint survey
instead of selecting a long walk while the flight survey is between refreshes.

The selected maneuver reuses the trial controller in a single-crossing mode.
It waits for at least 98% charge, aligns at takeoff, climbs, crosses and descends
through ordinary controls. Within 1.5 units of the measured floor and aligned
with the destination, it releases lift to establish contact instead of hovering
away the landing reserve. Real supported, balanced contact completes the leg.
The navigator then plans toward the original destination. Claiming and boarding
retain their normal scenario rules; the maneuver cannot write either state.

## Destruction and interruption

Each refresh rechecks the corridor and ship pose. A material revision temporarily
holds powered travel until a fresh survey arrives. A still-clear corridor with
essentially unchanged endpoints can be revalidated; stale geometry cannot grant
permission to fly. Larger geometry changes, a missing corridor or exhausted fuel
interrupt the leg. Gravity and collision contacts resolve the motion while the
navigator waits for retained planet support before replanning. It never teleports
or refills a pilot to rescue a failed route.

A changed or destroyed flag updates the objective while preserving an active
landing. Capture and recovery callers wait for that landing before transferring
control to claiming, rebuilding or boarding. The original ninety-second ground
budget remains fixed; three interrupted flight attempts also stop explicitly.
A failed landing that cannot regain planet support remains a bounded failure.

## Scope and validation

This handles the assigned parked full ship on a controlled planet. It does not
supply cave flight, arbitrary crater escape, escape-pod crossings, jetpack combat
or generated multi-planet strategy. Blocked hatches still prevent exit. Some
landing positions have no measured route even after exit: the retained desktop
exploration at seed 42, P1 offsets -0.4/-0.5/-0.6 reproduces that failure with
jetpacks enabled and disabled.

Focused checks exercise route choice, actual charge waiting, repeated ticks,
clone/reset, changed objectives, interrupted corridors, fixed deadlines, and
physical crossings in both directions from both seats. The physical round trip
also revises terrain during flight and verifies normal claims and hatch entry.
Read-only sensor tests cover both directions, blocked overhead space, dirty
queries and the on-foot survey gate.

The existing three-minute ground runner accepts `--jetpacks true`; its matrix
accepts `--jetpacks`. An opposite-side capture start forces the regular mission
to cross the ship on the way to the enemy flag and again on its return:

```sh
cargo build --locked --release -p spacewars-ai --example surface_flag_soak
cargo run --locked --release -p spacewars-ai --example surface_flag_soak -- \
  --seed 42 --seat 0 --offset -0.2 --mode capture --jetpacks true \
  --edit flight-flag --out /tmp/jetpack-capture
```

`--edit none`, `flight-flag`, and `flight-crater` select no interruption, removing
the enemy flag during flight, or revising terrain away from the active corridor.
All advance 180 simulated seconds, including after completion. Reports retain
failed outcomes, route/flight telemetry, actual edits, claims, boarding/departure,
physics/material audits and separate sensor/step timings.

The local evidence archive is
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ground-jetpack-20260909/`.
