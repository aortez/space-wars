# Reachable replacement ships

Material recovery checks a short route to the replacement hatch before building.
Humans and bots use the same placement rules, eight-second timer, ownership and
support gates. The bot can walk/jump to a measured rebuilding spot when none of
the nearby placements has usable access. Boarding still requires real supported,
balanced contact beside a settled ship.

## Placement and movement

Four local offsets remain available: -8, -14, +8 and +14 world units along the
standing support's tangent. Both foot rays predict a resting pose on surviving
material. The placement check includes hull clearance, other seats, hatch floor,
and a measured walk/jump route of at most 24 units. It prefers a shorter route
with fewer jumps. The old pod is excluded only in the replacement preview,
because construction removes that assembly from the same vehicle slot.

The preview uses the real ship hull/feet and spaceling capsule geometry. It
queries the completed world and intersects proposed shapes without inserting
bodies or taking a speculative physics step. It checks the raised spawn pose
and the expected resting pose; later impacts and settling can still change the
world. Recovery repeats the checks at the actual build tick.

The route ends where the supported spaceling can board under the existing
three-unit transfer rule, with a 0.2-unit planning margin. This matters in a
small crater: the actor may already be close enough to board without climbing
onto the exact point found by the hatch ray. The physical boarding range did
not change.

If placement fails, the recovery task surveys up to eight nearby standing sites.
A site needs both a current walk/jump route and a prospective replacement with
hatch access. The bot follows the shared ground task using ordinary movement,
then waits through normal construction. Terrain revisions invalidate the
chosen site. Four relocations, a five-second search window, existing movement
stall checks, and the original recovery deadline bound failure. Replanning does
not replenish the recovery budget. These are local outer-surface routes, not
cave navigation or an AI mining escape.

Full ground surveys remain staggered between seats at 2 Hz. Rebuild relocation
surveys run 15 ticks between that seat's full surveys, using a local patch of
at most 113 bearings. The replacement check also uses at most 113 bearings,
with the existing six-neighbor walk/jump bounds. This avoids adding both
search costs to a single observation frame.

`ground_navigation_v2` and `recover_ship_v3` identify the updated tasks.
Historical ordinary-match policies and frozen flight observations are retained.

## Diagnostics and reproductions

Ground telemetry distinguishes missing start footing, missing destination
footing and disconnected measured ground. It records nearest-footing distances,
reachable-node counts, route length and jump count. Survey rejections identify
absent retained floor, excessive slope, capsule obstruction, or a proposed ship
obstructing a node. These describe the planner's measurements; they do not prove
that a human could not traverse the terrain.

Recovery observations retain the latest placement attempts and their rejection
reasons. Relocation surveys record the tested bearing, current route and
replacement preview. Both soak runners retain ground maps at terminal route
failures, so the actor, hatch/flag and graph can be inspected together.

Run the twenty-case, three-minute matrix:

```sh
cargo build --locked --release -p spacewars-ai --example surface_flag_soak
python3 tools/run-ground-navigation-trials.py \
  --binary target/release/examples/surface_flag_soak --out /tmp/rebuild-trials
```

Use `$CARGO_TARGET_DIR/release/examples/surface_flag_soak` when a shared target
directory is configured. The script supports the existing SSH runner options.
Every case requires completion except the two intentionally disconnected route
fixtures. The four `--edit rebuild` cases cut a radius-four crater beside the
pilot during construction, and require an actual relocation as well as departure.
They cover both occupied-ship/pod loss and empty-ship loss, in both seats.

The deployed predecessor's P1 pod-return failure was reproduced before editing:
claim at tick 1948, then a hatch-route deadline with no departure. The corrected
run boards the replacement and departs within three minutes without extending
any deadline. The original report and exploratory failures remain archived.

Artifacts for this checkpoint:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/rebuild-access-20260908/`.

## Controlled validation

Twenty 180-second cases ran on each platform. Both desktop and Raspberry Pi
completed 18 missions and explicitly rejected the two deliberately disconnected
routes. All 40 physics/material audits pass. The four new relocation cases per
platform each require one measured relocation and complete the full departure.
The longest new case takes about 141 simulated seconds on Pi. The old failing
P1 pod case now departs at tick 2814 (46.9 seconds).

| Serial headless timing, range across cases | Desktop | Pi |
| --- | ---: | ---: |
| Ground survey P95 | 5.22–5.84 ms | 13.32–15.03 ms |
| Largest ground survey | 6.36 ms | 15.19 ms |
| Relocation survey P95 / max (one per new case) | 2.63–3.62 ms | 7.23–9.21 ms |
| Shared step P95 | 0.048–0.054 ms | 0.141–0.162 ms |
| Largest shared step, including rebuild validation | 3.01 ms | 7.75 ms |

The earlier combined-survey implementation peaked at 20.21 ms on Pi. Those
reports are retained under `combined-surveys-*`; the final matrices use the
alternating schedule and report both survey categories separately. Headless
measurements exclude rendering and do not establish multi-planet capacity.

One Pi pod/relocation departure differs by 150 ticks from desktop; both finish
and pass audits. The matrix does not assert cross-platform trajectory identity.

All 1,041 workspace/all-target tests pass; 21 display-dependent tests are ignored
by that command. The recovery, combat and duel UI workflows were then run
explicitly under Xvfb and pass launch, pause, restart and both renderers.
Navigation V1's six frozen episodes and Strategy V1's twelve episodes match.
Clippy completes with inherited repository warnings; the new code's redundant
cast was removed. Formatting and whitespace checks pass.

The earlier Pi live screenshot's blocked route was investigated with a separate
copy of `0dc5d67`, changed only to record failure maps in its runner. With seed 42,
Capture P1, interceptor P2 and 8s/4s breaks, both the old and new headless Pi
replays instead have P2 walking toward the enemy flag at 180 seconds. They pass
physics audits but do not finish that recovery. Thus this replay does not
establish the cause or resolution of the earlier live screenshot. Detailed maps
are now retained when a terminal route failure does occur.

Remaining limits include disconnected ground, paths the outer-contour
survey misses, later obstacles after placement, pinned/sideways pods, and the
separate severe-impact speed alarm documented in the prior ground-navigation
report. This slice does not claim unrestricted combat recovery, random-asteroid
endurance, cave escape, or generated multi-planet integration.

## Pi deployment and live verification

Gameplay commit `6a7f29b` was built into the Pi image and deployed to
`spacewars.local`. All 6,608 Yocto tasks succeeded (21 rerun). The Pi rebooted
into slot A, `/dev/sda2`, with an active kiosk service and no service restarts.
The installed client hash matches the client extracted from the archived image:

```text
image SHA-256:  8da70b564b009b3501678a7b1857d1e975669830cf3d650704ec35ab10088255
client SHA-256: 24d4be79f1df19ed881e2f24e0a75f4db71abae8c8e1287d769d11a2a257dc03
```

A fresh seed-42 material duel ran for about three minutes, with Capture P1,
interceptor P2, 8s/4s breaks and raster scale 2 at the device's 800×480 output.
Six sampled status reports show 59.8–60.1 FPS and 59.1–60.1 UPS, with zero service
restarts. These are periodic rate samples, not a frame-time distribution.

The live run **reproduces the earlier flag-route failure**. P1 owns the planet;
P2 is still landing its escape pod at the 120-second capture. By 150 seconds it
has landed and exited, but reports `no measured walk/jump route to destination`.
That remains at the final capture around 180 seconds, with 58 material cells
removed. P2 has not countercaptured or reached rebuilding. Thus the controlled
rebuild results do not establish successful recovery in this combat situation.
The old/new headless replay discrepancy remains open. No ground map was exported
from this live client run, so the screenshots do not establish why planning
failed.

Screenshots and status logs are archived as `pi-live-030` through `pi-live-178`
in the artifact directory above. `image-manifest.json`,
`installed-verification.json`, and `validation-manifest.json` retain build,
installation and validation provenance.

After the observation, a fresh material combat round was restarted and paused
for controller playtesting: P1 human, P2 Capture, seed 42, breaks 8s/4s. The
paused state and screenshot are retained as `pi-ready-state.json` and
`pi-ready-paused.png`. Press Start/B or choose Resume to play.
