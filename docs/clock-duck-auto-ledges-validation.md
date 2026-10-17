# Automatic duck ledge recovery

Implements #283 on top of #281 / PR #479. Careful and Flowing use the same
physical catch, hang, climb and release commands. Player bindings remain #282;
the occasional deliberate flap detour and direct-jump follow-up remain #478.

## Decisions and limits

The controller keeps an ordinary jump when its observed vertical motion and
measured gravity still allow it to clear the top. Otherwise, a nearby eligible
edge can become a recovery target. It must face the current route (toward the
exit once departure is available) and match the planned landing or selected
recovery surface when one exists. Local recovery can also use a fully open flap.
The nearest qualifying edge wins; inventory order breaks equal-distance ties.
The shared actuator validates reach and clearance for that exact edge. Refusal
cannot silently select an unrelated ledge.

Calibration and water recovery never request catches. Only a confirmed physical
attachment discards the old airborne plan. Learned capabilities remain intact.
The duck hangs visibly for 18 simulation ticks (0.3 seconds), then requests a
pull-up. A blocked climb releases on the next tick. The entire attempt has a
180-tick (3-second) limit, followed by a 60-tick cooldown. At most three attempts
are allowed per visit. These are engineering starting values.

No new attempt starts without the full attempt budget remaining in the ordinary
visit envelope. Water, disruption, invalid support, the deadline, or an attachment
facing away from a newly available exit can cancel the attempt. Takeover stops
automatic commands immediately while retaining the actual body and any committed
attachment or climb. Shared arena retirement clears all attachment state.

Selection examines at most 16 existing edges per tick, without trajectory search
or additional physics queries. The shared movement layer retains its 64-query
ceiling. The body, gravity, mass, jump strength, course RNG and event schedule
are unchanged. Climb successes have separate counters from ordinary landings.

Additive `duck.navigation.ledge` telemetry records target, active state, attempt
and hang ticks, cooldown, bounded candidate checks, attempts/completions/failures
and the latest outcome. The existing `duck.ledge` remains the physical state;
old payloads may omit the new navigation field.

## Local evidence

371 Clock tests, 53 common/control tests, and 74 matching client tests passed.
Core Clippy with warnings denied, formatting and diff checks passed. The ten
new recovery tests were rerun after the final takeover and exit coverage edits.

The regression suite covers both profiles, mirrored entrances and 800×480,
1024×768 and 480×800 layouts. Clean seed-42 visits complete without ledge
attempts. Controlled near-miss fixtures catch, hang, climb, resume navigation and
exit normally; calibration samples, body mass and jump count remain unchanged
during the recovery. Additional fixtures cover wrong-way/clearable targets,
blocked landings and climbs, cooldown and attempt limits, actual water immersion,
support loss, deadlines, takeover, open-flap support and shared arena retirement.

**Capture setup:** each fixture performs real automatic warm-up and takeoff,
then deliberately places the airborne body beside its selected edge with a
near-miss velocity. This makes the recovery reproducible; it is not evidence
that ordinary unperturbed jumps routinely fail. From there, automatic commands
alone catch, hang and climb. No test supplies catch/climb commands or a stronger
jump. The captures show this controlled recovery, not the future flap detour.

The full Clock scene produces 36 frames (three stages × two profiles × two
directions × three viewports). Each sequence replays twice with identical frames
and diagnostics; zero-duration pause steps preserve both, and resize releases
the scene. Raster and vector fixture exports use production render inputs.
Vector captures use Slint's SVG loader plus native text because the headless
software fixture cannot draw native Path items. These are local captures, not
device GPU or deployment evidence.

## Reproduction

```sh
cargo test --offline --locked --profile ci -p scenario-clock -p engine-common -p spacewars-control
cargo clippy --offline --locked --profile ci -p scenario-clock -p engine-common -p spacewars-control --all-targets --no-deps -- -D warnings

SPACEWARS_DUCK_AUTO_LEDGE_FRAMES=/tmp/duck-auto-ledge-frames.json \
  cargo test --offline --locked --profile ci -p scenario-clock \
  automatic_ledge_scene_fixtures_replay_pause_and_render_at_target_sizes
SPACEWARS_DUCK_AUTO_LEDGE_FRAMES=/tmp/duck-auto-ledge-frames.json \
SPACEWARS_DUCK_AUTO_LEDGE_ARTIFACTS=/tmp/duck-auto-ledge-captures \
  cargo test --offline --locked --profile ci -p engine-client \
  ui_render_tests::duck_flaps::automatic_ledge_fixtures_render_through_both_adapters \
  -- --ignored --exact
```
