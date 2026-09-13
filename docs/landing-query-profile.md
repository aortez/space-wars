# Landing-survey CPU stalls on sw-picade

## Captured live cause

The earlier [rendering investigation](text-memory-profile.md) saved a 9.55 FPS /
47.74 UPS window with 82.94 ms of host step work per displayed frame. That bucket
combines several fixed simulation updates, both bots and physics. Its actual
random seed was not recorded, so that exact run cannot be reconstructed.

The new live diagnostics caught another sustained slowdown in generated match
`7725194555774358125`. At tick 16731, the cabinet reported **9.5 FPS / 47.5 UPS**:

| Stage | Rolling mean |
| --- | ---: |
| P1 sensors, per simulation update | 0.265 ms |
| P2 sensors, per simulation update | 14.937 ms |
| P2 policy, per simulation update | 0.963 ms |
| Shared scenario step, per simulation update | 0.629 ms |
| Raw physics within that step | 0.458 ms |
| All adapter work, per simulation update | 16.806 ms |
| Host step work, per displayed frame | 70.439 ms |
| KMS draw, per displayed frame | 13.138 ms |

The two windows have different denominators and durations. The host averaged
4.342 updates per callback; its timings should not be added to per-update
subdivisions. P2 was aboard a full ship, choosing sheltered ground with no
selected landing site. Its observation returned all 64 candidates every update.
P1 was travelling with a no-site request. Three consecutive samples had P2 sensor
means of 11.151, 14.338 and 14.937 ms while FPS remained around nine.

The first diagnostic build called the recovery candidate count `pod_sites`.
For a full ship, recovery simply copies the ship candidates; it does **not**
perform another 64 pod queries. The final code names this `recovery_sites`.

## Attribution and first change

An independently recovered generated seed, `9216675843324634618`, supplies a
repeatable headless Pi comparison. Its first three simulated minutes contain
107,530 landing-site calls and 370 full ground surveys. Detailed opt-in scopes
attribute 14.12 seconds to hatch checks, 4.14 seconds to hull placement and
0.72 seconds to constructing preview shapes. Ground graph connections cost
6.51 seconds, compared with 0.87 seconds for finding ground nodes.

The first optimization removes repeated queries within one landing candidate:

- The central and two sideways untilted hatch poses are evaluated once each.
  The settling-margin loop then checks the six remaining tilted poses.
- Consecutive clearance checks reuse an answer only when capsule position and
  rotation **and** proposed vehicle position and rotation are identical. This
  one-entry cache ends with that read-only candidate check.

There is no cross-update cache. Other vehicles, fragments, revised terrain,
real boarding checks and dirty-query guards retain their collision semantics.
Every candidate still receives the existing floor, hull and hatch checks.
Physics frequency, bot selection policy and survey cadence remain unchanged.

| Three-minute Pi replay | Before | After |
| --- | ---: | ---: |
| All sensor work, mean per actor update | 1.382 ms | 1.109 ms |
| P1 sensor mean during seconds 150–180 | 6.309 ms | 4.797 ms |
| Cumulative hatch-check time | 14.120 s | 8.326 s |
| Shared scenario step mean | 0.654 ms | 0.639 ms |

Hatch work falls 41%; the busy-window sensor mean falls 24%. All non-timing
report and per-tick CSV fields match across 10,800 ticks. The added detailed
scopes also match the original replay's first 10,800 non-timing CSV rows.
This is a paired query-cost measurement, not an isolated rendered-FPS gain.

## Full replay and validation

The first 600-second Pi replay (`9216675843324634618`) matches all non-timing
report and CSV fields across **36,000 ticks**. Physical/material audits pass.
Sensor mean falls from 0.968 to 0.815 ms/actor; shared-step mean remains about
0.64 ms. Sensor samples exceeding 16.67 ms remain essentially unchanged
(1,550 versus 1,549): the optimization does not fix the ground-map spikes.

360 scenario library tests and 81 AI integration tests pass, covering flight,
landing, mining, loss/rebuild, ground routes, jetpack crossings and missions.
Client tests compare profiled and unprofiled adapter state/telemetry and verify
human/bot seat attribution, bounded history and read-only diagnostics.

The live-seed baseline exposed an older **diagnostic-only** panic at tick 13,500:
`pair_diagnostics` indexed a collider removed during ship rebuilding. Rapier's
contact pairs can retain that handle until the next physics step. The report now
uses checked lookups, counts `removed_collider_candidates` separately and excludes
those stale pairs from live-contact counts. It does not flush queries or step the
world. A regression removes a body between steps, reads the diagnostic without
mutation and verifies cleanup after the next ordinary step. Both paired runners
receive this reporting correction. Physics profile schema version 2 adds the
removed-collider count.

The live seed also matches all non-timing report fields, physics populations,
pair samples and all 36,000 CSV rows in its paired ten-minute run. Both rounds
finish at the same tick and physical/material audits pass. P2 sensor mean during
seconds 270–300 falls from **6.580 to 4.911 ms/update** (25%). Whole-run sensor
mean falls from 0.735 to 0.633 ms/actor (14%); shared-step mean stays near 0.61 ms.
The 976 sensor samples over 16.67 ms persist. Cumulative hatch work falls from
18.981 to 11.809 seconds. The corrected report records nine removed-collider
pairs at tick 13,500 instead of panicking.

The Pi reports roughly 72–75 °C during these trials. Both three-minute runs
sample 1.5 GHz throughout. The longer optimized runs each include one initial
lower-frequency sample before the remaining 1.5 GHz samples; these are observed
wall timings, without frequency normalization. Detailed scopes are identical
within the three-minute and live-seed paired comparisons. The first full replay
baseline has only the earlier coarse sensor scopes.

## Live diagnostics and reproduction

`spacewars-cli status` now includes the actual `match_seed`, `match_tick` and a
bounded 120-update `mission_*` timing window. It separates each bot's observation
and policy from the shared step, then includes existing world phase timings.
The per-seat metadata identifies the current request, vehicle form, task and
returned survey sizes. Human seats contribute zero bot work. Diagnostics format
and sort the window when the host refreshes status, not inside each query.
Only a few coarse monotonic timestamps run in the ordinary client; detailed
query clocks are compiled behind `sensor-profile`.

Automatic matches generate fresh seeds. The launch setting `seed = 0` is not
the current automatic match's seed. On older clients, pause and read
`spacewars-cli ui state --json`: `pause.restart.value` contains the actual seed.
Resume with `spacewars-cli ui activate pause.resume --expect-screen pause.main`.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --seed 9216675843324634618 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --landing-survey-hz 60 \
  --profile-physics true --timing-csv true --out /tmp/landing-query-replay
```

Repeat with `7725194555774358125` for the live capture's seed. Use the same
architecture and build settings for paired replay comparisons: desktop and Pi
trajectories can diverge even with the same seed. Headless replay also does not
include the rendered host's scheduling. Keep the live capture alongside it.

Artifacts are outside Git at
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/control-stall/`.
They include original and optimized runners, raw status, the live screenshot,
settings, per-tick CSV, nested sensor timings, thermal/frequency readings and
collection scripts. Pi runners execute while the kiosk is paused and suspended;
cleanup resumes it, checks unchanged settings, verifies downloaded hashes and
removes only the experiment's temporary directory.

## Cabinet deployment

Deployed to `sw-picade.local` on 2026-09-12 using the Yocto kiosk build and
restricted fast-update helper, without rebooting. The installed executable and
running PID 6558 both match SHA256
`940f967d6eb16795ca5a152186e26ee7c821a9fed3592f5f71d6ca26b3304738`.
The service is active with zero crash restarts; automatic two-bot play resumes
at native 1× resolution. Saved settings are byte-for-byte unchanged.

A 47.55-second sample of fresh match `11941972997238835850` measures
**43.66 FPS / 59.77 UPS** from counter differences. A brief full-survey phase drops
to 12.3 rolling FPS, with P1 sensors averaging 8.666 ms/update and the scenario
step at 1.057 ms. The following sample returns to 41.4 FPS. This is operational
validation and evidence of the remaining survey cost, not a matched before/after
rendered-FPS comparison. The original nine-FPS capture and this fresh match
have different seeds and phases.

## Remaining work

The follow-up [survey cadence change](landing-survey-cadence.md) schedules
repeated searches and preserves immediate selected-site checks. The evidence
above describes the earlier per-tick baseline; its reproduction command now
selects that behavior explicitly.

Removing duplication reduces each full survey's cost; it does not remove the
64-candidate survey on every update while choosing a destination. Repeated full
surveys and ground graph connection checks remain the next leads. Any wider
cache or staggered survey needs explicit invalidation for moving obstacles,
terrain edits, vehicle shape, planet motion and policy requests. Do not infer
safety from a terrain revision alone.

Ground graph construction still causes occasional large sensor spikes. The
current evidence favors investigating connection queries before changing the
512-node resolution. Continue capturing per-update timings and actual seeds;
keep rendering costs and physics work separate from these sensor costs.
