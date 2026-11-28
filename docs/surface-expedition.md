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

Any live opposing surface force on the planet prevents starting or advancing
lowering/raising, including the final ownership step. This includes distant or
knocked-down pilots, occupied or empty landed ships, and live tanks of another
side, upright or overturned, moving or still. Friendly units, a player's own
tanks among them, unowned units, wrecks and nearby flying ships do not block.
Boarding represents the pilot by the vehicle and does not change the vehicle's
allegiance. A tank's allegiance is set when it is placed, to a government
hostile to both players or to one player, and nothing changes it afterward. A
government is no player seat, and neither a capture nor a lost flag converts,
repairs or replaces a tank.

Opposition preserves partial time only while the same claimant still qualifies.
Losing footing, balance or flag range cancels normally; clearing the opposition
then resumes only a still-valid attempt. Simultaneous claimants do not win by
seat iteration order. An arriving defender does not revoke completed ownership.
Destroying the flag footing neutralizes ownership without retiring its defenders,
so the replacement flag remains gated.

Membership requires actual native terrain support: a contact that holds the
unit up, pushing it away from the planet's center. A tank has it through any
wheel or chassis contact of that kind, so a tank on its back still counts, but
one touching only a steep wall does not. Pilots, tanks and other registered
ground units retain their last association across a brief hop/contact gap for
less than one second and at most eight world units from their last supported
body position, measured in the planet's moving frame. Only new support
refreshes that interval. Foreign support, such as a detached fragment or loose
grains beneath a unit, ends continuity at once, as do clear departure, death,
retirement and allegiance reassignment. A touch from the side or from above is
not support, so a ship or tank brushing or riding a hopping tank leaves its
grace alone. Ships require current landing-foot contact; applying thrust clears
membership immediately. Approach frames and gravity selection never establish
surface membership.

The HUD shows ownership, lowering/raising progress and why progress is blocked.
Minimap planet colors show completed ownership; triangles locate flags, even
while raising or lowering. A rising flag does not prematurely recolor the
planet. Flags move and rotate with their planet at their original contact
location. Tanks are drawn as in Spacewars, in their player's color or a
government's olive, with a bar above each that its health fills. Rounds, the
laser and hard crashes wear a tank's armor, as
[Spacewars bodies](design/material-sources.md#spacewars-bodies) describes, and
a tank whose armor fails breaks up.

### Tank fire

A tank's command aims its cannon and holds its trigger. `TankCommand` gives the
barrel's `elevation`, 5° to 85° above the horizontal; its `facing`, 1 for the
side clockwise of the tank's up and -1 for the other; the `power` a round
leaves at, 12 to 48 relative to the hull; and whether `fire` is held. Each
value is held to its range, and a command with a value that isn't a finite
number is refused. Up is against the gravity the tank's chassis feels, or the
hull's up where it feels none, so a tilted tank aims as a level one does.

Tanks fire the shared cannon's rounds by its rules, in
`crates/engine-rapier/src/cannon.rs`, which
[Scorched Earth](scorched-earth.md#armor-and-shells) fires by and tests. Each
step, the tanks holding their triggers fire in id order, each once its cannon
has reloaded, 75 ticks after it last fired, and while fewer than 4 of its
rounds and 8 tanks' rounds in all are in flight. Ships' shells don't count. A
round leaves the muzzle, or the point where something other than its own tank
stands in the barrel, at its power along the barrel plus the velocity the hull
gives the barrel's pivot. A round that starts in a blocked barrel bursts in its
first step on what blocks it, and its blast costs the tank that fired it about
19 health.

- A round is a shell of the cannon's size and mass, in its casing, carrying its
  [payload](design/material-sources.md#payloads). It meets every solid from
  launch, except that it passes through the tank that fired it on its first 8
  steps, which carry it clear of the hull. Every other tank meets it at once.
- It falls by the gravity where it is on every tick, as the tanks do, rather
  than by seven times that on every 7th tick, as other debris does.
- It flies for 600 ticks and then vanishes without bursting. Terrain it lands
  on takes a ship's shell's crater, and it bursts there. It never leaves a
  wreck.
- Its hits, direct and by its blast, name the side of the tank that fired it,
  a government's or a player's. Where a match credits a seat with a hit on a
  ship, a pod or a pilot on foot, a player's round counts once for that
  player's seat, whether it struck or only its blast reached, and a
  government's counts for none.
- This game's planets are weighted to pull about what Scorched's ground
  does, 18, at their surfaces, so a round's arcs are Scorched's, bent round
  the planet. On a 60-radius planet, a round fired at 45° comes down about 14
  units along the surface at power 12, 50 at power 24, 84 at power 30, more
  than a fifth of the way round, and 97 at power 32, after nine and a half
  seconds. From power 33 it is still in flight when it expires, and at full
  power it leaves faster than that planet's escape speed, about 45. A steep
  shot at the least power comes down beside the tank that fired it: about 6
  units off at 75°, within its blast's reach, and on the tank itself at 85°.

#### Shot records

Each round a tank fires has a record, for the tanks' controller (#240) to aim
by. `SpacewarsState::tank_shots` lists a tank's records, oldest first, and
`tank_shots_ended` lists each record that ended in the last step that advanced
the world. A paused step, or one after the game is decided, leaves that list as
it was, so a reader that polls tells the records apart by their ids. A record
holds only what happened, and predicts nothing.

- **Launch.** The tick; the elevation, facing and power fired with; the up
  they were measured from; the hull's motion; the barrel's pivot and
  direction; where the round started and its velocity; and whether something
  stood in the barrel. The barrel is cast against the world as the last step
  left it, less the bodies of what has left the game since.
- **Flight.** After each step: how many steps the round has flown, where it is,
  and how it moves. The step that stops a round leaves it turned back from what
  it met, so an ended shot's flight tells of the rebound.
- **End.** The tick, where the shot ended, and how. It burst on a tank, a ship
  or pod, a pilot on foot, a rover, a piece of debris, the ground or something
  else, or in flight where its shell died another way. Or it expired after its
  600 ticks, or its shell was removed. The end also tells what the shot's hit
  and blast together charged each body it harmed, in that body's health, the
  worst harmed first, for up to 8 bodies, and where the shot's energy went. A
  body is charged no more than it had left. In a match, a pod or a pilot on
  foot can lose less than a record tells: the match's rules pass over a round's
  hit on a body that the round touched in the six ticks before, and take only
  its blast (#784).

A command can name a target: a ship or pod, a pilot on foot, a tank, a rover or
a piece of debris, by the id that `DebrisState::id` tells. The target only
labels the records of the shots fired under that command. Such a record adds
where the target was and how it moved as the tank fired and after each step of
the round's flight, and the closest the round came to where the target really
was. It sees the target as the step that fires the shot begins, once the world
holds a body for just what the game then holds. Each step samples the target
before the shot's blast can push it and before a target that the step destroyed
leaves the world, so the record of a shot that destroyed its target still tells
where the target was. A target that the world holds no body for adds nothing.
Naming a target changes nothing about the round or what it can hit, and it
blocks no claim.

A tank keeps the records of its rounds in flight and of the last 4 of its
shots to end. When a tank leaves the world its ended records go with it, and
its rounds in flight keep theirs until they end. Clearing the tanks drops
every record, and a rebuilt world starts with none. No two shots in a world
share an id, and a record that has ended never changes.

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
Material revisions invalidate flags whose footing was removed or detached;
flags do not migrate onto loose fragments.

Each completed tick builds one surface-force snapshot, then gates every claim
from it. Work is bounded to two pilot/vehicle seats and 32 registered ground
actors, each with at most four native colliders. Approach selection
is allocation-free; the landing feet query their local Rapier contacts. There
are no all-object scans, extra physics steps or extra gravity solves. An
outside pilot adds one capsule; boarding removes only that capsule. Healthy
ships remain present while unoccupied. Loss removes or replaces the vehicle
assembly, and rebuilding reuses the assigned slot with a fresh full assembly.
Recovery adds bounded per-seat checks, not another physics step or gravity
solve. Expedition no longer spawns physical outpost terminals.

Each live tank is registered as a ground force the first time claims update
after it is placed, with its chassis body, its chassis and wheel colliders, and
its allegiance. Removing a tank takes its bodies out of the world, and the next
update drops its registration; tank IDs are never reused. Unregistering a live
tank lasts only until that next update. A tank whose armor fails leaves the
world in the step it fails, once, and `SpacewarsState::tank_losses` records it
with the hit that failed it. That step's claim update no longer counts it, with
no contact grace, so a claim it held can complete on that tick. Its wreck is
six ownerless breakup fragments in its side's color that keep its bodies'
motion and share their mass: ordinary debris, which never counts and which the
ground wears away. As with a rover's, its wheels' pieces start overlapping its
chassis's a little, and its first steps part them at about 2 u/s at most.
Nothing replaces it, and its loss touches no pilot, round rule or score. Other
ground actors use `register_ground_surface_force` with their native
body/colliders and player or government allegiance. Repeated registration does
not renew contact history. Owners must call `unregister_ground_surface_force`
on destruction/retirement if a physical wreck remains; missing native bodies
are pruned automatically. Reset starts with an empty registry; cloning
preserves its state. A restart builds a new world from its seed, and a new
match builds one from a fresh seed, so no tank, wreck or loss carries over.

## Observations and verification

Claim-enabled scenario observations are version **12**, with a top-level `version` and
`players` array in seat order. Each view retains stable pilot/owner/vehicle IDs,
input/transfer gates, landing and motion metrics, `travel_enabled`,
`ship_support_planet` and `pilot_support_planet`.

`planet_claim` is the active actor's focused planet view; `planet_claims` is
the complete inventory. Each reports owner, claimant, phase, progress, required
seconds/range, eligibility status, flag world position/normal/raised fraction
and capture/neutralization counters. Nonempty claimant-relative `opposition`
reports pilot, ship and ground-unit counts; `opposing_surface_forces` explains
a blocked eligible attempt. The session's `surface_forces` list exposes stable
IDs, allegiance, planet and elapsed contact gap, in stable ID order. Polling
observations does not update membership or grant progress. `outpost` is optional:
null in Expedition, populated in pinned fixtures. Expedition's `outposts` list is
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

# Real landed opponent -> blocked flag -> takeoff -> capture, both dirt shapes
cargo run --locked --release -p scenario-spacewars --example surface_opposition -- --replay

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

Surface-opposition regressions cover both flag stages at their completion
boundary, normal cancellation while opposed, a distant landed defender,
unbalanced and dead pilots, empty ships and boarding, actual hops and takeoff,
government forces without another player seat, friendly/unowned allegiance,
retirement with a retained wreck, bounded contact continuity, registration
limits, and clone/replay equality. Destroying the flag footing in both dirt
shapes neutralizes ownership while retaining the hostile force and conserving
material. The workflow example uses the main match host on a controlled
one-planet start; it never writes ownership or fabricates contacts.

Tank regressions place real government and player-owned tanks on a material
planet. They cover opposition from each seat, with readable allegiance and
color and no third seat; an overturned tank on the far side; a hop against a
departure that ends the block at the departure distance, with clone/replay
equality; a hop by a tank carrying another on its back, which keeps its block;
a tank riding a detached fragment while the claim completes around it; clearing
the last tank beside a flying ship; a lost flag footing that keeps every tank
with its allegiance and damage; a health bar that stays above a tank on its
back; a claim that completes on the tick its last defender dies, while the
defender's wreck never counts; and a world rebuilt from its seed after tanks
fought, as a restart rebuilds it, which holds nothing of them. Firing
regressions cover the barrel's aim on the planet's top and flank, a held
trigger against the reload and both limits, a round's exit and launch
velocity, its clearance, gravity and expiry, the credit for each side, and a
burst on terrain.

The driving fixture and the driving and firing regressions run on the tanks'
proving ground, `SpacewarsScenario::init_tank_proving_ground`, or on a planet
of their own weighed the same way. The proving ground is the terrain
fixture's world with its planet weighing what the planet of Surface Sortie
and `spacewars-terrain` does, so tanks are tried under the pull they fight
under. The terrain fixture's own planet keeps the arcade game's pull, about
39 times as strong, under which a full-power round came down about 8 units
from its tank. The surface games' pull is named once, as
`engine_rapier::SURFACE_GRAVITY`, which Scorched Earth, the labs and this
game's planets read.

Shot record regressions fire real tanks on the material planet, where the
ground pulls about 18, and in the rounds' weightless fixture above it. They
cover a record's launch against the round and the hull as it fired; a blocked
barrel, and its round's burst on what blocks it; a body that left the game in
the step before, which stands in no barrel; a new id for each shot, and no
record for a refused trigger; a target's motion as the tank fired and after
each step, and the round's closest approach to where a flying ship really was;
a target that the shot destroys, one that left the game in the step before, one
that a host moved, and targets that the world holds no body for; a burst on the
ground and direct hits on a tank, a ship, a pod and a pilot on foot, against
what each lost, and ejection protection; a burst among eleven rocks, which
lists the eight it cost most; two rounds that end in one step; expiry, and the
steps that advance nothing; a round that a laser destroys in flight, one whose
shell leaves the game, one whose shell comes back, and one that a host kills on
its last tick; a round that outlives the tank that fired it; the last four
ended shots, kept by when they ended; removal, clearing and a rebuilt world; a
named target that changes nothing but its shots' records and blocks no claim;
and the same records from a second run and from a clone taken with rounds in
flight. A round that stops on a rover is covered in the arcade world, beside
the shots' own tests.

The firing fixture is the tank fixture's third case, `FixtureCase::Fire`, on
the same proving ground, whose planet turns and is carried along. A government
tank on the planet's top is commanded by a pinned script, `FIRE_SCRIPT`: one
shot each reload, each with an elevation, a facing, a power and the target it
is meant for. The launcher runs it as `planetary-tank-fire` and
`planetary-tank-fire-angular`. `planetary_tank_lab --case fire --tanks 3` runs
it headless, and its result lists the cannon's settings, the cast's places and
the script. The script fires:

- a 45° lob at power 13.2 onto player 1's tank, 14 units clockwise along the
  surface, and one at power 18.3 onto player 2's, 26.5 units clockwise. They
  land after 94 and 139 steps and cost the tanks 22.4 and 22.8 health, which
  name the government as their source.
- its flattest shot, 5° at power 20, at seat 0's ship, which has come down 52
  units the other way and stands there on its tail, held by nothing but the
  ordinary rules. It is never quite at rest: it turns 9° against the ground
  over the run and slides half a unit. The ship is beyond the tank's horizon:
  the straight line from the muzzle to it passes through the planet, more than
  a unit under its surface. The round bursts on the ground half way there, 26
  units from its tank, and digs the run's one crater.
- a 40° lob at power 24 over that horizon onto the same ship, which it strikes
  after 243 steps for 9.4 life.
- a 33° shot at power 35 at seat 1's ship, which the script carries round the
  planet at 8 units a second, its hull about 24 units up. The round meets it
  after 65 steps for 8.5 life.

Each aim has room on both sides: the lobs at the tanks and the landed ship hit
across about 2 units of power, and the shot at the flying ship across 15. The
proving ground's rover drives behind the landed ship, 21 units from it at the
nearest, and no burst reaches it. The fixture's checks, which its unit test,
the lab example and the launcher entries all run, hold for both dirt shapes:

- every shot ended as its record tells, its target lost in that step what the
  record tells, and it harmed nothing else; the flat shot harmed nothing;
- each round had fallen more than a unit and a half toward the planet half a
  second after it left, and the long lob's fall turned with it round the
  planet;
- each round left with the hull's motion, about 2 units a second on this
  planet, on top of the cannon's;
- after every step the tank's records were those of its rounds in flight and
  of the last four of its shots to end, and two rounds flew at once at most,
  of the four a cannon may have;
- a step cast along a barrel once for each shot it fired, and not otherwise;
- each round carried one payload, and the ground burst dug one crater;
- material is conserved, and the crater's dirt is released in the configured
  shape;
- and the tanks' control work stays within the driving cases' bound for each
  tank.

A second fixture built from the seed replays every sample, the rounds' motion
included. No check counts what the weapons do for a payload in flight: by
reading, they look at every body once a step while one flies, however many fly.

Positive AI flight/capture fixtures now let their otherwise idle opponent
depart through real controls only when its landing feet support it on the
planet being claimed, releasing thrust as soon as that contact clears.
Captures elsewhere leave it idle, preserving the target for later combat.
Their subject controllers and physics remain unchanged. A separate tactical
test holds the defender in place for three seconds, requires no reported
capture, then verifies completion after departure. Recovery retains its
existing timeout and identifies surface opposition without extending its
clock or treating the changing reason as progress. Destination selection and
attacking defenders remain separate AI work.

The [#234 validation receipt](data/surface-opposition-234.json) records the local
source identities, 599 scenario tests, 589 AI tests, 41 client tests and both
workflow replay hashes. The AI CI Clippy command passes. Strict Clippy across
the entire scenario still reports existing lints outside this change. Native
test/build logs and the local UI captures are retained in the receipt's artifact
directory. The HUD workflow renders at 800×480, 1024×768 and 480×800; it is a
desktop check, not a new Pi deployment or a performance benchmark.

The receipt also records the CI follow-up on Ubuntu 24.04: repeated thrust
in the original fixture destroyed the seed-7 opponent at tick 6,246 before
combat began. Restricting thrust to blocking contact preserves the native
target; the capture-then-combat test now diagnoses premature target loss
directly. The failure was reproduced with the original source and runtime
before validating the corrected fixture. All 589 AI tests pass on both the
development host and Ubuntu 24.04 after the correction.

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
