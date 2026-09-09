# Pod recovery after failed landings

This follows the [controlled two-planet missions](two-planet-missions.md).
The shared recovery task is now `recover_ship_v6`. This slice addresses failed
pod approaches; ground navigation, equipment and material interaction rules
retain their existing behavior.

## Failure and correction

The saved desktop seed-42/P1, unmirrored, interceptor plus 3-second Mixed asteroid
case loses its ship around 100 seconds. Its first approach becomes wedged and
exhausts the ten-second progress timer at about 131 seconds. The retry then uses
a separate climb routine, which cannot trigger the pod's physical righting lift.
It remains tipped against the ground until another asteroid moves it around
171 seconds.

After stabilizing again, the only surveyed landing candidate is one the task
previously rejected. That rejection lasts for the whole task, so the bot reports
"no suitable pod landing site" around 174 seconds. By 180 seconds the actual pod
is landed with a ready exit, but the task is already permanently blocked.

Retries now return through the shared stabilization/righting routine. They do
not reset the overall recovery clock. A rejected site is deferred for fifteen
seconds, or reconsidered sooner when its measured local footing changes by at
least half a unit. It must still pass the ordinary pod-foot, hull and hatch
survey before selection.

An empty or fully deferred survey allows up to fifteen seconds for new footing,
an expiring deferral or an actual landing. The bot brakes and aligns with local
up during that wait. Actual landed state and ready hatch access take precedence
over the pending survey or retry count. A blocked hatch still uses the existing
two-second exit wait; four failed attempts remain terminal. The two-minute
recovery deadline and existing one-time hostile-ground extension are unchanged.

Telemetry records each rejected site, local footing, terrain revision, rejection
tick and eligibility time, together with the start of an active site search.
The HUD distinguishes checking another pod landing site from active descent.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror false --mode intercept --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --out /tmp/pod-landing-retries
```

`--trace true` writes optional `trace.jsonl` observations, encoded ordinary
actions and mission telemetry once per second and when a bot's label changes.
Entries precede the host's physics step. Sensor and policy timings exclude this
diagnostic serialization. The normal compact report and physical audit remain
available with tracing disabled.

The saved case now lands at about 143 seconds, claims, rebuilds and boards its
replacement, resuming the mission around 155 seconds. It has not completed two
capture-sortie departures by the three-minute cutoff. Further validation and
device evidence are recorded below.

Artifacts are retained at:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/landing-route-reliability-20260909/
```
