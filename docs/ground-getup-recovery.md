# Ground get-up recovery

This follows [pod landing retries](pod-landing-retries.md). The shared ground
navigation task is now `ground_navigation_v6`.

## Recorded failure

Two saved Pi seed-42/P1, duel, 3-second Mixed asteroid runs end with repeated
get-up attempts. Both the mirrored and unmirrored layouts have steady contact
with retained material. The character is recovering from a knockdown and its
get-up result is `Blocked`; there is no swept clearance along gravity-relative
up. Even larger proposed upright lifts intersect the terrain. These are actual
stair/overhang contacts, rather than gravity oscillation or an unsettled body.

Diagnostic baseline replays matched all 180 original per-second motion hashes
in each case. The original controller requests a fresh get-up once per second
but never moves sideways during this state, so the obstruction cannot clear.

## Behavior

The additive recovery observation now includes balance, get-up result, attempt
count, contact stability and at most two short crawl corridors. Each corridor
sweeps the real capsule shape for 0.6 units along the actual contact tangent and
checks three retained-floor samples. The query has a 0.025-unit contact-slop
offset. It permits separation from an existing contact; a static overlap test
would reject the very escape being measured. Sensors do not move bodies, flush
dirty queries, change terrain or authorize claims/boarding. Detached debris
cannot supply crawl footing.

After a blocked get-up, a settled recovering bot chooses an available corridor,
preferring its existing direction or the closer destination. It uses ordinary
horizontal input: the same reduced-speed movement a human already has while
recovering. It finishes at most one second of the measured short step while its
body turns against contacts. Support loss, dirty queries and a changed terrain
revision invalidate the retained step. Missing clearance cannot renew it.
Getting up still uses the normal fresh jump press. Active lifting pauses crawl
movement. The whole crawl episode is bounded by eight seconds and six units of
displacement, within the existing ninety-second ground task deadline.

Once balanced, the bot surveys a new route from its actual position. A nearby
waypoint above the actor can also trigger an ordinary jump after walking stops
making progress, even when its horizontal error is inside the usual dead zone.
The HUD distinguishes crawling clear from trying to stand; telemetry retains
the direction, short target and reposition count.

There is no change to spaceling strength, get-up lift, jetpack charge, shared
physics/gravity stepping, or the human capture and transfer rules. Historical
V1/V2 flight observations keep their existing semantics.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror false --mode duel --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --out /tmp/ground-getup-recovery
```

Repeat with `--mirror true`. The optional trace now includes physical posture
and writes when the balance or get-up result/count changes, in addition to its
previous once-per-second and label-change samples. Entries precede physics;
sensor/policy timings exclude trace serialization.

In the development Pi replays, the unmirrored character crawls at 147.82 seconds
and is balanced at 148.10; the mirrored character crawls at 125.42 and is balanced
at 125.82. The mirrored run reaches the enemy flag at about 150.52 seconds and
lowers it. Raising the replacement later loses valid flag support, a separate
remaining claim-footing problem. The unmirrored run resumes walking, jumping
and jetpack travel, reaching about 13 units from its flag by 180 seconds. Neither
run has completed its second capture-sortie departure by cutoff. These outcomes
establish recovery and resumed traversal, not a completed two-planet mission.

Artifacts, including the unchanged baseline traces and development replays:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ground-getup-recovery-20260909/
```
