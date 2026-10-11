# Ship, pod and on-foot durability

Issue [#154](https://github.com/aortez/space-wars/issues/154) makes the full ship
the strongest form, the escape pod appreciably weaker, and the on-foot pilot
fragile. **Both vehicles fully shield their occupant.** Ship hull, pod hull and
persistent pilot health are separate: damage never reaches an occupied pilot.

## Rules

The full ship retains its existing hull health, weapons, collision damage and
heat rules. Ship destruction ejects a living occupant into a fresh 100-point
pod hull and starts the existing 180-tick protection window. The killing shot
cannot also damage the new pod. A destroyed unoccupied ship creates no pod.

A match pod's hull takes twice the full ship's raw laser damage and loses 50
health to a direct missile. Above the unchanged safe closing speed of 12 units/s,
a fresh impact removes two hull health per excess unit/s, capped at 100.
The pilot inside takes **zero** damage from lasers, missiles, impacts and heat,
including the hit that destroys the pod. After all damage in the shared step,
a broken pod releases the same spaceling at the cockpit, preserving pilot
health, jetpack charge, velocity and spin. It does not add another protection
window. Subsequent on-foot hits can hurt the pilot.

The on-foot pilot retains a persistent 100-point health pool and takes four
times the full ship's raw laser damage. A direct missile removes 100 health;
impacts above 12 units/s remove four health per excess unit/s, capped at 100.
Resting contacts and ordinary walking remain harmless. Zero-damage wreckage
retains its physical shove without direct health damage.

The solar corona retains its 24-unit width and linear falloff. At maximum
exposure, it removes 20% hull/s from a full ship, 40 hull health/s from a pod,
and 80 pilot health/s on foot. Pod-hull and on-foot damage respect the existing
ejection protection deadline.

Boarding and exiting repair neither pool and never extend protection. Empty
pods remain destructible. Rebuilding replaces the vehicle with a full ship;
it does not heal the pilot. A later full-ship loss supplies a new pod, but
boarding or exiting cannot refresh the current pod's hull.

The HUD shows **Hull**, **Pod hull**, or **Pilot** for the active form. In a pod
it also shows the occupant's preserved health. Combat sensors report pod hull
health instead of the occupant's health; no controller policy is changed.
Historical labs without match rules retain invulnerable survivor forms.

## Controlled measurements

Baseline: `a483fa41cf85e351436b2d10314b715a2077c60a`. Both versions use seed 42,
100 hull health, 100 pilot health, a 60 Hz step, stationary gravity sources
with zero mass, and real laser rays and Rapier contacts. The table below starts
without ejection protection. Ship loss means losing the hull and ejecting;
in the baseline, pod/on-foot loss means pilot death. In this version, pod loss
means destruction of its hull with a living pilot released; only on-foot loss
means pilot death. These are not interchangeable round endings. Every tuned
ship and pod exposure below leaves the pilot at 100 health.

| Exposure | Ship before → after | Pod before → after | On foot before → after |
| --- | --- | --- | --- |
| Continuous laser, 40-unit pivot separation: seconds to loss | 13.88 → 13.88 | 14.38 → 7.18 | 14.50 → 3.62 |
| Half-corona exposure: seconds to loss | 10.02 → 10.02 | 10.02 → 5.00 | 10.02 → 2.52 |
| First supplied missile contact, 300 units/s: health lost | 28.96 → 28.96 | 40 → 50 | 40 → 100 |
| Incoming missiles every 2s: seconds to loss | 4.03 → 4.03 | 2.38 → 2.05 | 1.33 → 0.05 |
| Planet impact, 40 units/s incident speed: health lost | 0.395 → 0.395 | 100 → 55 | 100 → 100 |

The missile sequence schedules real one-mass-unit supplied shells eight units
behind the target, at 300 units/s relative speed, every two seconds. Target
motion and knockback remain free; the abandoned on-foot pilot's ship is moved
out of the knockback path during initialization. This isolates incoming
exposure, not shooter accuracy or reload decisions. Times start at the first
launch, including travel to contact. The ship takes three contacts in both
builds; its impulse-based damage varies as it moves and rotates. The old pod
dies in a world-boundary collision after two missile hits; the old on-foot
pilot dies in a boundary collision after one. The tuned pod hull breaks on its
second missile without hurting its occupant; the on-foot pilot dies from the
first. Pilot damage records retain the final cause: a knockback death is not
counted as direct missile damage.

For planet impacts, the fixture maintains the incident speed until first
contact, then records actual closing speed. Native flight drag makes the
ship/pod close at 39.5 units/s versus 40 on foot. At incident speeds 10, 20 and
30, tuned pod damage is respectively 0, 15 and 35; on-foot damage is 0, 32 and
72. Full ships take no damage at those measured speeds. This keeps a pod's
landing cushion while making it less durable than a full hull.

Protected laser and heat cases are recorded separately. They first lose
hull or pilot health at tick 180. The native supplied-missile braking test also
confirms that a fresh pod retains 50 hull health and 100 pilot health, and can
brake below 2 units/s after the hit. Physical transfer tests preserve both a
wounded pod hull and its wounded pilot; rebuilding preserves pilot health.
Native lethal laser and missile tests verify that pod destruction releases one
pilot without damage spillover, healing, or another protection period.

## Paired bot matches

The matrix freezes two seeds before inspecting results:
`6365347488789871033` and `7248310457775985154`. Each runs Weak Round and Weak
Angular dirt with slumping, ordinary Mixed asteroid arrivals every 8s, and
heavy Mixed arrivals every 3s. Each world is repeated with Recovery bot v1
and Value bot v13 in swapped seats. Both retain `material_mission_v13` combat
policy and 15s/4s combat breaks. The recovery seat explicitly selects
`terrain_recovery_supported_hatch_ordinary_offsets_v1` from tick zero.

There are 16 runs per build, 32 total, each ending at native pilot death or
the ten-minute match limit. No reset, control override or terrain edit is
applied during these matches. Per-tick work bounds, material conservation,
finite motion and physical speed checks pass in every run.

| Match group (8 runs per build) | Before | After |
| --- | --- | --- |
| Ordinary: deaths / ship losses / physical rebuilds / completed recovery tasks | 6 / 10 / 4 / 4 | 6 / 10 / 4 / 4 |
| Heavy: deaths / ship losses / physical rebuilds / completed recovery tasks | 4 / 13 / 6 / 8 | 6 / 15 / 6 / 8 |
| Ordinary: median round duration | 207.09s | 211.08s |
| Heavy: median round duration | 439.19s | 258.23s |
| Ordinary: P1 wins / P2 wins | 4 / 4 | 4 / 4 |
| Heavy: P1 wins / P2 wins | 6 / 2 | 6 / 2 |

Every pair retains its winner, physical rebuild counts and completed recovery
counts. The revised version records four pod losses in ordinary matches and
five in heavy matches. All twelve pilot deaths occur on foot after physical
impacts; no pilot dies aboard a ship or pod.

Two heavy Angular cases that previously timed out now end in an on-foot impact
death at 235.30s, with the same winner. Across the matrix, timeout endings fall
from six to four. Round durations include timeouts and are distinct from the
controlled hull-loss and pilot-death measurements above. Recovery-task
completions are reported separately from physical rebuilds.

The former Angular impact deaths 3.12s after ship ejection now occur 27.13s
after ejection; those pilots survive the pod-breaking collision and die from a
later on-foot impact. The Round case still ends soon after protection expires:
pod destruction is followed by a lethal on-foot impact two ticks later. Full
shielding does not guarantee that a high-speed trajectory becomes survivable
once the hull is gone.

This small paired matrix does not establish overall win-rate balance or solve
the known blocked-route cases. Aggression and pursuit changes remain under
[#155](https://github.com/aortez/space-wars/issues/155).

## Evidence and reproduction

[Machine-readable results](data/durability-154-v2.json) include every case,
health/protection settings, separate hull-loss/death meanings, pod losses,
causes, physical rebuilds, recovery completions, outcomes and executable hashes. Full commands, action
traces, snapshots and the retained baseline/tuned executables are under
`target/durability-154/`. These desktop measurements use Rust 1.94.1 on the
same machine; they are not Pi performance measurements. No device deployment
or automatic Clock settings change is part of this work.

Run the native exposure probe with an absolute, new output path:

```sh
RUST_MIN_STACK=16777216 SPACEWARS_DURABILITY_OUT=/tmp/durability-new.json \
  cargo test --locked --profile ci -p scenario-spacewars \
  measure_native_durability -- --ignored --nocapture
```

For the baseline, use a detached checkout of `a483fa4`. Copy the exposure probe
from commit `69a745e` (`tests/match_tests/durability.rs`) and add its
`mod durability;` declaration, then run only the ignored measurement. That
retained probe reads pilot health for the old pod model; the current probe
reads pod hull health. The non-ignored tier assertions describe tuned behavior
and are not baseline assertions.

Build and retain an AI test executable for each version before editing damage
rules; Cargo prints its path with `--no-run`:

```sh
cargo test --locked --profile ci -p spacewars-ai --features sensor-profile \
  --lib --no-run
python3 tools/run-durability-matches.py /tmp/durability-matches-new \
  --binary /absolute/path/to/retained-ai-test-executable --jobs 2
```

The baseline executable also needs the diagnostic-only allowance for 3s
arrivals in `qualification/heldout.rs`. The runner rejects an existing output
directory and records its binary, matrix and runner hashes. All 16 cases are
retained, including failures; no outcome determines seed selection.

Focused tests cover native tier ordering, full occupant shielding, lethal pod
hits without spillover, one surviving spaceling with preserved motion and
jetpack charge, destructible empty pods, wounded hull/health persistence across
physical transfers, safe and lethal impacts, bounded protection, rebuilding
and finished rounds. The 609 scenario tests, 456 AI tests, 45 ground-navigation
and 51 surface-recovery integration tests, and all 687 client tests pass.
