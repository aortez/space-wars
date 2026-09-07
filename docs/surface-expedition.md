# Surface Expedition

An opt-in, single-pilot planet-to-planet loop on the experimental Surface V1
world. Choose **surface-expedition** in the launcher, or run:

```sh
cargo run -p engine-client -- --scenario surface-expedition --seed 0
```

The original Surface Sortie presets and the raw/profile compatibility runner
remain pinned to their selected planet. Expedition uses the same scenario,
controllers, Rapier world, gravity solve, and natural landing rules, with travel
enabled and one intact outpost per generated planet. Ordinary Spacewars is
unchanged. See [Surface Sortie](surface-sortie.md) for the full controls and
[Surface V1](surface-compatibility.md#surface-v1-experiment) for world parameters
and known physical limits.

## Playtest the loop

1. Settle on the starting planet. B / X disembarks; release controls afterward.
2. Walk to the amber terminal and stand still for three seconds to capture it.
   Its flag and minimap square change to your color. A friendly site repairs
   your nearby landed ship, without claiming the whole planet.
3. Return to the cyan hatch, board with B / X, and take off with A / Space.
4. Fly to a different planet using the minimap. The HUD's approach-planet index
   updates automatically; no target-selection button is needed.
5. Brake with Down / S and point the nose away from the destination planet.
   Settle rear-first, then exit and capture its separate terminal. Find a clear
   walking route from the hatch: the parked hull and terminal are solid.
6. Reboard and depart. The first site's ownership remains recorded; its repair
   service does not follow you to another planet.

Left/right turns aboard and walks on foot; A / Space thrusts aboard and jumps
on foot. Start / Esc pauses. R or the pause menu restarts with the same seed and
fresh ownership. Controller mappings and neutral-after-transfer gating are
shared with Surface Sortie.

### Initial Pi playtest (2026-09-07)

The current Expedition build was deployed to `spacewars.local` through the
normal A/B updater. The kiosk ran the scenario at a sampled 60 FPS / 60 updates
per second, with no service restarts; launcher selection, pause and round
restart were verified through the public control API. The user then reported
that playtesting worked. This is initial device acceptance, not an exhaustive
controller, generated-route, or long-duration stability check.

## Approach is not support

The approach frame is chosen by distance to **planet surface**, not distance to
its center. Real rear-foot contact takes priority. A two-world-unit hysteresis
avoids selection chatter near a free-space boundary; it never extends contact
or grants a landed state. Selection uses completed Rapier terrain poses before
control and again after the physics step, not the next prescribed target pose.

Landing still requires two qualifying rear-foot contacts, the existing angle,
speed and spin limits, and 0.25 seconds of settling **on the same planet**.
Changing planet resets accumulated settling time. Proximity or a nose/hull
collision alone cannot allow a transfer.

The hatch is projected beside the ship onto its current approach/landing
planet, but is usable only after the physical landing gate passes. Boarding
also requires the spaceling to be balanced, slow, nearby, and supported by
that same planet's terrain or terminal. An unrelated platform near the hatch
does not qualify. On-foot diagnostics and site focus follow the creature's
support; in flight they use its nearest surface, independently of the parked
ship's approach frame. No actor is attached, transported, or given a new force.

## Independent sites and cost

Each site has a stable ID, planet index, local surface location, capture state,
owner, and repair accounting. Every site updates independently. Capture uses
the spaceling's real contact identity; repair requires the ship to be landed
on **that site's planet**, as well as friendly ownership and service range.
Being near a foreign planet's friendly site is insufficient.

This experiment creates exactly one site per planet. That is fixture content,
not a requirement that every future claim needs a neutral outpost. Multiple
sites on one planet, contested multiplayer service arbitration, destruction,
and economy remain separate work.

A site adds one collider to its existing kinematic planet and no new body.
Approach selection is allocation-free and linear in the small planet list;
the two landing feet use their local Rapier contacts. Spaceling support identity
is decoded directly from its contact ID. There are no all-object scans, extra
physics steps, or extra gravity solves. Aboard/on-foot body counts still differ
by exactly one capsule.

## Observations and verification

Scenario observations are version **7**. They add `travel_enabled`,
`ship_support_planet`, `pilot_support_planet`, and the complete `outposts` list.
`landing.planet` identifies the approach/contact frame, while
`motion.planet` identifies the active actor's diagnostic frame. `outpost` remains
the currently focused site's convenient HUD view; it is not the full world
inventory. `generated_case` identifies the **starting** fixture, not the current
destination. Planet indices are zero-based; site IDs start at one.

```sh
cargo test --locked --release -p scenario-spacewars travel_tests -- --nocapture

# Existing comparison, unchanged physical criteria and pinned fixtures:
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --profile both --seeds 4

# Real client lifecycle and screenshot checks on an existing X display:
cargo test --locked -p engine-client --test ui_control_functional surface_expedition -- \
  --ignored --test-threads=1 --nocapture
```

The full journey test uses two controlled, spinning planets, repeated with
translating centers. After initial fixture construction it uses **only player
actions** to capture A, board, launch, coast, brake/turn, land on B, disembark,
capture/repair, reboard and depart. It verifies both sites remain owned, no
planet is implicitly claimed, both repairs occur, identities/body counts stay
stable, four transfers occur, and ship damage stays zero. The sampled routes
complete in about 36 simulated seconds; wall-clock timing is not a test gate.
The scripted test pilot is not installed as a gameplay autopilot or bot brain.

Separate generated-arrival tests start near nonzero-index planets with a stale
departure frame, then establish real support, exit and reboard there. Other
checks cover nearest-surface selection and hysteresis, settling-time reset,
foreign-site repair rejection, unrelated-platform boarding rejection,
deterministic replay, restart, site collider counts, and independent minimap
ownership colors. The real UI workflow covers launcher, both renderer choices,
pause, restart, return and relaunch; software-backend vector limitations are
documented in [Functional UI tests](functional-tests.md).

The controlled routes are not a guarantee that every generated planet pair is
easy to traverse. Surface V1's passive-approach failures, mutual-field mismatch
and long-idle drift remain tracked limitations. Landing too far from a site's
24-unit service range requires walking and/or relocating the ship.

## Remaining integration

Next come multiple independent pilots/vehicles, explicit loss/rescue/rebuild
rules, contested services, and versioned bot surface intents. Damage or loss
of this experiment's only vehicle can still require a restart; there is no
combat, rescue, ship swapping, economy, or new ordinary-game docking policy.
