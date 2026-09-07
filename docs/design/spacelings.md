# Spacelings and natural surface support

Status: basic Spaceling Lab and knockback/recovery implemented, regression-tested,
and manually playtested. Tracks
[issue #41](https://github.com/aortez/space-wars/issues/41).

## Naming

A **spaceling** is the small humanoid creature, whether controlled by a player
or a bot. This names the organism, not its role: a spaceling can walk, pilot a
ship, or operate a rover. **Entity** remains the generic engine term for an
object or assembly. The reusable mechanics live in `engine-rapier::spaceling`;
the dedicated testbed is **Spaceling Lab** (`spaceling-lab`).

## First slice

Build a playable `spaceling-lab` scenario, using one dynamic capsule per spaceling in
the canonical Rapier world. Arms and legs are animated render geometry, not
separate physical bodies. This keeps the baseline small enough to measure
before considering articulated crowds or ragdolls.

Gravity comes from `engine-gravity`, independently of terrain geometry.
Opposite gravity defines up; with no gravity, retain the last valid up direction.
Walkable contact normals define support. A nearby planet, wall contact, or
sensor overlap alone cannot make a spaceling grounded.

Walking applies bounded acceleration toward a tangential target velocity,
relative to the supporting body's velocity at the contact point (including
rotation). Air control is weaker. Upright control is bounded and acts on angular
velocity, not pose. A fresh jump press while supported adds outward velocity;
holding the button through a landing does not repeat the jump. Ordinary gravity
and Rapier contacts remain responsible for flight and landing.

The lab starts on a small rotating planet with a shallow ramp/obstacle. Use
keyboard left/right (or A/D) and Space, or NES d-pad left/right and A. Diagnostics
show grounded state, support identity, relative speed, gravity, and jump count.
The usual host controls pause, restart, and return to the launcher.

Run instructions and diagnostic definitions are in [Spaceling Lab](../spaceling-lab.md).

Acceptance tests cover both directions, jump/land, slopes, moving/rotating
support, wall rejection, removal of support, zero gravity, and deterministic
action sequences. Visual verification must also check both renderers and
readability at the normal client viewport.

The first slice passes workspace tests, the six real-client functional
workflows, and Rust 1.89 checks. Desktop vector and software-raster screenshots
were visually checked. The full-lap test covers both obstacles in both walking
directions. Physical gamepad feel remains a manual acceptance check.

## Boundaries

`engine-rapier::spaceling` owns the reusable body/controller and contact queries;
the scenario owns input decoding, gravity participants, fixture construction,
and presentation. Rapier poses/velocities remain authoritative. No new Rapier
pipeline, per-spaceling terrain scan, or second position integrator is introduced.

This is a contact-based arcade controller, not a biomechanical simulation.
Automatic stair climbing, coyote time, jump buffering, ragdolls, NPC policy,
inventory, damage, ship entry/exit, and Spacewars integration are follow-ups.
Do not claim crowd performance from a single-spaceling demo; benchmark populations
before choosing more expensive mechanics.

## Second slice: knockback and recovery

Keep physical support separate from balance (`Balanced`, `KnockedDown`,
`Recovering`). The same single capsule handles all three; visual limbs are
still not separately simulated. Off-center impulses use the canonical world's
mass/inertia-aware binding, also available to future explosions and collisions.

Unexpected velocity changes and excessive spin can overwhelm the controller.
Knockdown disables locomotion and self-righting, enables ordinary contact
friction, and waits for sustained settled support. Recovery ramps bounded
angular correction and ground braking. Brief contact gaps have bounded grace;
support removal, prolonged flight, gravity loss, or another strong hit aborts
recovery. There is no free-space auto-recovery, pose snapping, or buffered jump.
The supplied gravity step is accounted for in disturbance detection; it is not
integrated a second time. The caller applies controls once per physics step.

Spaceling Lab exposes a held-state, edge-triggered shove on gamepad B/keyboard X,
with balance colors, recovery progress, transition counts, and a short impulse
marker. Deterministic fixtures cover impacts, moving support, recovery loss,
zero gravity, and normal movement remaining balanced. This remains the same
player/bot-independent `SpacelingControl`; no NPC policy or damage is added.

Next: a small Spacewars surface-outpost experiment to test shared-world actors,
vehicle entry/exit, and useful surface interactions. Population measurements
remain necessary before expanding to crowds; articulated ragdolls are optional
follow-up work.

## Third slice: opt-in surface sortie

Implemented on `spaceling-surface-sortie`; the first-pass round trip and assisted
physical landing/minimap refinements passed manual playtesting.
The [fixture guide](../surface-sortie.md) documents its boundaries.

Start with `surface-sortie`, a single-player fixture inside the Spacewars scenario
crate. It uses the existing Spacewars world, ship assemblies, gravity pass, and
physics step, not a second lab physics pipeline. A pilot has a stable identity
and owner independent of the ship, and is either aboard that ship or represented
by one physical spaceling outside it. Boarding removes only the external body;
it does not recreate or heal the ship. An unoccupied ship remains in the world.

Use a fresh interaction press to disembark from a physically landed ship, and to reboard
near its surface access point while supported and settled. Reject unsafe exits,
blocked placement, remote boarding, and unavailable vehicles with visible
feedback. Require controls to return to neutral after a successful transfer.
The fixture no longer uses the old elevated berth. Two rear feet on the dynamic
ship body establish landing through actual contacts, alignment, low relative
speed/spin, and a short settling interval. Nearby nose-outward flight assistance
damps sideways motion/spin and bounds descent without auto-pointing or hovering.
There is no kinematic hold, radial snapping, or hidden takeoff impulse. The hatch
follows the landed ship, not the old pad marker; clearance uses actual colliders.
A translucent north-up minimap preserves world context when flying away.

The original fixture starts parked on a slowly rotating planet with a stationary center.
The spaceling inherits the local surface velocity at spawn; after that only
gravity and Rapier contacts move it. Do not copy the rover's scripted-orbit
transport into this controller. The fifth slice below adds controlled orbital
evidence; arbitrary generated Spacewars orbits still need an explicit policy.

The initial slice disabled capture/services, weapons, and asteroid spawning in
the fixture. It leaves the ordinary Spacewars game and bot observations unchanged.
Pilot damage, ship destruction while unoccupied, rescue/pods/elimination, NPC
policy, and the contested outpost reward loop require explicit later policy;
the fixture reports a lost vehicle and offers restart rather than inventing
those rules. The next slice below makes walking useful through an intact
outpost capture and repair service, without adding rebuilding.

Acceptance: rear-first landing at multiple bearings, physical takeoff/return,
bounded assist, rejected hover/nose contact, parked exit/walk/jump/reboard,
moving-surface velocity inheritance,
actual collider clearance, fresh-input gating, preserved ship state, rejected
unsafe transitions, deterministic restart/observations, both renderers and the
standard launcher/pause flow, plus unchanged ordinary-game regressions.

## Fourth slice: surface-outpost capture and repair

Implemented on `surface-outpost-loop`; the deployed loop passed user-reported
controller playtesting on the Raspberry Pi on 2026-09-06. Extend
Surface Sortie with one intact terminal attached to the rotating planet body
as a solid collider. The ship starts at 75% health so the reward is visible.
Disembark, walk to the terminal, remain supported/balanced/settled for three
seconds, capture, repair the nearby landed ship, return, and depart.

Keep three separate contracts: physics establishes support and landing;
scenario policy establishes outpost ownership; service policy checks ownership,
ship availability, landed state, and range before gradually restoring health.
The outpost owns no terrain, creates no landing hold, and grants no remote or
airborne repair. Reboarding still preserves the same ship without itself
changing health. The reusable spaceling controller needs no capture code.

Neutral-outpost capture is only one path toward establishing a presence, not
the sole future way to claim a planet or get it running. Keep planet claims,
infrastructure ownership, and operational services distinct. For example,
building a new base or restoring abandoned infrastructure could provide other
paths; their specific rules are not decided or implemented by this slice.
Do not require every claimable planet to spawn with a neutral outpost, or
equate ownership of one installation with control of all terrain. The current
capture-enables-repair rule is fixture policy, not an engine invariant.

Capture requires support from this planet or its terminal, not merely any
grounded contact nearby. Leaving, jumping, losing balance, or excessive
support-relative speed resets partial progress. Ownership remains secured on
departure. The owner-colored physical flag and square minimap marker expose
that state; the HUD shows capture progress and service eligibility. Structured
observations include owner/claimant, progress, captures, and health restored.

This is deliberately one operator and one outpost, not a general contested-base
system. The terminal is intact and non-destructible; terrain removal and service
invalidation need explicit later policy. There are no new weapons, resources,
pod rebuilding, pilot death rules, or changes to ordinary Spacewars/bots.
Orbital compatibility and independent pilot/vehicle loss rules still precede
ordinary-game integration.

Acceptance: an action-driven capture/repair/departure round trip; interrupted
capture and wrong-support rejection; solid rotating terminal/body-count bounds;
friendly, live, in-range, landed repair with rate/clamp checks; deterministic
replay and restart; capture/ownership rendering in both paths and window
orientations; unchanged landing, normal-game, and pinned bot regressions.

## Fifth slice: orbital surface motion

Implemented on `surface-sortie-motion`. Keep the stationary-center scenario and
register `surface-sortie-orbit` as another preset of the same scenario, controls,
landing, and outpost loop. A headless translating preset isolates linear motion.
The orbital fixture adds one sun and a prescribed circular path whose center
acceleration matches that sun's gravity. Nearby actors receive the unmodified
shared gravity field once; no actor attachment, frame transport, or common-field
subtraction is added. The unused second ship slot is not simulated.

Landing, boarding, and gravity sample the completed terrain pose before the
next kinematic step. Contact-point velocity accounts for Rapier's potentially
offset center of mass. The fixture schedules orbit/spin only once per tick.
Bounded landing and spaceling controllers retain their existing force limits.

Versioned observations expose the completed frame, relative motion, scripted
acceleration versus external gravity, support losses, knockdowns, damage, and
planet-local idle drift. The HUD shows a compact subset. Deterministic tests
cover the full orbital round trip, extended idle support, faster/reverse motion,
multi-bearing landings, independent jumps/takeoff, and separation when motion
exceeds the support budget. Desktop lifecycle/render tests cover both presets.
The user reported successful playtesting of the deployed orbital preset on the
Pi/controller on 2026-09-06.

This is evidence for a controlled acceleration envelope, not a general solution
for ordinary planets whose prescribed orbit and gravity masses are chosen
independently. Decide that policy before enabling natural landing there. Pilot
loss/rescue, contested infrastructure, terrain, and bot intent remain separate
slices; this controlled preset's acceptance does not cover arbitrary generated
orbits or combat.

## Sixth slice: generated-world compatibility evidence

Implemented on `surface-generated-compatibility`. The generated diagnostic keeps
all ordinary planet/sun masses and prescribed paths, uses an explicit selected
planet index, and runs independent landing, idle, walking, jump and takeoff
probes. Prerequisite failures remain visible instead of masquerading as passes.
An untuned launcher preset provides visual reproduction at planet 0 / bearing 0.

The first 72 generated cases confirm a major force/motion mismatch, not merely
a radius or landing-assist problem. Controlled radius changes from 15 to 150
pass at lab gravity; generated surface gravity is roughly 650–712 versus 18
in the lab. High gravity overwhelms takeoff/jumps and disrupts balanced support;
rapid spin and unmatched external acceleration add independent constraints.
See [the compatibility report](../surface-compatibility.md) for reproducible
commands, criteria, results and limits. Ordinary game tuning and bot baselines
remain unchanged.

The same slice now includes the explicit **Surface V1** experiment, alongside
the raw baseline: lab-scale planet gravity, a gentler sun, spin bounded by local
gravity, central-field-matched circular orbits, and exterior flight clearance.
All generated sources remain active and the ship/spaceling controllers are
unchanged. This remains a cheap kinematic world, not full N-body motion or
actor-relative gravity compensation. `surface-sortie-world` is the selectable
visual preset; `--profile both` runs paired headless cases.

The initial 72 cases pass all five probes; the wider 620-case matrix passes
3,093/3,100 probes, with seven passive approach failures retained as evidence.
Four sampled planets pass the full outpost round trip. Extended idle tests
expose brief support gaps and slow drift without damage/knockdown, so this is
not a universal contact-stability claim. Playtest the profile and decide its
acceptance envelope before multi-pilot, loss/rescue, contested services and bot
surface-intent integration. Ordinary Spacewars defaults have not changed.

## Seventh slice: single-pilot planet-to-planet expedition

Implemented on `surface-planet-travel`, following the compatibility work in
PR #47. The opt-in `surface-expedition` preset shares Surface Sortie's physics,
gravity and controllers, but follows the destination rather than a permanently
pinned planet. Approach selection prefers actual foot contacts, then nearest
surface with hysteresis; landing requires fresh settling on that same body.
Hatch transfers and repair validate physical planet identity independently of
proximity. Spaceling diagnostics/site focus are independent of the parked ship.

Sites are a collection with stable IDs and independent ownership/capture/repair
state, one per generated planet in this fixture. The minimap and world show all
sites, while the HUD focuses the active planet's site. No new terrain bodies or
gravity solves are introduced. Existing raw/profile probes remain pinned and
ordinary Spacewars is unchanged.

Two controlled action-only journeys cover rotating planets with stationary and
translating centers. Generated arrivals, stale-frame reset, foreign services,
unrelated boarding support, resource bounds and deterministic replay cover the
new boundaries. The initial Pi deployment and user playtest on 2026-09-07 were
positive. This does not yet prove all generated routes, multiplayer, combat or
rescue rules. See [Surface Expedition](../surface-expedition.md).

## Planet and ship direction

The continuous surface and external berth from PR #40 fix the interior-bay
problems, but the special landing area is an interim compatibility mechanism.
Long term, ships should land naturally on suitable physical surfaces. Surface
normal, support-relative motion, clearance, and pilot intent should establish
landing, independently of ownership or services.

Capture, healing, and pod rebuilding remain explicit scenario policies.
Natural contact anywhere must not automatically provide those services.
Surface Sortie now exercises natural rear-first landing in isolation. Replace
the ordinary game's berth in a later slice, with launch/landing, damage, bot,
and moving-planet regressions; neither lab changes those ordinary-game rules.

[Cell-based terrain (#16)](https://github.com/aortez/space-wars/issues/16)
should start from the continuous exterior, not recreate a cavity or ownership
gate. Bases and mining machinery belong on supported exterior geometry.
Terrain removal must invalidate physical support and explicitly reconcile
infrastructure and service eligibility. Spacelings and rovers provide small testbeds
for that contact contract before it is used by ships and planetary bases (#13).
