# Scorched Earth

Choose **scorched-earth** in the launcher, or launch directly:

```sh
cargo run --locked --release -p engine-client -- --scenario scorched-earth --seed 42
```

This first playable slice of [#25](https://github.com/aortez/space-wars/issues/25)
puts two physical tanks on seeded hills with downward gravity. Aim and fire at
the opponent, or undermine its footing. The other tank returns fire every five
seconds. The first tank reduced to zero health loses; **Reset** starts again on
the same hills. A simultaneous destruction is a draw.

## Controls

| Action | Keyboard | Controller |
| --- | --- | --- |
| Lower / raise elevation | Left / Right, A / D | D-pad left / right, right-stick X |
| Lower / raise power | Down / Up, S / W | D-pad down / up, right-stick Y |
| Fire (hold to repeat) | Space | Bottom face, left shoulder, right trigger |
| Take control of the other tank | Tab | Left face |
| Toggle the scripted CPU duel | V | Right shoulder |
| Switch Angular / Round dirt and reset | T | Top face |
| Restart with defaults and the same seed | R | Pause → Restart |
| Pause | Esc | Start |

The on-screen controls also adjust aim/power, fire, select a tank, toggle Demo,
switch dirt, and reset. The on-screen **Reset** retains the current dirt shape
and Demo choice. Camera framing fits the entire battlefield in landscape and
narrow windows. The host provides the normal launcher, pause, and renderer controls.

**Angular** is the initial dirt choice. **Demo** drives both tanks with the same
seeded aiming sweep used by the headless runner. It estimates ballistic range
but varies its aim; intervening terrain and loose grains can intercept shots.
Power is muzzle speed (12–48 world units/s); elevation ranges from 5° to 85°.
Human shots have a 1.25-second cooldown. The tanks currently cannot drive.

## Shared mechanics

The scenario owns hills, gravity, tanks, shell trajectories, health and controls.
It calls the existing [shared loose-terrain layer](shared-loose-terrain.md):

1. A shell sweeps through the canonical physics world each fixed update, so
   fast shots collide with terrain, tanks and loose grains.
2. At impact, `PreparedRelease` plans the affected fields before changing them.
   Admission covers the complete event. `LooseTerrain::commit` transfers removed
   cells into conserved grains and detached material fields.
3. `RadialImpulse` throws nearby loose material and kicks tank hulls. Rapier
   supplies gravity and contact response in the same world as the terrain.
4. `LooseTerrain::settle` returns eligible quiet groups to existing terrain
   through the ordinary deposit boundary. Published geometry and collision
   shapes update together, with actor clearance checked by the shared layer.

There is no second soil solver or Scorched-specific deposition algorithm. New
improvements to the shared layer can benefit this scene, the lab and Spacewars.
The flat scene is also a small consumer to exercise before adding the Clock
event in [#132](https://github.com/aortez/space-wars/issues/132).

The default pool is 192 grains; the headless runner accepts 1–512. Detached fields
are capped at 64. A release that cannot fit is rejected atomically and reported
on screen. Existing material is retained, including offscreen dirt. Combat damage
and tank knockback still apply, so a full pool does not make tanks invulnerable.

This is bounded rigid-grain behavior, not a calibrated soil model. Whole-cell
deposition can leave ledges, a settled pile may remain loose, and sustained fire
can exhaust the pool. Cohesion, slope relaxation, shock propagation, compression,
fluids and napalm remain later work. Tank traction/driving, weapons, sound and
broader round progression are also outside this first slice.

## Repeatable development

```sh
cargo test --locked --profile ci -p scenario-scorched-earth
cargo test --locked --profile ci -p engine-client --bin engine-client \
  client_scenarios::scorched_earth
cargo test --locked --profile ci -p scenario-spacewars deposited_flag_footing

cargo run --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape angular --seconds 30 --seed 42 --replay
cargo run --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape round --seconds 30 --seed 42 --replay

SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 xvfb-run -a \
  cargo test --locked --profile ci -p engine-client --test ui_control_functional \
  scorched_earth_launch_pause_restart_and_both_renderers -- --ignored --nocapture
```

The duel runner emits JSON with whole-step mean/P95/maximum time, body/contact
and grain peaks, shots, impacts, rejected releases, deposition and material
balance. Audits, observation hashing, replay and rendering are outside the timer.
`--replay` repeats the full seeded run and compares observation hashes every
second. The result describes this workload; it is not a rendered FPS claim or a
guarantee of cross-platform physics lockstep. After a tank loses, existing shells
and material continue simulating but the tanks stop firing; a longer duration
therefore measures the same duel followed by settling, not endless combat.

Simulation tests cover both grain shapes, repeated release/deposition, clone
continuation, capacity rejection, loss of tank support, fast swept shells, reset,
bad input and the full `u64` seed range. Client tests cover keyboard/controller
parity, disconnection, narrow-window framing and both render paths. The display
test launches, fires, verifies visible displaced dirt, pauses, restarts and
returns to the launcher on both renderers.

The accompanying Spacewars regression completes the flag lifecycle from #51:
claim a deposited footing, blast it away, observe neutralization, let material
return, then require a fresh full claim. Returning dirt never restores ownership
by itself. Both Round and Angular exercise that path.
