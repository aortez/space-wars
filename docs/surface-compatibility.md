# Generated-surface compatibility lab

This measures the gap between the playable Surface Sortie fixtures and ordinary
Spacewars planets. Named experimental profiles can retune the fixture world;
the raw baseline is retained. This does **not** enable natural landing in
ordinary Spacewars, retune shared controllers, change bots, or replace its
legacy berth.

## Run it

```sh
# Four generated worlds, every planet, four sun-relative bearings.
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --seed 0 --seeds 4

# One reproducible case, with full measurements as JSONL.
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --seed 0 --planet 0 --bearing 0 --json

# Paired comparison: same seed/planet/bearing and identical probe criteria.
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --profile both --seed 0 --seeds 4

# Wider experimental sweep (diagnostic, including known failures).
cargo run --locked --release -p scenario-spacewars --example surface_compatibility -- \
  --profile surface-v1 --seed 0 --seeds 32 --json

# One-factor radius/gravity/spin checks in the controlled lab.
cargo test --locked --release -p scenario-spacewars surface_controller_envelope -- --nocapture

# Visual diagnostic: the same generated seed, planet 0, bearing 0.
cargo run -p engine-client -- --scenario surface-sortie-generated --seed 0

# Experimental profile: same case and controllers, explicit world changes.
cargo run -p engine-client -- --scenario surface-sortie-world --seed 0
```

The visual diagnostic is also controller-selectable in the launcher. Its title
says **GENERATED / UNTUNED** and identifies the case. It intentionally exposes
the known raw failures. The experimental alternative says **GENERATED / SURFACE
V1**; the original `surface-sortie` and `surface-sortie-orbit` remain available.
Existing controls, minimap, pause/restart, and renderers work.
Only planet 0 / bearing 0 is currently launcher-selectable; the headless API and
runner accept every generated planet and bearing. Restart uses the launch seed.

Planet indices are zero-based. Bearings are 0 away from the sun, 1 a quarter-turn
counterclockwise, 2 toward the sun, and 3 a quarter-turn clockwise. The matrix
defaults to the **raw** profile, one world, all its planets and all four bearings;
`--profile` accepts `raw`, `surface-v1`, or `both`. `--seeds` permits
1–32 worlds. Invalid arguments exit 2. Physical failures/blocked prerequisites
are **data**, not command errors or CI failures. `--json` emits only JSONL on
stdout; per-profile summaries go to stderr. Redirect stdout to retain a
comparison run. JSON uses `surface_v1` for the profile's enum value.

## Model and criteria

The **raw** fixture uses the default 1,200-unit-radius Spacewars world at 60 Hz.
Every generated planet and the sun retain their original mass, radius, orbit,
and spin. Surface V1's explicit changes are listed below. Asteroids/starfield
are disabled in both profiles, as are the ordinary capture,
combat input, rover-deployment and win-condition policies of the Surface Sortie
fixture. Its unused second ship is absent. Neighboring planets remain physical
and gravitational participants. This is a single-pilot test, not multi-player
integration or a population-performance benchmark.

Support, landing, boarding, outpost attachment, rendering and diagnostics use
the selected planet index without reordering the world. All terrain targets
advance once per step. Actors remain dynamic; there is no surface attachment,
frame transport, common-gravity subtraction, or extra per-tick gravity solve.
The generated ship starts with surface linear/angular velocity. The intact
terminal is placed about 20 surface units to the side, without scaling actors.

Each probe starts independently with unchanged controllers:

| Probe | Measured behavior | Passing criterion |
| --- | --- | --- |
| Landing | Approach from 18 units above the normal start, 5° tilt, 5 units/s descent and 3 sideways; up to 15 seconds | Reach the actual landed gate without damage |
| Idle | Stand for 10 seconds | Continuous support, no knockdown, under 1 unit maximum idle drift |
| Walk | Hold right for 3 seconds | Over 5 units positive planet-local arc displacement, at least 90% support, no knockdown |
| Jump | One fresh press, then neutral for 10 seconds | Exactly one jump, over 0.5 units rise, supported again, no knockdown |
| Takeoff | Hold thrust for 3 seconds | Live ship ends with over 10 units rear-foot clearance |

Except for the flown approach, prerequisites allow up to 10 seconds to settle
from the near-ground start. On-foot probes then use the real transfer action and
two seconds of neutral settling. An unavailable landing, rejected transfer, or
unbalanced/unsupported pilot produces **blocked**, with zero measured ticks and
no distance. These stages are not claimed as tested walking/jumping. Setup
metrics are retained separately, including support loss, knockdown and damage;
the final pilot report includes balance, speed, gravity and the last disturbance.

Measured counters reset after setup, not the physics or controller state.
Walk distance unwraps surface-relative angle across revolutions; jump distance
is peak radial rise above the initial capsule altitude. Takeoff reports maximum
foot clearance, but passing also requires final clearance. Clearing this probe
alone does not prove robust flight, safe return, or sufficient thrust in other
orientations; external gravity and planet motion can also separate a ship.

Reports include the surface gravity, spin/orbit rates, external field versus
prescribed center acceleration, and effective inward/lateral acceleration at
ship spawn. Units are world units/s² and rad/s, not pixels or per-tick deltas.
The ship's legacy gravity query point is offset from its Rapier origin; both
points are reported, and the diagnostic field is checked against the real
shared solver. Scenario observations are now version 6, with
`generated_case.profile`; compatibility reports have their own version 2 schema.
Old case identifiers without `profile` deserialize as raw, not Surface V1.

## Initial evidence (2026-09-06, Rust 1.89, x86_64)

Seeds 0–3 produce 18 planets and **72 planet/bearing cases**. Their sampled
radii span 18.0–142.2, own surface gravity 653.4–712.1, and spin -0.450 to +0.367.
The external-field versus prescribed-orbit acceleration mismatch spans
43.7–389.7. The lab's gravity is 18 and ship thrust acceleration is 45.

- All 72 flown approaches fail the damage-free landing criterion. Some do
  settle; reaching `LANDED` alone is not a safe-approach pass.
- Takeoff: 2 pass the clearance criterion, 63 fail, 7 cannot first land.
- All 72 cases block each on-foot probe: 7 lack the initial ship landing;
  the other 65 transfer but lack balanced support after settling.
- Both existing stationary and orbital presets pass the identical five probes.

The one-factor controlled checks help isolate the gap:

| Change from radius 60 / gravity 18 / spin 0.015 | Observed result |
| --- | --- |
| Radius 15 or 150 | All five probes pass |
| Gravity 6 or 36 | All five pass; jump rise about 5.78 / 0.84 units |
| Gravity 90 / 180 | Walking and standing pass; jump rise about 0.29 / 0.12, takeoff pinned, approaches take damage |
| Gravity 700 | Approaches take damage, on-foot prerequisites fail, takeoff pinned |
| Spin 0.08 | All five pass |
| Spin 0.25 | Approach fails; the near-ground-start probes still pass |
| Spin ±0.5 | Approach fails and the other probes cannot first land |

These are bounded probes, not exhaustive gravity/spin combinations, an input
autopilot, or a guarantee that a skilled pilot could not land. Tests protect
known-good controls, reproducibility, source preservation, selected-body support,
explicit blocked outcomes, and the demonstrated high-gravity launch failure.
No wall-clock thresholds or regenerated bot baselines are involved.

## Surface V1 experiment

This profile applies **once**, after generating the original world and before
constructing its physics bodies. It does not alter the random generator or
rescale the ship, capsule, landing feet, terminal, controller forces, or probes.

| Parameter | Surface V1 policy |
| --- | --- |
| Planet gravity | Set each mass for 18 units/s² at the standing capsule center, `0.99 × radius + capsule half-height` |
| Sun gravity | Set mass for 3 units/s² at the sun's radius-200 surface |
| Planet spin | Preserve sign and relative generated variety; cap at 0.04 rad/s and at centrifugal acceleration of 2% of own surface gravity |
| Planet orbit | Preserve radius, initial angle and direction; set angular speed to `sqrt(60 × GRAVITY × sun.mass / orbit_radius³)` |
| Flight clearance | Enlarge world radius from 1,200 to 1,400, translating the complete sun/planet layout by (200, 200) to the new center; relative layout and body sizes stay intact |

The spin formula is `raw_spin / (π/6) × min(0.04, sqrt(0.02 × 18 /
standing_radius))`. These are versioned fixture constants in
`surface_sortie/profiles.rs`, not per-player gravity or global defaults.

Circular paths now match the **sun's** field at each planet center. Other
planets still contribute their full real fields to the actors. Planets remain
kinematic; their paths do not follow mutual N-body gravity, so external-frame
mismatch remains measurable. There is no attachment, common-field subtraction,
invisible launch impulse, or extra gravity solve. Free-flight regression tests
check that both actors coast under the actual shared field after separation.

The flight margin addresses a separate generated-geometry problem: seed 13,
planet 4 has only about 2.5 units between its outward edge and the old wall.
That does not fit the ship, let alone an approach. The raw case stays unchanged;
the expanded profile case passes all five probes.

### Comparison (2026-09-06, Rust 1.89, x86_64)

| Matrix | Raw | Surface V1 |
| --- | --- | --- |
| Seeds 0–3: 72 cases / 360 probes | 2 pass, 135 fail, 223 blocked | 360 pass, 0 fail, 0 blocked |
| Seeds 0–31: 620 cases / 3,100 probes | Not run as a paired 32-seed baseline | 3,093 pass, 7 fail, 0 blocked |

The wider profile matrix passes **620/620** of each idle, walk, jump, and
takeoff probe. Passive approaches pass **613/620**. Failures are reproducible at
`seed/planet/bearing`: `11/0/0`, `11/1/0`, `17/1/0`, `23/1/0`, `30/3/0`,
`30/4/2`, and `30/4/3`. Some miss the 15-second settling deadline; others
separate or end nose-inverted. None is silently omitted or assigned different
pass criteria. These probes are not skilled piloting or an autopilot.

The full disembark/walk/capture/repair/return/reboard/depart loop passes on
`0/0/0`, `0/3/2`, `1/2/1`, and `2/5/3`. These same cases also run 200-second
idle diagnostics after transfer setup. Three retain continuous support;
`1/2/1` is supported for 11,918/12,000 ticks, with nine gaps, the longest
18 ticks (0.3 seconds). No case suffers a knockdown, ship damage, or complete
ship-support loss. The difficult case temporarily loses the landed gate,
drifts up to 2.58 units during an uninterrupted idle interval, and reaches
4.30 units from the hatch—outside the 3-unit boarding range. It is not a
promise of indefinitely hands-off boarding or repair eligibility.

The extended regression deliberately checks a different contract from the
10-second probe: at least 99% support, no gap over 0.5 seconds, within 5 units
of the hatch, stable body count, and no damage/knockdown/complete ship-support
loss. The original short probes and their strict criteria are unchanged.
Tests also protect raw world preservation, profile scope, replay, and the known
inverted-approach case so future controller changes can measure its resolution.

```sh
cargo test --locked --release -p scenario-spacewars surface_v1 -- --nocapture
```

This is ready for **experimental playtesting**, not ordinary-game rollout.
Residual mutual-field mismatch, arbitrary approaches, long-idle drift, and
longer planetary trajectories need continued evaluation. No Pi deployment or
physical-controller acceptance has been performed for Surface V1 yet.

## Remaining path

Playtest this named profile and agree acceptable contact/approach limits before
adopting a world policy in ordinary Spacewars. Keep the raw comparison and
uncompensated free-flight behavior available during that decision.

After agreeing that policy: generalize to multiple pilots/vehicles, decide loss
and rescue/rebuild rules, connect contested services, and add versioned bot
surface intents. Economy and deformable terrain remain separate work.
