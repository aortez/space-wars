# Surface gameplay and destructible terrain integration

Integration preparation, refreshed after PR #49 on 2026-09-07. Surface gameplay
is merged into `origin/main` at `6cf12def6b2a84a1a0ab45a26acee5f4a9bca00c`.
The terrain checkout remains at `2504452` plus its uncommitted integration and
gravity work. This document does not claim the two lines have been combined.

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

The first acceptance scene combines one destructible planet with the shared
one/two-player surface loop. Generated-world expansion follows validation of
this scene. Preserve the established ordinary Spacewars behavior and existing
diagnostic workloads so their regression evidence remains meaningful.

## Division of work

| Work | Source |
| --- | --- |
| Pilot/vehicle identities, ship/pod landing, boarding, travel, multiplayer controls/views, planet flags, and ship loss/rebuilding | Merged `origin/main` checkpoint `6cf12de` (PR #49) |
| Material fields, committed edits, fragmentation, terrain contacts, bounded interior gravity, spaceling get-up behavior and endurance diagnostics | Current terrain work in `/home/data/workspace/space-wars2` |
| Material-aware support/queries, flag footing, destruction invalidation, and combined gameplay validation | Terrain task as integration owner; surface task as proposed reviewer for landing, claims and recovery |

The merged Expedition report records 861 workspace/all-target tests, 14 real
UI workflows, unchanged navigation/strategy baselines, Windows/ARM64 compile
checks, 16 optimized recovery tests, and accepted basic Pi playtesting. These
are the surface task's recorded results, not reruns or combined-branch evidence.
The terrain task separately records its desktop/Pi endurance validation in
[the endurance report](../terrain-endurance.md). Neither report validates the
new material-ground gameplay loop by itself.

## Concrete merge boundaries

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

An earlier read-only three-way merge rehearsal against common ancestor `57eb527` found
15 shared modified tracked files, with textual conflicts in 7: `Cargo.lock`,
`README.md`, client scenario registration, input, Slint UI, and the Spacewars
simulation and physics files. This is a snapshot of ongoing work, not an
exhaustive conflict count or proof that cleanly merged code is correct.
Repeat that assessment against `6cf12de`; the merged flag and recovery work
postdates the original rehearsal.

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

The surface checkpoint is available at `6cf12de`. Establish a named terrain
checkpoint first, preserving all implementation, tests, untracked terrain modules,
lockfile changes, and validation records. Combine it with the merged main in an
isolated integration branch/checkout. Resolve the boundaries above as well as textual
conflicts. Run focused tests first, then the existing regression suites, real
client workflows, and the combined endurance cases before device deployment.

Coordinate the shared Yocto build directory and `spacewars.local` deployment
with the surface task. Only one task builds/deploys at a time. Record the source
checkpoint and exact image/binary used so a later playtest cannot silently run
the other branch's image.
