# Smooth gameplay camera

The material Spacewars match and Surface Sortie/Expedition presets share a
small per-player camera controller in the client. Classic Spacewars and unrelated
scenarios keep their existing camera behavior. This implements the first slice
of [#72](https://github.com/aortez/space-wars/issues/72); no new launcher scenario
or input binding is needed.

## Feel and limits

- Normal motion follows the active ship or spaceling directly. Only composition
  changes are eased, so fast flight and steering do not acquire camera follow lag.
- Focus offsets have a 0.14-second half-life. Camera height has a 0.16-second
  half-life, interpolated in log space so equal zoom ratios feel alike. Roughly
  95% of a fixed focus/zoom change completes in 0.6/0.7 seconds, without overshoot.
- Nearby-opponent framing enters below 260 world units and persists until the
  opponent reaches 300 units. A landed/on-foot player, or an opponent no longer
  eligible for combat framing, clears that hold immediately; the view still eases.
- Boarding/disembarking rebases the framing offset before easing, preserving
  the previous world-space view when the active actor changes.
- During a transition, narrow panes may widen the visible world enough to keep
  the active actor inside 10% margins. This prevents a lingering focus offset
  from hiding the player during zoom-in and respects the current viewport aspect.
- Pause and completed rounds freeze the camera. A fresh session/restart starts
  at its desired view, without animating in from an old match. A discontinuous
  actor relocation greater than 1.5 times the larger old/new target height
  (at least 100 world units in one update) also resets the camera rather than
  panning across the universe.

This does not add velocity look-ahead, camera rotation, shake, manual zoom, or
new combat/landing rules. Focus and zoom timing remain tuning constants for now.

## Ownership and cost

`SurfaceSortieScenario::camera_target` produces read-only framing intent: a
desired `Camera2`, active-actor anchor/identity, and whether an opponent is framed.
Landed framing uses the hull and nominal hatch transforms, not a terrain access
query. The actual boarding gate still validates the real floor. On-foot framing
reads the existing body/contact snapshot. There are no added terrain raycasts,
clearance searches, physics bodies, or gameplay RNG calls.

The client allocates one small camera record per player at construction. It
advances those records only when the simulation tick advances, with bounded
scalar arithmetic and no per-update allocations. Rendering is read-only:
requesting frames at 30, 60 or 120 Hz does not change camera history, physics,
bot observations, or deterministic state. Headless scenario rendering remains
stateless and uses the target view; it does not need a camera controller.

The client passes the same displayed camera to world construction, terrain
culling, minimap footprints, and pointer projection. The uncropped reference path
uses that camera too. Raster scale changes neither the framing nor the logical
HUD layout; raster and vector consume the same frames.

## Verification

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked \
  -p engine-client -p scenario-spacewars camera

SPACEWARS_CAMERA_ARTIFACTS="$PWD/target/camera-captures" RUST_MIN_STACK=16777216 \
  cargo +1.89.0 test --locked -p engine-client --bin engine-client \
  camera_transition_fixture
```

The fixture physically lands both ships, exits P1, boards again, then uses
swept-wing thrust to take off. It asserts that camera changes are gradual and
writes real Slint software snapshots at 1024×768 and 800×480, raster scale 2.
Filename suffixes are ticks since the named transition, at 60 Hz: `000`, `012`
(0.2 seconds), `036` (0.6 seconds), and `060` (1 second), as applicable. It is an
existing Surface preset driven through ordinary actions, not a separate game.

Other regressions cover separate combat enter/exit thresholds, moving anchors,
both zoom directions, update-rate independence, active-actor visibility in
portrait panes, pause/restart/teleport, and equal gameplay observations with or
without camera/render work. A prescribed mid-transition camera additionally
checks culled/uncropped pixel equality at two viewport shapes and raster scales,
matching minimap bounds, and the pointer projections used by the host.

For manual testing, watch Spacewars autoplay on a Picade: approach and leave
another ship, then land, disembark, board and take off. The surrounding world
should ease into its new framing while the active actor stays visible. The
[compact HUD](gameplay-hud.md) stays anchored to the screen throughout.
