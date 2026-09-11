# Recovery routes through damaged ground

This follows the [asteroid-pressure checkpoint](asteroid-pressure.md). It improves
the measured ground/jetpack routes shared by capture and recovery bots, widens
the search for crater launch points, and samples actual surviving footing for
rebuild relocation. Policy labels are `ground_navigation_v5`, `recover_ship_v5`
and `tactical_sortie_v4`.

## What the saved failures showed

The seed-7/P2, mirrored, 3-second Light case is a real stranded-ship event.
Two missile contacts at tick 1572 knock the newly emptied ship away; its radius
from the planet reaches roughly 474 units before it falls back. The pilot
successfully claims at tick 1760, but cannot board an airborne ship. The missing
hatch observation is correct. The ground task now shows "waiting for the
assigned ship to land" and reports "assigned ship has no grounded hatch" if its
existing wait expires. This case is not counted as a completed capture sortie.

Several other failures were incomplete route searches. The ground graph covers
the planet, but only eight nearby terrain crossings are surveyed for jetpack
travel. Requiring a complete route to a distant flag rejected useful local
walking and flight before the bot could reach the next survey area.

The seed-42/P1, mirrored, 3-second Mixed case also knocks the recovering P2
spaceling far away from its route. The old task keeps its previous waypoints and
expires its eight-second progress timer while the spaceling is still falling
and getting up. With displacement recovery, it resumes travel but encounters
a later crater whose immediate edge is a poor launch point. Looking farther
back along measured footing provides a usable crossing to the flag.

## Controller and sensor changes

A full measured route remains preferred. If the combined walk/jump/jetpack
graph cannot reach the destination, the navigator may follow a partial route
that advances at least 1.5 units of surface arc toward it. It executes only
measured edges and its first measured flight, then surveys again. Partial-route
completion never authorizes claiming or boarding; actual interaction range,
support, balance and transfer gates still decide those actions. Telemetry marks
partial routes and counts them separately.

Displacement more than six units from the remaining route invalidates it. The
navigator waits for actual retained-planet contact, gets up if needed, and plans
from the new footing. The original ninety-second ground deadline is retained;
the ordinary eight-second progress timer resumes after landing. Measured jetpack
flight continues to use its separate corridor revalidation and interruption
rules.

Terrain crossing surveys now inspect up to eight endpoint margins instead of
four, with a maximum endpoint separation of 24 units instead of 14. They retain
the existing three cruise-height candidates, ten-unit climb envelope, capsule
clearance checks, fuel rules and eight-candidate survey budget. This changes
where the bot can discover a flight, not the spaceling's equipment or thrust.

Rebuild relocation previously checked eight fixed bearing offsets. Those can
all miss suitable ground around a crater. It now chooses up to 32 measured
standing positions, at least two units apart, within 24 units of the pilot.
Eight candidates are checked per scheduled observation, rotating through
the set within two seconds. Relocation includes measured local jetpack corridors
when the pilot has that equipment; walking and flight distances are reported
separately, with the same 24-unit combined travel limit. The navigator retains
the rebuild destination during flight unless ownership changes. The proposed
ship must still have a usable ground route to its hatch. The actual build
rechecks placement; previews reserve no space.

The existing five-second missing-route and relocation waits, fifteen-second
missing-hatch wait, capture deadline, recovery deadlines and retry limits remain
in force. Unreachable sampled routes produce explicit terminal outcomes. No bot
uses the diagnostic ship-loss chord to discard a surviving ship.

## Reproduction and evidence

Use the existing pressure, impact and jetpack matrix drivers. For the saved
knockback/crater case:

```sh
cargo build --locked --release -p spacewars-ai --example surface_combat_soak
target/release/examples/surface_combat_soak \
  --seed 42 --subject-seat 0 --mirror true --seconds 180 \
  --landing-policy tactical --land-after 0 --break-interval 8 --break-seconds 4 \
  --asteroid-interval 3 --asteroid-severity mixed \
  --continue-after-failure true --frames true --out /tmp/damaged-ground
```

The runner now preserves the last rebuild preview with each route failure and,
with `--frames true`, a close actor view at the failure tick. Sensor and policy
timings are recorded separately from the existing combined AI timing, which
also includes diagnostic bookkeeping. Reports distinguish capture departure,
later recovery completion, ongoing work at 180 seconds and terminal failures.

Artifacts are retained at:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/damaged-ground-recovery-20260909/
```

## Validation at `cbc1660`

The release workspace suite passed: **1,060 passed, zero failed, 22 ignored**.
The six ignored material UI workflow tests were then run explicitly under Xvfb
and passed. Ordinary `navigation-v1` and `strategy-v1` baselines matched. Clippy
for the changed scenario/AI packages and all their targets passed with existing
warnings in untouched code.

Three new contract tests cover partial routes and resurvey without false
arrival, knockback settling/replanning without resetting the overall deadline,
and preserving a rebuild destination during jetpack flight until ownership
changes. They also exercise read-only observations, cloning or reset behavior
where relevant.

Each platform ran the same 100 cases for 180 simulated seconds per case:

| Matrix | Desktop | Pi |
| --- | ---: | ---: |
| Jetpack navigation: completed objectives and clean physics | 16/16 | 16/16 |
| Impact recovery: completed objectives and clean physics | 36/36 | 36/36 |
| Asteroid pressure: clean physics audits | 48/48 | 48/48 |
| Initial capture sortie completed through departure, within pressure runs | 43/48 | 45/48 |

That is ten simulated hours across both platforms. A pressure physics pass does
not mean every capture or later recovery succeeded. The pressure runner continues
after a failed initial sortie to observe subsequent combat and recovery.

Compared with the previous `d20b8b1` binaries on the same platform/cases:

| Recovery measure across the 48 pressure runs | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Completed recovery events, including repeated recoveries | 18 → 20 | 23 → 23 |
| Last recovery task blocked at cutoff | 15 → 9 | 19 → 15 |
| Last recovery task still running at cutoff | 28 → 32 | 22 → 27 |
| Last recovery task succeeded at cutoff | 16 → 18 | 19 → 19 |

These task totals include the active opponent brain and the subject's combat
brain after its initial sortie; they exclude the unused subject brain. They are
task counts, not mutually exclusive outcomes for the 48 rounds. Desktop and ARM
physics trajectories differ, so comparisons are within each platform.

The saved desktop knockback/crater case now reaches the flag: at 180 seconds P2
is supported and lowering P1's flag, with about 25.6% of that stage complete.
It has not completed that recovery before cutoff. In the saved Pi rebuild case
(seed 7/P1, mirrored, 3-second Mixed), the final survey now evaluates eight
reachable candidates using one or two measured flights. Their ship-placement
previews are rejected for missing ground, obstructed hulls or missing hatch
footing. This fixes the walking-only search mismatch but does not produce a
buildable position within that bounded search.

Remaining final blocked recovery reasons are explicit:

- Desktop: eight with no suitable pod landing site and one with no supported
  claim progress.
- Pi: twelve with no suitable pod landing site, one with no reachable valid
  rebuild placement, one after three interrupted jetpack routes, and one whose
  assigned ship has no grounded hatch.

Running tasks are also preserved as unfinished, rather than counted as success.
Most are still landing pods; others are claiming, rebuilding, stabilizing or
searching for build space. Broader site searches, highly damaged footing and
repeated disruption remain useful follow-up cases. Mining has not been added to
the bot: the replay evidence first justified improving measured routes and
placement searches. A surviving ship thrown out of reach still needs an explicit
normal-match salvage, recall or abandonment rule.

## Timing and provenance

The Pi kiosk remained active during headless validation. Across its 48 pressure
cases, the largest per-case sensor p95 was 0.314 ms and policy p95 was 0.00345 ms.
The largest individual sensor call was 18.09 ms, policy call 1.39 ms, and physics
step 11.17 ms. These are separate maxima, not a combined frame time; the sensor
tail can exceed one 60 Hz frame and remains a performance follow-up. Desktop
sensor/policy maxima were 18.25/0.436 ms. Diagnostic aggregation is excluded from
the new split sensor/policy timings.

`validation-summary.json` retains before/after counts, unfinished capture reasons,
every final recovery status and timing maxima. `final-{desktop,pi}-binaries.json`
records the immutable headless binaries used for validation. Reports from earlier
intermediate binaries remain separately named and are not included above.

The Pi image was built from source commit
`cbc1660176bea605799e6505da0e42cf424cb10a` and archived as
`spacewars-image-cbc1660.ext4.gz`:

- Image SHA-256: `dea2e2c7684103bde7852bf8fae5b9bb6a914b20236b65cc374bfc0b8976f0b5`
- Packaged client SHA-256: `789469156bf277a7e7d2fddac018ef6e5fb8ea66069a03fb29b8c307eb3bb5b1`

The image was deployed to `spacewars.local` (`192.168.1.108`) through the A/B
updater, from slot A to slot B (`/dev/sda3`). The installed client hash matches
the archived image; `spacewars-kiosk.service` is active with zero restarts.

A live Capture duel with 3-second Mixed asteroid arrivals ran for about 224
wall-clock seconds. Screenshots/status were retained at about 32, 77, 122 and
225 seconds. Sampled FPS was 59.7–60.1 and UPS was 60.1–60.6, with zero service
restarts throughout. The samples show P1's initial capture/departure, the planet
later becoming neutral, and P2 subsequently on foot with its own flag and
rebuilding a ship. This live observation does not establish a completed final
rebuild/departure; the headless matrices carry that objective evidence.

The device was then left in a fresh paused `spacewars-terrain-combat` round:
P1 human versus the Capture bot, 8-second Mixed asteroid arrivals, and combat
breaks configured for every eight engagement seconds lasting four seconds.
Press Start to resume. `pi-ready-state.json`, `pi-ready.png`, deployment logs,
`pi-live-summary.json` and the four live screenshots retain the device evidence.
