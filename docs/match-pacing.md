# Match pacing and pilot impact diagnosis

Runtime checkpoint: `86a56df`. This is a bounded continuation of
[pilot health and finished rounds](material-match-lifecycle.md), on
`surface-terrain-integration`. Ordinary Spacewars promotion and merging remain
separate steps.

## What the impact replays established

Read-only contact telemetry now records the last damaging pilot contact: the
other body's identity/kind, point, normal, closing speed, impulse and both
pre-solver motions. Its bounded record does not add queries or change physics.
The runners capture it with the current recovery/mission decision.

All sixteen close-combat replays retained their original finish ticks and
winners. Their fourteen impact deaths were **nine world-boundary collisions
and five wreckage hits**, not failed planet landings. The other two deaths
were from lasers. For example, desktop seed 42/reflected/breaks 8 takes a
40-health missile hit at tick 2486, followed by a lethal wreckage contact at
2487. The missile impact changes pod motion sharply; the fragment then closes
at about 200 units/s. Ejection protection ended at tick 2484.

Spacewars already gives breakup wreckage zero direct damage. The new pilot
impact rule had inadvertently ignored that setting and treated it like solid
ground. Pilot contacts now respect zero-damage debris. Wreckage remains
physical and can push a survivor into another collider. Missiles still deal
40 direct health; asteroids, retained/detached material and hard ground impacts
remain dangerous. There is no extra immunity window or health reset.

The two recorded quiet Pi touchdown cases also reproduced their old health:

| Case | Pilot | Damage ticks | Closing speeds | Health lost |
| --- | --- | --- | --- | --- |
| Seed 2, reflected, quiet | P2 | 8622 | 17.03 units/s | 20.11 |
| Seed 7, reflected, quiet | P1 | 5371 / 7720 / 9557 | 16.78 / 17.27 / 13.77 | 19.11 / 21.07 / 7.07 |

These were actual moving spaceling/planet contacts while the ground task
reported `settle`. They were not standing support forces. The 12-unit/s safe
impact threshold is unchanged. Before adjusting it, inspect why ground travel
reaches those contacts with that much closing speed. In particular,
`GroundNavigationTask` relinquishes control while settling after displacement;
that is an investigation lead, not proof that the guard caused every impact.
The broader [landing investigations](landing-investigation-guide.md) remain
parked independently.

## Bounded pursuit between capture trips

`material_mission_v8` keeps historical lab itineraries and enables these
priorities only for finished material matches:

- A visible exposed pilot/pod or ship below half hull health can trigger pursuit
  within 400 units.
- A visible opponent within 300 units can trigger pursuit after securing any
  planet, or in response to a weapon hit in the preceding three seconds.
- Recovery and solar escape take priority. A committed landing, on-foot capture
  and departure finish before a new pursuit can interrupt travel.
- A local chase lasts at most 30 seconds. Losing sight for six seconds or
  exceeding 600 units ends it sooner. A twelve-second interval then allows
  capture travel to resume before another opportunity is considered.

The existing flight, combat, weapon-energy and configurable combat-break
controllers execute those decisions using ordinary actions. The original
all-planets-owned hunt remains available. Target form and normalized health
come from the same physical combat observation; visibility still requires the
first-solid query. No ownership, damage, movement or outcome is prescribed by
the policy.

The report separates opportunistic pursuit from the historical
capture-all-then-hunt clock. Mission phase counts include capture, travel,
pursuit and recovery, and event history records pursuit start/end reasons.

## Paired three-minute trials

Generated worlds use seeds 0/2/7/42, both reflections, and either no random
asteroids or Mixed arrivals every three seconds. Close combat uses seeds 7/42,
both reflections, and combat breaks Off/8 seconds. Each run has its original
180-second limit; an actual result stops it earlier. Existing baseline reports
are retained rather than relabeled or extended.

| Generated-world measure | Previous desktop | Current desktop | Previous Pi | Current Pi |
| --- | --- | --- | --- | --- |
| Actual round finishes | 0/16 | 4/16 | 0/16 | 3/16 |
| Actual weapon contact | 1/16 | 13/16 | 0/16 | 13/16 |
| At least one planet claimed | 16/16 | 16/16 | 16/16 | 16/16 |
| Physical/material audits | 16/16 | 16/16 | 16/16 | 16/16 |

Current generated-world finishes take 128.85–163.90 seconds on desktop and
96.62–162.60 seconds on Pi. All 25 remaining rounds are explicitly
`budget_exhausted`, not successful match completions. Capture still occupies
about 53% of combined pilot time, with pursuit about 12%.

The seven generated-world defeats end in three planet contacts, two world
boundary contacts, one asteroid contact and one sun contact. Weapon encounters
can precede those impacts, so final damage cause is not a complete account of
how a fight was lost. Desktop seed 2/reflected/Mixed 3s includes an actual
rebuild and continued play; it remains unfinished at 180 seconds. A Pi seed 0
recovery returns to the original ship and must not be counted as a rebuild.

All sixteen current close-combat rounds finish. Thirteen end at the world
boundary, two with lasers and one with a missile. No direct wreckage damage
remains. This fixes the damage inconsistency but does not resolve the large
physical kicks that can make pod survival difficult. Close-combat rebuilds
and continued play still occur in one desktop and two Pi cases.

The matrix contains 48 final-candidate trials, 18 diagnostic replays and four
initial probes: 70 runs totaling 8,302.2 simulated seconds (about 138 minutes).
All final-candidate physical/material audits pass. Pi headless runs share the
device with its paused kiosk renderer; they are not isolated CPU benchmarks.
The median per-run generated-world physics p95 is about 0.30ms desktop and
0.96ms Pi. Rendering is verified separately below.

## Regression checks and evidence

1,148 workspace tests and ten example tests pass. Ten explicit rendered terrain
UI workflows pass, as do formatting, Clippy (existing warnings remain), and
all six navigation/twelve strategy baseline episodes. New contracts cover
real wreckage collisions with both survivor forms, contact diagnostics,
pursuit identity/reset and budget behavior, recovery priority, committed
capture protection, and a generated-world match from capture through actual
weapon contact, ship-loss survival and a terminal result.

The archived runners were built with the final production source. The only
source differences between their manifests and the committed checkpoint are
the subsequently completed tests. Image construction uses the committed tree. A later iterator cleanup in the
combat runner changes no production code; its repeated standalone reproduction
retains the same round and final audit at tick 2588.
All 34 remote Pi reports were copied and checksum-verified before updating.

Artifacts are under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/match-pacing-20260910/
```

`comparison-summary.json` contains paired counts and phase totals;
`diagnostic-verification.json` verifies unchanged diagnostic outcomes;
`report-manifest.json` identifies all 70 reports. Per-architecture binary
manifests record source hashes and runner checksums; `build_runners.py` retains
the toolchain and build recipes.
The group summaries retain every command. Generated reports include all pilot
damage events and traces; combat reports retain their damage/contact events.

## Reproduction and next investigation

A normal generated-world match, without forced hits or ownership:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 42 --seat 0 --mirror false --mode duel \
  --match true --asteroid-interval 0 --seconds 180 --frames true --trace true \
  --out /tmp/spacewars-match-pacing
```

The missile/wreckage case, now continuing past its former defeat:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --mirror true --break-interval 8 --match true --seconds 180 \
  --frames true --require-finish true --out /tmp/spacewars-pod-impact
```

For the quiet Pi ground contacts, use the generated command with seed 2 or 7
and `--mirror true`, retaining quiet asteroid settings. Architecture matters;
the original desktop traces did not have these quiet injuries.

The next survival investigation should track the missile collision, pod motion
and available braking distance before lethal world-boundary arrivals, alongside
hard on-foot touchdowns. Distinguish controller recovery time, collision kicks
and damage tuning. Do not extend all run budgets, remove real collisions or
relax impact thresholds just to obtain a passing finish count. The new pursuit
is a useful first policy; 7/32 completed worlds is not evidence that ordinary
match pacing or defeat balance is finished.


## Verified Pi build and rendered check

The A/B updater installed runtime `86a56df` on `spacewars.local`, booting slot A
(`/dev/sda2`). The installed client checksum matches the archived image:

```text
image  dcb7ffe0fbae053c2f3b20169a0c7103a348850256ed8440ba6a29d6187555c3
client b542b5476f4eceab1e08ee268d285cb4155d03a5488a55629168264f068a47ae
```

A rendered seed-42 arena duel used Mixed asteroids every three seconds and
raster scale 2. Actual 800×480 screenshots were inspected, separately from the
headless frame JSON. The four capture records are:

| Nominal capture | Actual capture completion | FPS | UPS | Updates |
| --- | --- | --- | --- | --- |
| 45s | 49.76s | 52.7 | 59.7 | 2967 |
| 90s | 90.88s | 55.5 | 59.5 | 5388 |
| 135s | 145.04s | 56.9 | 59.9 | 8654 |
| 180s | 180.89s | 56.6 | 59.6 | 10833 |

Capture names are nominal labels; completion timestamps include screenshot
and status RPC time. The final sample exceeds 180 simulated seconds slightly.
Every sample reports no service restarts. The screenshots show a claimed
planet, pursuit and weapon damage, followed by P2 surviving ship loss in a pod
with 97% pilot health while P1 continues engaging. The round is still active
at that capture; this is not another completed-round result.

After this check, the Pi was returned to a fresh, paused human-P1 versus
mission-bot-P2 arena with Mixed asteroids every eight seconds for controller
playtesting. The ready-state JSON and actual screenshot are archived. In-person
controller feedback on this build remains a user playtest, separate from the
automated menu/control-path checks.
