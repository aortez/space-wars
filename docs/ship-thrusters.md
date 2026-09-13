# Ship thrust feedback, missiles and visual fixtures

The current material-planet Spacewars match and Surface Sortie/Expedition use
the same directional ion effects. Classic Spacewars retains its legacy exhaust.
This slice changes presentation, not handling, weapon mechanics, landing gear
or input bindings. It does not add another launcher scenario.

## Missile presentation

Supplied missiles have a slim gray body, small tail fins and the original
yellow-orange tip.
Their drawn length is 2 world units (previously 4), and their maximum width is
0.8 (previously 2). The rails and reload feed travel are shortened to match.
Mounts sit 1.5 units farther aft and 1.4 units farther inward than the original
large-round mounts. The rounds partially overlap the fuselage edges, staying
attached with either open or swept wings. Launch origins move with the rails;
the fixed body mounts always point forward. Loaded, reloading and flying rounds
use the same two-polygon artwork. Reloading rounds are dimmed as before.

Flying tips point along the velocity vector; the renderer accounts for the
engine's clockwise vector-angle convention when applying its counterclockwise
transform. This corrects the old mirrored flight orientation without changing
projectile motion. Speed, mass, damage, collision geometry, recoil and reload
timing are unchanged. Classic Spacewars keeps its original cannon shell.

Capture the hardware through the same software/vector adapters as the thrusters:

```sh
SPACEWARS_MISSILE_ARTIFACTS=target/ship-missile-captures \
  cargo test --locked -p engine-client --bin engine-client missile_visual_fixture
```

`index.html` contains loaded, swept-wing, half-reloaded, launched and rotated
flight cases at close-up, desktop, Picade, HyperPixel and wide-combat scales.
`overview.svg` is the close-up contact sheet. Raster scales 1 and 2 are both
captured; the gallery displays each at its intended player-pane size.
Core regressions check silhouette dimensions, gray/warm-tip colors, fuselage
overlap throughout the wing sweep, matching rail/flight artwork, nose direction
in every quadrant, safe clearance of the firing ship, and retained weapon stats.

## What the jets show

- Forward thrust has a cyan/white attached plume and a short blue wake, even at
  low speed. It does not wait for the legacy 150-world-unit speed threshold.
- Turning fires an opposing pair of fore/aft side jets. Left/right means
  rotation, not new strafing controls. Rate-control corrections and automatic
  angular damping are visible too; once the desired turn rate is reached,
  there need not be a continuous torque jet.
- Reverse acceleration uses paired forward-facing jets. The visual model and
  fixture cover this direction, but the current Surface controls do **not** gain
  a reverse button in this change. Braking forward motion uses these jets too.
- Braking uses violet pulses on the nozzles opposing actual planet-relative
  motion and rotation. It is not an arbitrary all-jets-on animation. Braking
  while stationary does not imply a thrust force.
- The controller publishes its applied linear/angular acceleration, including
  the speed governor and landing assistance. Gravity, collisions and recoil do
  not themselves light fictitious thrusters. A held thrust input can legitimately
  show no plume once the speed governor reduces engine force to zero.

Each active jet has a steady core plus two outward-moving ion packets. The
nozzles sit outside the rendered hull rather than inside the old engine triangle.
Pulse phase advances only with simulation time. The wake fades after release;
pause leaves it unchanged, and vehicle form changes clear the previous wake.
Humans and bots use the same controller/output/render path.

## Cost and boundaries

`scenario-spacewars::thrusters` owns presentation-only state: at most 24 short
trail segments and seven attached jets per ship, with at most 52 effect
primitives in a player frame. Trails live for 0.4 simulation seconds and the
attached pulses cycle at 4 Hz. There are no extra bodies, colliders, queries,
lights, textures, or gameplay-RNG calls. Only the small effect state is added
to a ship; the flight controller's force and angular-velocity calculations are
unchanged. The minimaps do not draw these effects.

## Repeatable pictures

From the repository root:

```sh
SPACEWARS_THRUSTER_ARTIFACTS=target/ship-thruster-captures \
  cargo test --locked -p engine-client --bin engine-client thruster_visual_tests
```

Open `target/ship-thruster-captures/index.html` for the labelled gallery, or
`overview.svg` for a compact nine-state sheet. Relative artifact paths resolve
from the workspace root, regardless of Cargo's test working directory.

Cases: idle, low-speed forward, left/right turning, reverse-model coverage,
full braking, rotated braking, swept-wing cruise, and escape-pod thrust. Each
is captured at tick 36, using prescribed actuation and kinematics through the
production ship/effect renderer. These are deterministic **visual fixtures**,
not claims about a second physics model or the availability of new controls.

The gallery includes close-ups, actual desktop/Picade/HyperPixel/portrait
player-pane dimensions, and the widest combat camera at raster scales 1 and 2.
Every PNG comes from the real
software raster renderer. SVGs export the actual vector adapter's projected
paths; they are useful for inspection, but are not screenshots of Slint's GPU
backend. `manifest.json` records the actuation, camera, resolution, trail count
and changed-pixel count versus the identical unlit ship.

The tests run without a display or connected device. They check visible pixels
at each size, vector effect inclusion, and a full pulse cycle without complete
blink-out. Core tests separately drive the real Surface controller to verify
thrust, turn signs, rotated braking, gravity separation and speed-governor output:

```sh
cargo test --locked -p scenario-spacewars --lib thruster
```

Future visual changes should rerun the same gallery and tests. Real gamepad
feel and a moving full-match scene still benefit from manual playtesting.
