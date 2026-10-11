# Ship, pod and on-foot durability

Issue [#154](https://github.com/aortez/space-wars/issues/154) makes the full ship
the strongest form, the escape pod appreciably weaker, and the on-foot pilot
fragile. The change uses **per-form damage**, retaining one persistent
100-point pilot-health pool. There is no additional pod armor meter to refill
by boarding, exiting or rebuilding.

## Rules

The full ship retains its existing hull health, weapons, collision damage and
heat rules. Its **Hull** meter shields the occupant. Ship destruction ejects a
living occupant and starts the existing 180-tick protection window. The killing
shot cannot also damage the new pod. Protection never heals the pilot, and
boarding or exiting does not extend it.

The pod and on-foot **Pilot** meter shows the same remaining health. Compared
with a full ship's raw laser hit, pod damage is doubled and on-foot damage is
quadrupled. A direct missile removes 50 pilot health in a pod or 100 on foot.
Above the unchanged safe closing speed of 12 units/s, a fresh impact removes
two health per excess unit/s in a pod or four on foot, capped at 100. Resting
contacts and ordinary walking remain harmless. Zero-damage wreckage retains
its physical shove without direct pilot damage.

The solar corona retains its 24-unit width and linear falloff. At maximum
exposure, it removes 20% hull/s from a full ship, 40 pilot health/s in a pod,
and 80 on foot. All exposed damage respects ejection protection.

The HUD continues to distinguish **Hull** from **Pilot**, labels pod protection
as light, and shows the remaining ejection protection. Match help explains the
durability hierarchy and persistent health. Historical labs without match
rules retain invulnerable survivor forms.

## Controlled measurements

Baseline: `a483fa41cf85e351436b2d10314b715a2077c60a`. Both versions use seed 42,
100 hull health, 100 pilot health, a 60 Hz step, stationary gravity sources
with zero mass, and real laser rays and Rapier contacts. The table below starts
without ejection protection. Ship loss means losing the hull and ejecting;
pod/on-foot loss means pilot death. These are not interchangeable round endings.

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
pilot dies in a boundary collision after one. The tuned pod dies from its
second missile and the tuned on-foot pilot from the first. The raw records
retain the final cause: a knockback death is not counted as direct missile
damage.

For planet impacts, the fixture maintains the incident speed until first
contact, then records actual closing speed. Native flight drag makes the
ship/pod close at 39.5 units/s versus 40 on foot. At incident speeds 10, 20 and
30, tuned pod damage is respectively 0, 15 and 35; on-foot damage is 0, 32 and
72. Full ships take no damage at those measured speeds. This keeps a pod's
landing cushion while making it less durable than a full hull.

Protected laser and heat cases are recorded separately. They first lose
health at tick 180; the same persistent pool then drains at its form's rate.
The native supplied-missile braking test also confirms that a fresh pod keeps
50 health and can brake below 2 units/s after the hit. Existing physical
transfer and rebuilding tests preserve wounded pilot health throughout.

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
| Heavy: deaths / ship losses / physical rebuilds / completed recovery tasks | 4 / 13 / 6 / 8 | 4 / 13 / 6 / 8 |
| Ordinary: median round duration | 207.09s | 206.77s |
| Heavy: median round duration | 439.19s | 438.56s |
| Ordinary: P1 wins / P2 wins | 4 / 4 | 4 / 4 |
| Heavy: P1 wins / P2 wins | 6 / 2 | 6 / 2 |

Every pair retains its winner and recovery counts. Six runs in each build
reach the time limit; round duration includes those timeouts and is distinct
from the controlled time-to-kill measurements above. Recovery-task completions
are reported separately from physical rebuilds.

Four defeats become missile kills: two ordinary cases finish 0.65s earlier,
and two heavy cases finish 1.27s earlier. Previously those same pilots died
from a later impact or laser hit. Total lethal causes change from eight
impacts and two lasers to six impacts and four missiles.

The existing violent-impact deaths 3.12s and 3.82s after ejection remain. A
three-second grace period is an opportunity to recover, not a guarantee that
every trajectory is survivable. The tested recovery completions are retained;
the small, paired matrix does not establish overall win-rate balance or solve
the known blocked-route cases. Aggression and pursuit changes remain under
[#155](https://github.com/aortez/space-wars/issues/155).

## Evidence and reproduction

[Machine-readable results](data/durability-154-v1.json) include every case,
health/protection settings, loss/death ticks, causes, physical rebuilds,
recovery completions, outcomes and executable hashes. Full commands, action
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

For the baseline, use a detached checkout of `a483fa4`, copy only the
`tests/match_tests/durability.rs` probe and its `mod durability;` declaration,
and run the ignored measurement. Its non-ignored tier assertions deliberately
describe the new contract and are not baseline assertions.

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

Focused tests cover native tier ordering, direct missiles, safe and lethal
impacts, persistent wounded health, harmless surface movement, bounded ejection
protection, one killing shot per form transition, material occlusion, braking,
rebuilding and finished rounds. Scenario and AI suites, the 45 ground-navigation
and 51 surface-recovery integration tests, and all 687 client tests pass.
