# Surface Expedition

For the combined destructible-ground acceptance scene, choose
**spacewars-terrain** and see [Spacewars Terrain](spacewars-terrain.md).
It shares Expedition's pilots, claims and recovery, with aimed mining and
ownership loss when a flag's supporting material is destroyed or detached.

An opt-in, one- or two-player planet-to-planet loop on the experimental Surface
V1 world. Choose **surface-expedition** in the launcher, or run:

```sh
cargo run -p engine-client -- --scenario surface-expedition --seed 0
```

In the launcher's **Settings → Players**, choose **1** (the default) or **2**.
The choice persists and applies to direct launches too:

```toml
[surface_expedition]
players = "two" # "one" is the default, including for older settings files
```

This evolves the existing Expedition entry, not a new scenario. It shares the
Surface Sortie controllers, Rapier world, gravity solve and natural landing
rules, with travel enabled. **Planet claiming does not require an outpost.**
The older terminal/capture/repair model remains in the pinned Sortie and
compatibility fixtures for future infrastructure work. Ordinary Spacewars,
its docking rules, services, bots and protocols are unchanged.

See [Surface Sortie](surface-sortie.md) for landing controls and
[Surface V1](surface-compatibility.md#surface-v1-experiment) for the experimental
world parameters and known physical limits.

## Playtest the loop

1. Land rear-first and settle on both rear feet. B / X disembarks; release
   controls afterward. The ship must be physically landed to enter or exit.
2. On a neutral planet, stand still on the actual surface for **3 seconds**.
   A flag rises at your contact point, and the planet becomes yours when it
   reaches the top. There is no terminal to find and no extra capture button.
3. On an enemy-owned planet, walk **within 3 world units of the existing flag**.
   Stand still for **3 seconds to lower it**, then **another 3 seconds to raise
   your replacement**. Landing elsewhere cannot remotely remove a flag.
4. Return to your own cyan hatch, board with B / X and take off with A / Space.
5. Fly to another planet using the minimap, brake with Down / S, face away from
   the surface, land and repeat. Previously claimed planets stay yours.

Left/right turns aboard and walks on foot; A / Space thrusts or jumps.
Start / Esc pauses. R or the pause menu restarts the same seed with fresh
ownership. Ships start at full health. This loop has **no repair service**.

### Flag and contest rules

Claims require real contact with that planet's terrain, a balanced spaceling,
and support-point relative speed at most 1 world unit/s. Standing on a nearby
ship or unrelated platform does not count. Flag proximity is measured from
the actual surface contact point to the flag base, not from the ship.

While an enemy flag is lowering, its owner still owns the planet. Once fully
lowered, the planet becomes neutral and a new flag starts at height zero at
the claimant's contact point. No time spent lowering counts toward raising.
The replacement must finish rising before ownership changes to the attacker.

Leaving eligibility, jumping, walking too quickly, boarding or being knocked
down resets the **unfinished stage**. Interrupted lowering restores the
defender's still-owned flag; interrupted raising removes the unfinished flag.
A completed lowering stays completed: the planet remains neutral if the
attacker subsequently leaves. Partial progress never transfers to another
claimant.

Opposing eligible spacelings near an existing flag pause progress. A distant
defender, or an airborne/knocked-down defender, cannot block it. A contest
preserves partial time only while its claimant still qualifies. Before a
neutral planet has any flag, simultaneous eligible claimants contest rather
than winning by seat iteration order; one must yield before raising begins.

The HUD shows ownership, lowering/raising progress and why progress is blocked.
Minimap planet colors show completed ownership; triangles locate flags, even
while raising or lowering. A rising flag does not prematurely recolor the
planet. Flags move and rotate with their planet at their original contact
location.

### Two-player expedition

Each pilot has its own assigned ship, input state, landing gate, transfer
history and motion diagnostics. Players start near separate planets when the
layout permits; they can travel to the same planet and physically meet there.
Each split-screen pane follows its active actor with its own HUD and minimap.
World physics and ownership are shared.

Assigned gamepads use Left/Right, A, B and Down. Keyboard P1 uses A/D, Space,
X and S; P2 uses numpad 4/6 to turn/walk, 8 to thrust/jump, 2 to board/exit and
5 to brake. Existing gamepad assignment and reconnect rules still apply.

Only your own ship can be boarded. One pilot's neutral-after-transfer gate
does not interrupt or arm the other. A hatch exit occupied by another spaceling
is blocked, including transfers in the same physics tick. Spacelings and both
ships collide in the same Rapier world.

### Ship loss and recovery

Expedition now keeps the pilot and vehicle lifecycles separate:

- Destroying an **occupied ship** leaves that same pilot aboard a flyable escape
  pod. Pods use the same bounded flight assist and a scaled pair of rear feet;
  they must actually land and settle before B / X can disembark. Planet ownership
  is not required to fly, land or exit a pod.
- Destroying an **empty ship** leaves its existing on-foot spaceling alive.
  The dead ship disappears; it does not create an empty pod or a second creature.
- Without a full ship, stand balanced and still on an **owned planet for 8
  seconds** to rebuild. You can first claim a neutral planet using the normal
  flag rules. Rebuilding requires no outpost or flag proximity. Jumping, losing
  support/balance, moving too quickly or losing ownership resets unfinished
  construction. Standing on a ship or unrelated platform does not qualify.
- A replacement appears nearby, at full health, with local surface velocity and
  spin. It must settle naturally; walk to its cyan hatch and board normally.
  Its pilot remains on foot, and an old pod is removed when the replacement is
  created. There is never more than one live assigned vehicle per seat.

The HUD reports pod landing, eligibility, rebuild progress and blocked space.
Placement tries at most four local terrain rays and conservative hull/foot
clearance checks. It prefers putting the hatch toward the waiting spaceling.
If all sites are blocked, progress stays at 100% and placement retries every
half-second while eligibility remains valid. Move along the surface to select
a different build site; moving fast enough resets the timer.

For a deliberate **loss drill**, hold **A + B + Down together for 3 seconds**
(P1 keyboard: Space + X + S; P2: numpad 8 + 2 + 5). The chord destroys your
assigned full ship, even when you are on foot; it never destroys a pod.
Release early to cancel. While charging, the chord consumes movement/transfer
input. Release all controls after loss or replacement before continuing.
Try it both aboard and after disembarking. The drill is an Expedition testing
affordance, not a weapon or a new ordinary-game control.

Breakup pieces remain physical and visible. For their first half-second they
cannot hit the surviving owner vehicle or spacelings, allowing initially
overlapping pieces to disperse. Terrain, debris and opponent-vehicle collisions
remain active, then normal fragment collisions resume. This exception applies
only to owner-tagged breakup fragments in the recovery-enabled fixture.

This is deliberately not a complete survival system: **pods and spacelings are
still invulnerable**, and there is no pilot death/elimination, combat, ship
swapping, remote rescue or recovery of a living full ship stranded elsewhere.
The recovery build has been deployed and smoke-checked on the Pi as described
below. The user subsequently reported that the basic recovery loop worked
full circle on the device.

### Initial Pi playtest (2026-09-07)

The earlier single-player and then two-player **outpost-based** Expedition
builds were deployed to `spacewars.local` through the normal A/B updater.
The two-player build was observed near 60 FPS / 60 UPS with no service
restarts; launcher selection, pause/restart and screenshots were checked.
The user reported successful playtesting, then requested this simpler flag
loop. That first feedback accepted the preceding travel/multiplayer work;
the flag loop was then tested separately as described below.

The flag-loop build was subsequently deployed on the same day to slot A
(`/dev/sda2`), preserving the preceding build in slot B. The installed client
checksum matched the new package; settings were unchanged. Two-player
Expedition was launched and sampled at 60.1 FPS / 60.1 UPS with zero service
restarts. Both controllers were detected, and an actual 800×480 screenshot
confirmed the new claim HUD. The user subsequently reported that the flag-loop
playtest seemed good; this is initial acceptance, not exhaustive route coverage.

The recovery expansion was subsequently built from the working tree after
`9c8d841` and deployed through the same updater to slot B (`/dev/sda3`), retaining
the accepted flag build in slot A. The installed client checksum matched the
fresh package (`ced27785fc6b72ab7022400d552accfbad464c70a3e2e2d89d770259858e91c8`).
Settings were byte-for-byte unchanged and persistent data remained mounted.
Two-player Expedition was launched with seed 0 and raster scale 2, sampled at
60.1 FPS / 60.1 UPS, and an 800×480 screenshot confirmed both landed ships,
HUDs and minimaps. Both gamepads were detected; the service stayed active with
zero restarts, including after the screenshot. This is deployment verification,
separate from the subsequent manual playtest.

The user then reported successful scuttling both in flight and while landed
aboard a full ship, followed by pod landing, disembarking, claiming a planet
and returning to a full ship. The existing rebuild percentage and "stand still"
guidance were visible; no additional UI polish was requested. This accepts the
basic occupied-ship recovery loop, not exhaustive route or multiplayer failure
coverage. Scuttling an empty ship while its pilot is already on foot remains
automatically tested but was not explicitly included in that manual report.

## Approach is not support

Approach selection uses distance to planet **surface**, with real rear-foot
contact taking priority. Two-world-unit hysteresis prevents chatter in free
space; it never extends contact or grants a landed state. Selection reads
completed Rapier terrain poses, not the next prescribed target.

Landing still needs two qualifying rear-foot contacts, the existing angle,
speed and spin limits, and 0.25 seconds of settling on the same planet.
Changing planet resets settling time. Proximity or a nose/hull collision
cannot allow a transfer.

Boarding needs a balanced, slow, nearby spaceling supported by the landed
ship's planet. On-foot diagnostics follow the spaceling's support; in flight
they use its nearest surface independently of the parked ship. No actor is
attached, transported, or given a new force by claiming.

## Ownership, attachment and cost

The canonical owner is the existing planet ownership field. Each planet has
a small claim record: optional flag attachment, current stage/claimant/time,
seat-relative eligibility and capture/neutralization counters. Infrastructure
is not a prerequisite for ownership and can be added independently later.

A flag stores the actual support point and normal in the completed planet
body's local frame. Rendering transforms those back to world space; it does
not reproject onto a nominal radius. A flag adds **no body or collider**.
This attachment contract leaves room for noncircular terrain; handling terrain
removal or migrating support between fragments remains future work.

Each completed tick samples each pilot's real support once, then updates the
small planet list. Scratch space is bounded to two seats. Approach selection
is allocation-free; the landing feet query their local Rapier contacts. There
are no all-object scans, extra physics steps or extra gravity solves. An
outside pilot adds one capsule; boarding removes only that capsule. Healthy
ships remain present while unoccupied. Loss removes or replaces the vehicle
assembly, and rebuilding reuses the assigned slot with a fresh full assembly.
Recovery adds bounded per-seat checks, not another physics step or gravity
solve. Expedition no longer spawns physical outpost terminals.

## Observations and verification

Scenario observations are version **10**, with a top-level `version` and
`players` array in seat order. Each view retains stable pilot/owner/vehicle IDs,
input/transfer gates, landing and motion metrics, `travel_enabled`,
`ship_support_planet` and `pilot_support_planet`.

`planet_claim` is the active actor's focused planet view; `planet_claims` is
the complete inventory. Each reports owner, claimant, phase, progress, required
seconds/range, eligibility status, flag world position/normal/raised fraction
and capture/neutralization counters. `outpost` is now optional: null in
Expedition, populated in pinned fixtures. Expedition's `outposts` list is
empty; those older fixtures retain their inventory and repair telemetry.

`landing.planet` is the ship's approach/contact frame; `motion.planet` is the
active actor's diagnostic frame. `generated_case` identifies the starting
fixture, not the current destination. Planet indices are zero-based.
Surface action V2 addresses a player explicitly; malformed/inactive seats
are ignored and continuous input persists separately per seat. These
observations do not add a live IPC telemetry endpoint.

Version 10 adds `vehicle_form` and optional `recovery` per player (null in
the pinned non-recovery fixtures). `ship_available` still means an intact full
ship, not a pod; a live pod remains accessible through the usual landed transfer
gate. Recovery reports status, build planet, required/progress times, ships
lost, pod ejections, completed rebuilds, interrupted rebuilds and blocked
placement attempts. Stable vehicle IDs identify the assigned slot, not a
particular hull generation; the counters distinguish losses and replacements.

```sh
cargo test --locked --release -p scenario-spacewars surface_sortie::claim::tests
cargo test --locked --release -p scenario-spacewars travel_tests -- --nocapture
cargo test --locked --release -p scenario-spacewars multiplayer_tests
cargo test --locked --release -p scenario-spacewars recovery_tests

# Retained comparison: unchanged criteria and pinned outpost fixtures
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --profile both --seeds 4

# Real client workflows on an existing X display
cargo test --locked -p engine-client --test ui_control_functional surface_expedition -- \
  --ignored --test-threads=1
```

Flag regressions cover neutral raising, nearby-only enemy lowering, a full
fresh replacement timer, interruptions, local contests, order independence,
wrong-planet and unrelated-platform support, walking/jumping/knockdown,
exact attachment through translation/rotation, and real physical land/exit/
claim/reboard cycles at twelve motion/bearing combinations. A second pilot
landing on the far side cannot lower the flag until physically supported
near it.

A controlled two-planet journey uses only player actions after fixture setup:
claim A, board, launch, coast, brake/turn, land on B, claim, reboard and depart.
It runs with stationary and translating centers and spinning surfaces,
preserving both flags, identities and body counts, with four transfers and no
ship damage. The original outpost/capture/repair journey remains a separate
reference test. The scripted test pilot is not a gameplay autopilot.

Client checks cover one/two-player input, disconnect, cameras, minimap
footprints, ownership colors and actual flag raster pixels at desktop,
portrait and 800×480 Pi-sized resolutions. Real UI workflows exercise both
renderer choices, launcher, settings persistence, pause, restart and relaunch.
See [Functional UI tests](functional-tests.md) for software-backend vector
limitations. Hardware/controller feel remains a manual check.

Validation of the multiplayer + flag loop (2026-09-07, Rust 1.89): **844**
workspace/all-target tests and all **14** real UI workflows pass. Frozen
ordinary-game `navigation-v1` (6 episodes) and `strategy-v1` (12 episodes)
baselines match. Windows client and ARM64 scenario compile checks pass.
Clippy completes with pre-existing warnings outside the surface changes.
The separate Pi deployment smoke check is recorded above; it does not replace
manual controller and flag-loop playtesting.

Recovery regressions add an input-only pod → landing → disembark → neutral
claim → rebuild → board → takeoff journey, pod landings at twelve controlled
motion/bearing combinations, empty-ship loss, preserved identity/velocity/spin,
independent two-seat loss gates and repeated/replayed rebuilding. Policy checks
cover lost ownership/support/balance, relative speed, blocked placement/retries,
surface-frame spawn velocity, neutral input gates and read-only rendering.
Client tests use the minimal gamepad mapping and check pod/rebuild/replacement
HUDs and both renderers at desktop, Pi-sized and portrait resolutions.

Validation including recovery (2026-09-07, Rust 1.89): **861** workspace/all-target
tests and all **14** real UI workflows pass. Both frozen ordinary-game suites
still match; Windows client and ARM64 scenario compile checks pass. Clippy
reports only the existing warnings outside the changed surface paths. Device
playtesting subsequently accepted the basic occupied-ship recovery loop above.
All **16** recovery tests also pass in the optimized release build.

## Remaining integration

Pilot survival/remote rescue and versioned bot surface intents remain separate
work. There is no combat, ship swapping, economy or ordinary-game docking
change here. An unreachable living ship or stranded airborne spaceling can
still require a restart. Surface V1's
passive-approach failures, mutual-field mismatch, arbitrary generated routes
and long-idle support drift remain known limits. Ownership is now deliberately
simpler than future outposts, resources and services.
