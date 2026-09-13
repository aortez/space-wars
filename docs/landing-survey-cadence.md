# Landing survey cadence

This follows the [landing-query profile](landing-query-profile.md). Reusing hatch
checks made each candidate cheaper, but a bot still scanned 64 candidates every
simulation tick while choosing a site. The new mission adapter schedules that
planning work separately from immediate physical checks.

## Rules

- Selected landing sites, current support, hatch/boarding eligibility, combat
  and ship/pod state still use the current completed physics step at 60 Hz.
- Ordinary full searches refresh at four Hz, with the two seats seven ticks
  apart. A new planet or vehicle context gets an immediate first search.
- Approaches to an enemy flag pair full searches with the existing two Hz
  route-to-flag-and-back survey. The route survey's original ticks are preserved:
  P1 at multiples of 30, P2 at 15 modulo 30. Intermediate candidate lists could
  not be selected without those routes, so they are omitted.
- No collision result is cached across ticks. The planner stores only its last
  survey tick, planet and vehicle form. Moving debris and changed terrain are
  checked afresh whenever candidates are returned.
- `LandingSiteQuery` distinguishes a full survey, a selected-site check, a
  deferred survey with its next tick, and no request. Deferred data does not
  count as rejected ground, invalidate a site or start a no-ground deadline.
  Physical query readiness remains a separate guard.
- Tactical approaches retain their existing clearance climb while waiting for
  usable candidates. Actual landing/exit gates take priority over deferred work.

The default applies to ordinary Spacewars matches and mission/arena clients.
Standalone historical observation APIs retain their per-tick behavior. The
headless mission runner defaults to the new cadence and accepts
`--landing-survey-hz 60` for reference runs. No player setting was added.

## Iterations and gameplay checks

Simply delaying every first scan to a four Hz slot failed the seed-3 asteroid
test: neither bot completed a sortie. Giving a new context an immediate scan
fixed that short test, but ten-minute Pi runs exposed further changes. The
intermediate version hovered while waiting and moved P2's flag-route phase.
In seed `9216675843324634618`, P1 became blocked by its ground traversal timeout
for 228.9 seconds. That implementation was not deployed.

The retained version preserves the earlier waiting flight command and original
flag-route decision ticks. Both versions still use physical collision/claim
rules; this cadence deliberately does not promise identical trajectories.

The following Pi trials used generated three-planet matches, both rule bots,
the normal 15-second/4-second combat breaks, no random asteroids and a ten-minute
round limit. Runs stop on a real pilot death. All terrain audits passed.

| Seed | Schedule | Simulated seconds | Completed sorties P1/P2 | Completed recoveries P1/P2 | Finish |
| --- | --- | ---: | --- | --- | --- |
| 9216675843324634618 | Every tick | 600 | 6 / 5 | 1 / 1 | Time limit, draw |
| 9216675843324634618 | Scheduled | 600 | 5 / 6 | 1 / 0 | Time limit, P2 wins |
| 7725194555774358125 | Every tick | 600 | 4 / 2 | 1 / 2 | Time limit, P1 wins |
| 7725194555774358125 | Scheduled | 302.8 | 3 / 2 | 0 / 1 | P2 pilot dies, P1 wins |

Neither retained scheduled run entered the terminal `Blocked` mission state.
This does not establish that every local approach or ground route remains
productive on arbitrary seeds. Keep the earlier recovery investigation notes.

Whole-run sensor means fall from 0.811 to 0.620 ms/actor/update in the first
seed, and 0.632 to 0.355 in the second. These runs have different trajectories
and, in the second pair, different lengths. Those numbers are workload
observations, not isolated estimates of the optimization's CPU savings.

## Measurements on the same physical trajectory

Each paired probe completed 36,000 physics updates and 72,000 paired actor
observations. Every non-timing physical/mission field in the ordinary per-tick
CSV matches its earlier every-tick reference, as do final pilots, planets,
mission telemetry, round outcome and terrain audit. All live observation
equality assertions passed.

| Seed | All sensors, reference → scheduled (ms/actor/update) | Reduction | Full-search requests, reference → scheduled |
| --- | --- | ---: | ---: |
| 9216675843324634618 | 0.851 → 0.611 | 28.1% | 2,045 → 86 |
| 7725194555774358125 | 0.641 → 0.372 | 41.9% | 4,276 → 252 |

At each reference run's busiest 120-update window, the same observations cost:

| Seed / seat / inclusive ticks | Reference | Scheduled | Reduction |
| --- | ---: | ---: | ---: |
| 9216675843324634618 / P1 / 22,713–22,832 | 11.42 ms | 1.75 ms | 84.6% |
| 7725194555774358125 / P2 / 16,606–16,725 | 10.35 ms | 1.32 ms | 87.3% |

The initial probe alternated order each tick, which put the periodic route
refreshes in one order group. The final probe also flips order each 30-tick
period. A repeat of seed `7725194555774358125` yields 0.639 → 0.374 ms overall
(41.6% saved), with savings of 43.1% when the reference runs first and 40.0%
when scheduled sensors run first. Its busiest window saves 86.9%. The physical
trace and final state again match the reference. Pi CPU-frequency samples stay
at 1.5 GHz throughout the retained gameplay and paired timing trials.

These are per-actor sensor averages, not complete frames or individual query
latencies. The full surveys that remain are still expensive. Ground graph/route
construction also remains: individual sensor calls in the scheduled gameplay
runs still reach about 51 ms. Scheduling removes repeated work; it does not
spread a single expensive ground survey over several updates.

In seed `9216675843324634618`, the largest retained sensor call is 51.77 ms
(runner row 21,511, P1). Its flag-objective survey takes 41.90 ms: 20.39 ms for
the base ground survey, including 18.04 ms connecting nodes, plus 21.51 ms in
the objective's own work. Landing candidates take another 9.37 ms. These are
nested scopes, so do not add the parent and child values. Investigate both
ground connections and the hull-aware candidate route evaluation before
reducing map resolution or adding a persistent geometry cache. The second seed
has the same pattern in its largest call. `remaining-spikes.json` preserves the
attribution alongside the complete raw scope logs.

## Automated validation

- 363 scenario unit tests and 170 AI unit/integration tests pass in release
  builds. The asteroid duel test now explicitly exercises both schedules for
  seeds 2 and 3, for three simulated minutes each, with the existing completed
  sortie, physical audit, material conservation and speed checks intact.
- Cadence tests cover both seats, read-only queries, immediate first ship/pod
  searches, paired flag candidates/routes, every-tick selected-site checks,
  destroyed selected ground and dirty physics queries. Deferred policy tests
  check that waiting does not fabricate rejection/failure evidence.
- Five client mission/profiling tests pass, covering mixed human/bot seats,
  pause/reset and instrumentation parity. The additional normally ignored
  normal-Spacewars-versus-arena comparison also passes: seeds 0, 7 and 42, each
  with no asteroids and three-second asteroid arrivals, up to three minutes per
  entry. Both clients produce the same physical state and bot telemetry.
- The mission example compiles with and without nested sensor profiling; the
  Yocto client/CLI build succeeds. Formatting and whitespace checks pass.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --seed 7725194555774358125 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --landing-survey-hz 4 --profile-physics true --timing-csv true \
  --out /tmp/landing-scheduled

target/release/examples/surface_mission_soak \
  --world generated --seed 7725194555774358125 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --landing-survey-hz 60 --compare-landing-surveys true \
  --profile-physics true --timing-csv true --out /tmp/landing-paired
```

The optional paired probe keeps the every-tick policy in control and measures
both observations against the same completed world. Its independent survey
history follows the scheduled requests. It alternates measurement order and
asserts that all observations agree apart from the explicitly omitted candidate
lists/cover and query status. `landing-cadence.csv` records both sensor times,
order, seat, tick and scheduled query. Its extra work is outside the runner's
ordinary timings but can affect CPU caches. It does not measure rendered FPS
or prove that using the scheduled observations produces the same actions.

Use the same architecture and build options for trajectory comparisons. The Pi
runners use Rust 1.89.0, AArch64 release builds and opt-in nested sensor scopes.
The installed kiosk has only coarse per-seat timing. Live profile version 2 adds
full/deferred/selected request counts in its bounded 120-update window and the
latest query kind. `match_seed` identifies the actual automatic match.

Artifacts are outside Git at
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/landing-cadence/`.
They preserve rejected and retained runners, patches, raw per-tick CSV and
nested timings, reports, paired-probe output, collection scripts and deployment
verification. Each headless Pi trial pauses/suspends the kiosk, samples thermal
and CPU-frequency readings, resumes the cabinet, checks unchanged settings,
verifies downloaded hashes and removes only its temporary directory.

## Cabinet deployment and live check

Deployed to `sw-picade.local` on 2026-09-12 through the Yocto fast-update helper,
without rebooting. The installed client and running PID 9360 both match SHA256
`d9789f716a689cd7c4ee41a2d93d0ffcbc208eb6089814b7a80bd1d21c4a66f4`.
The kiosk stays active with zero crash restarts. Settings are byte-for-byte
unchanged, including native 1× raster scale and automatic two-bot matches.

The 37-sample live capture covers 156.58 seconds of fresh generated match
`4483370521358205685`. Counter differences measure **42.28 FPS / 60.09 UPS**;
rolling FPS samples range from 33.8 to 55.7. The mean sampled KMS draw time is
13.00 ms. Both bots capture ground and return to combat; `live.png` records the
two-player game with its HUD and performance overlay.

The deployed counters show the new path in use. At tick 5,641, P1's last 120
updates contain 112 deferred surveys, four full surveys and four selected-site
checks, with a 1.595 ms sensor average. Deferred work appears for both seats.
That is also the largest sampled per-seat sensor average during this capture.

This fresh rendered match is operational validation, not a matched rendered-FPS
comparison with the old slow seed. Rendering remains a substantial frame cost;
the paired headless probes establish the sensor savings independently.
