# Surface gameplay and destructible terrain integration

The current [objective landing pass](../objective-landings.md) measures a walking
and jumping round trip to enemy flags before choosing a landing, and adds
bounded recovery for cramped returns under the parked hull. Prospective jetpack
access and reliable crater/exposed approaches remain unfinished.

The [usable landing pass](../usable-landings.md) strengthens hull and hatch
surveys and chooses a clear neighboring exit through the shared transfer logic.
The bot uses bounded rotation to finish one-foot touchdowns through ordinary controls.

The preceding [mission reliability pass](../mission-reliability.md) preserves physical
capture/departure milestones and separates pursuit measurements from capture
time. Match completion and promotion into ordinary Spacewars remain later work.

Latest gameplay slice: [solar collisions and post-capture pursuit](../solar-hunt.md)
fixes surface actors passing through the sun, adds heat, and sends the mission
bot after its opponent once all planets are secured.

Latest reliability slice: [parked ship return](../parked-ship-return.md) preserves
earned two-foot contact on material corners and bounds waiting at an unsettled
hatch. It retains the original boarding and physical recovery actions.

Latest reliability slice: [landing on generated material planets](../large-planet-landing.md)
adds committed touchdown, distinct-site retries and shared hatch prediction.
The reports separate landing/capture from later boarding and pressured missions.

The [generated material arena](../generated-material-arena.md)
adds three seeded, orbiting destructible planets, shared mission bots and sun
avoidance. Full ordinary-match lifecycle integration remains ahead.

Previous reliability slice: [ship return recovery](../ship-return-recovery.md)
provides a bounded replacement path when the assigned ship cannot be reached
after capture, using the existing human scuttle and rebuild rules.

The [controlled two-planet mission slice](../two-planet-missions.md)
adds destination selection, travel, local-task handoffs and replanning on fixed
destructible planets. Generated-world match integration remains ahead.

The preceding navigation slice, [recovery through damaged ground](../damaged-ground-recovery.md),
adds progressive local routes, replanning after knockback, wider crater launch
searches and measured rebuild relocation candidates.

The preceding reliability slice, [pod recovery lift and asteroid pressure](../asteroid-pressure.md),
adds a shared way to free tipped pods, bounded landing retries under changing
cover, and configurable environmental arrivals in material combat and duel.

The previous recovery slice, [jetpack routes across pods and damaged ground](../jetpack-recovery-navigation.md),
adds measured short flights, chained walking/flight routes, reachable flag
approaches, and stable settling at the destination. It builds on
[reachable replacement ships](../rebuild-access-ai.md) and their shared
hatch-access placement and measured bot relocation.

The [jetpack ground-navigation integration](../jetpack-ground-navigation.md) now
lets capture and recovery bots choose measured flights over their parked ship.
Terrain combat and duel equip humans and bots alike. The standalone
[crossing trial](../jetpack-crossing-ai.md) remains available for demonstration.

Implemented on the isolated `surface-terrain-integration` branch, combining
Surface Expedition from `6cf12def6b2a84a1a0ab45a26acee5f4a9bca00c` (PR #49)
with the preserved terrain checkpoint `4acfaa5` on `terrain-checkpoint-20260907`.
The `spacewars-terrain` launcher now runs the combined one/two-player scene.
See [the playtest guide](../spacewars-terrain.md) for controls and
[the endurance report](../terrain-endurance.md) for measured validation.

## Completion target

The user accepted the first Pi gameplay slice and clarified that merging is
deferred until this work is ready for integration into ordinary Spacewars.
Working AI is a required part of that milestone. The controlled human-playable
scene establishes the shared mechanics; it does not complete the overall goal.

The remaining target is a playable Spacewars match on generated destructible
planets, with human and AI pilots using the same natural landing, disembarking,
capture, mining and recovery rules, alongside combat and match outcomes.
The [AI integration plan](surface-ai-integration.md) describes the next slice
and the evidence needed before expanding it into the normal game.

The controlled combat slice now adds the shared laser/cannon pipeline,
`rule_pilot_v4` pursuit and firing, and recovery followed by renewed combat.
See [Material combat V4](../material-combat-ai.md) and the opt-in
[cover-aware capture sortie](../tactical-surface-sorties.md), which coordinates
landing and departure around the existing surface and recovery tasks. Shared
[ground navigation](../ground-navigation-ai.md) now adds enemy flag traversal
and routes back to ship hatches on measured material. Pod escape and sustained
random asteroid pressure now have dedicated regression/diagnostic coverage.
Controlled two-planet missions now exercise destination selection and task
handoffs. The generated three-planet arena now varies sizes, spacing and orbital motion.
Ordinary-match lifecycle integration remains ahead, along with broader
navigation through heavily damaged ground.

## Player outcome

The player lands the ship on surviving ground, exits, and controls the spaceling
to capture the planet. The ownership flag rises at the spaceling's actual
ground contact. The old external landing berth and center flags are removed
from this combined gameplay mode, along with berth-based capture and holding.

Reuse the merged Expedition claim rules: a three-second stationary hold raises
a flag on neutral ground; replacing an enemy flag requires approaching within
three units, lowering it for three seconds, then raising a replacement for
another three seconds. Walking, jumping, losing support, or being knocked down
interrupts the unfinished stage. Landing alone never captures a planet.
Expedition has no outpost terminal or repair service; the older pinned Sortie
fixtures retain their separate outpost experiments.

Reuse the merged recovery lifecycle too: occupied ship loss creates an escape
pod, empty ship loss preserves its on-foot pilot, and eight supported, stationary
seconds on an owned planet rebuild the assigned ship. The replacement must
settle before boarding. The combined journey is land, exit, claim, excavate,
lose support, recover, rebuild, board, and depart.

Finished-match survival rule, confirmed by the user on 2026-09-10: a living
spaceling or escape pod keeps the player in the game even with no full ship
and no owned planets. They may claim or reclaim a planet and rebuild through
the ordinary mechanics. Losing the last flag during recovery must preserve
that opportunity. Match integration must therefore replace the legacy
no-ship/no-planets elimination rule; terminal pilot defeat and any respawn
rules remain to be designed. See the
[continuation investigation](../continuation-investigation.md) for the planned
work and acceptance cases.

The first acceptance scene combines one destructible planet with the shared
one/two-player surface loop. Generated-world expansion follows validation of
this scene. Preserve the established ordinary Spacewars behavior and existing
diagnostic workloads so their regression evidence remains meaningful.

## Division of work

| Work | Source |
| --- | --- |
| Pilot/vehicle identities, ship/pod landing, boarding, travel, multiplayer controls/views, planet flags, and ship loss/rebuilding | Merged `origin/main` checkpoint `6cf12de` (PR #49) |
| Material fields, committed edits, fragmentation, terrain contacts, bounded interior gravity, spaceling get-up behavior and endurance diagnostics | Terrain checkpoint `4acfaa5`, preserved in `/home/data/workspace/space-wars2` |
| Material-aware support/queries, flag footing, destruction invalidation, and combined gameplay validation | Terrain task owns `/home/data/workspace/space-wars-integration`; surface task supplied read-only review of landing, claims, recovery and input gates |

The merged Expedition report records 861 workspace/all-target tests, 14 real
UI workflows, unchanged navigation/strategy baselines, Windows/ARM64 compile
checks, 16 optimized recovery tests, and accepted basic Pi playtesting. These
are the surface task's recorded results, not reruns or combined-branch evidence.
The terrain task separately records its desktop/Pi endurance validation in
[the endurance report](../terrain-endurance.md). Neither report validates the
new material-ground gameplay loop by itself.

## Integration boundaries

These are the invariants used to combine the two implementations.

1. **One step and gravity solve.** Fold terrain commits and diagnostics into the
   shared `step_with_surface_pilots` path. Preserve fragment recipients and all
   spaceling recipients. The current branches both use gravity tag `7` for
   different recipients; give pilots and fragments distinct named tags.
2. **Actual terrain contacts.** The surface branch currently recognizes its
   rough circular planet collider. Material planets replace that circle with
   chunk rectangles. Update both collision filters and support identity, so
   feet and spacelings collide with the same surviving material as other actors.
   A detached fragment is a separate body and cannot claim its former planet.
3. **Ground queries and clearance.** Hatch positions and landing-assist altitude
   currently use nominal planet radius. Query surviving ground near the actual
   ship or pod, including excavations, and retain capsule clearance before exit.
   Recovery already uses bounded local terrain rays and conservative vehicle
   clearance: adapt its filters/support checks to material terrain rather than
   replacing that placement algorithm. Blocked exits or rebuilds must not create
   duplicate bodies or vehicles.
4. **Edit ordering.** Commit edits before transfer or capture eligibility is
   consumed. A contact from a removed collider cannot authorize landing,
   boarding, a claim, or rebuilding. Revalidate cached landing state after
   support loss and invalidate pending construction when its eligibility changes.
5. **Flags and ownership.** The merged claim system already uses the canonical
   planet owner and stores the contact position/normal in the planet's local
   frame. Extend that anchor with material footing validity. Keep the existing
   raising/lowering/contest rules and frame transforms. Collider replacement
   during chunk remeshing must not remove a flag whose material support survives;
   actual removal or detachment of the supporting material must invalidate it.
   Do not attach planet ownership to a detached fragment.
6. **Character mechanics.** Add the surface branch's capsule-clearance and
   collision-group support without overwriting the terrain branch's tested
   get-up and recovery changes.
7. **World tuning.** Reuse the accepted surface profile for the combined scene
   and retain bounded interior gravity for material planets. Its surface
   acceleration and spin must suit the shared flight/character controllers.
   Keep historical stress-fixture parameters explicit when comparing reports.
8. **Client lifecycle.** Combine launcher registration, settings, seat mapping,
   help text, camera/minimap layout, pause and restart. Reuse the surface input
   transfer gate so a held button cannot turn into an unintended jump or thrust.
   Expose excavation in the combined playtest without displacing either seat's
   jump or transfer controls, and keep automated edit-boundary tests independent
   of that input mapping.

The actual merge had textual conflicts in seven files: `Cargo.lock`, `README.md`,
client scenario registration, input, Slint UI, and the Spacewars simulation and
physics files. Resolving those also required separate pilot/fragment gravity
tags and retaining one shared `velocity_at_point` implementation.

Review exposed additional edit-boundary problems beyond the textual merge.
Structural changes now defer spatial queries until the normal physics step
refreshes their index. Durability-only edits keep identical colliders; structural
remeshing preserves earned landing time only when both sampled footing cells
survive. The get-up input keeps the real button edge and holds a fresh request
through a dirty query index only while the button remains down. Regression tests
cover stale clearance, repeated harmless edits, and held jump across edits.

## Accepted flag support-loss policy

The user accepted this rule: removing or detaching the material supporting the flag
removes the flag, clears unfinished claim progress, and makes the retained planet
neutral. It never gives ownership to whoever caused the damage. A supported
spaceling must complete the ordinary neutral-planet claim to own it again.
Loss of ownership interrupts unfinished rebuilding through its existing gate.

This makes destroying the footing an alternative to lowering an enemy flag,
while keeping capture an on-foot action. It also means an owner can accidentally
neutralize their own planet by mining beneath the flag. The flag is currently
the only owned planetary object; future owned structures will need an explicit
extension of the ownership rule. Harmless remeshing,
nearby excavation that leaves the footing intact, and simple planet motion must
not trigger it. No simulation of a falling flag is needed for the first slice.

## Acceptance checks

| Check | Required behavior |
| --- | --- |
| Physical landing | Ship and pod rear feet settle on surviving terrain; nose/hull contact or an empty nominal surface cannot grant landing. No berth sensor, hold, or pad is active. |
| Exit and boarding | Repeated transfers preserve pilot/ship identity and health; exactly one capsule appears on exit and disappears on boarding. Occupied or excavated hatches fail safely. |
| Capture | Staying aboard never captures. Preserve the merged neutral raising, nearby enemy lowering/replacement, and contest rules on real material contacts. |
| Flag anchor | The flag appears at the qualifying ground contact, follows rotating/translating terrain, and survives harmless collider remeshing. Removal/detachment of its footing follows the chosen ownership rule. |
| Interrupted and competing claims | Movement, jump, knockdown, and support loss interrupt an unfinished claim. Simultaneous opponents cannot win by update order; distinct planets update independently. |
| Two players | Inputs, transfer gates, camera focus, vehicle ownership, and claims remain independent. Neither pilot can board the other's ship or exit into an occupied space. |
| Mining during surface play | Removing ground under ship/pod feet, a pilot, or a flag invalidates the affected state at the edit boundary. Falling fragments retain finite motion and conserved material. |
| Recovery and lifecycle | Jump/get-up works after a fall; occupied/empty ship losses preserve the right pilot; rebuild eligibility resets after support/ownership loss and blocked placement never duplicates a vehicle. Pause advances no simulation; restart resets claims and actors; cloned states continue deterministically within one build. |
| Regression and duration | Existing terrain/recovery and surface/multiplayer suites pass, ordinary navigation/strategy baselines remain stable, and combined sessions run up to 180 simulated seconds with health audits. |
| Pi playtest | One identified combined image runs the scene with controller input, readable local flags/HUD, and no service restarts. Record performance and screenshots. |

## Handoff and deployment

The checkpoints are `6cf12de` and `4acfaa5`; the original terrain checkout is
preserved. The combined work lives in `/home/data/workspace/space-wars-integration`.
Focused tests, existing regression suites, real client workflows, and combined
endurance cases are the device-deployment gates.

Coordinate the shared Yocto build directory and `spacewars.local` deployment
with the surface task. Only one task builds/deploys at a time. Record the source
checkpoint and exact image/binary used so a later playtest cannot silently run
the other branch's image.

The latest claim-footing recovery slice is documented in
[claim footing recovery](../claim-footing-recovery.md).

The generated arena now includes a solid sun, heat, post-capture pursuit and
[solar-aware landing/departure](../solar-landing.md). Match completion and
ordinary Spacewars integration remain separate from this checkpoint.
