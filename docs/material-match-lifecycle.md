# Pilot health and finished material rounds

The generated material arena now opts into a pilot-based round lifecycle.
The confirmed rule is that a living pilot can survive loss of every ship and
flag, reclaim a planet and rebuild. Pilot death ends that player's round even
if they still own planets. There is no respawn entitlement from a flag.

These are the first-slice results. The subsequent [match-pacing investigation](match-pacing.md)
identifies the recorded impact deaths, corrects direct wreckage damage and
adds bounded pursuit before every planet is secured. Its paired desktop/Pi
results and deployment supersede the current-build counts below.

The earlier landing investigations are parked with exact reproductions,
recordings and next hypotheses in [the resumption guide](landing-investigation-guide.md),
committed at `e18b960`. They remain open.

## First damage model

- A pilot starts with 100 health. That pool persists through boarding, leaving
  a vehicle, full-ship destruction and rebuilding. It has no automatic healing.
- A full ship shields its occupant using the existing hull health. Destroying
  an occupied full ship creates the existing escape pod. Destroying an empty
  ship preserves the external spaceling.
- On foot and in an occupied pod, laser hits damage pilot health. A physical
  missile hit removes 40 health and consumes the missile. The pod uses pilot
  health rather than its existing hull-life field, which measures rebuild
  progress. Destroying an occupied pod therefore means losing its pilot.
- Fresh hard impacts above 12 units/s deal four health per excess unit/s,
  capped at 100 per impact. Damage uses pre-solver relative closing speed and
  deduplicated contact pairs. Resting support forces are harmless.
- Solar heat uses the existing 24-unit corona and linear falloff, reaching
  20 pilot health/s at its inner edge. Actual material and physical colliders
  determine line of sight and impact contacts.
- Ejection grants three seconds of pilot protection. It does not reset health.
  The full-ship killing shot cannot also hit the newly created pod. Boarding
  and exiting do not grant another protection window.

These are initial tuning values, not a balance claim. In particular, the
survival time under concentrated fire and the opportunities to reach cover
need controller playtesting.

## Step and outcome boundaries

Both pilots still share one gravity solve and one Rapier step. Lasers, contacts
and heat finish before the outcome is evaluated. One surviving pilot wins;
both pilots dying in the same completed step draws. Asset counts never decide
this outcome. Death is resolved before that step can advance claims, mining
or rebuilding.

A terminal round freezes world state and controller policy updates. The arena
uses the existing game-over menu, including Play again and return to launcher.
Restart reconstructs the same seed with healthy pilots and reset bot state.
Each player view shows pilot health, ejection protection and the round result.

The combat sensor includes living external pilots and occupied pods in this
mode, using their real body position and first-solid visibility query. Unarmed
survivors do not count as incoming ship-weapon threats when selecting cover.
The existing mission and combat controllers consume those targets through
their normal controls. Further improvements to attacking a small ground target
remain a separate performance question.

## Entry points and historical fixtures

Interactive `spacewars-terrain-arena` and `spacewars-terrain-arena-duel` enable
the rules. The scenario entry point is `init_material_match(seed)`; controlled
trials can call `enable_match_rules()` once, before stepping. Initialization
does not provide a runtime heal, elimination or ownership command.

The two-planet travel experiment and focused landing, recovery and combat labs
retain their prior survival behavior. `init_material_arena` also retains it for
historical endurance comparisons. The ordinary `spacewars` registration is a
later promotion step after the material lifecycle has been playtested.

## Reproducing a measured round

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 42 --seat 0 --mirror false --mode duel \
  --match true --asteroid-interval 3 --seconds 180 --frames true --trace true \
  --out /tmp/spacewars-material-match
```

`report.json` records pilot health, last damage cause, death tick, winner/draw,
finish tick and `termination`. `round_finished` means an actual outcome;
`budget_exhausted` is an unfinished round, even when physical audits pass.
`--require-finish true` turns a missing result into a failed assertion after
writing the report. Omitting `--match true` reproduces the historical lab rules.

One-second samples include the round state. A terminal frame is recorded even
when death falls between samples. Frame JSON is renderer input, not a screenshot.
Read actual captures separately when evaluating Pi presentation.

## Validation

Focused contracts exercise actual laser and missile hits on both survivor
forms, material occlusion, hard-impact speed, harmless surface movement,
health through transfers/rebuilding, asset-loss survival, ejection protection,
solar falloff, owned-planet death, same-step draws and frozen results. Client
checks cover mode selection, health initialization and reset; the existing
host game-over workflow supplies controller menu and restart behavior.

The current run's logs, reports and deployment evidence are stored under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/material-match-20260910/
```

Runtime implementation: `211b201`; combat-runner and client-result contracts:
`1797959`. The workspace passes 1,142 tests, plus nine example tests. All ten
explicit rendered terrain UI workflows pass. Formatting and Clippy pass
(existing Clippy warnings remain), and the six navigation/twelve strategy
historical episodes retain their frozen results. The final client run includes
an actual bot-combat round followed by result-message, frozen-policy and reset
checks; it runs all 238 client tests.

The 48 headless trials cover 6,663 simulated seconds (about 111 minutes), with a
three-minute maximum per trial. Finished rounds stop at the actual death tick.
All physics/material audits pass.

| Trial group | Desktop | Pi |
| --- | --- | --- |
| Generated arena, seeds 0/2/7/42, both reflections, quiet/Mixed 3s arrivals | 16/16 audits; 0/16 finished by 180s | 16/16 audits; 0/16 finished by 180s |
| Close combat, seeds 7/42, both reflections, combat breaks Off/8s | 8/8 finished in 33.55–88.98s | 8/8 finished in 20.47–114.83s |

The generated bots continue their capture itineraries; these are unfinished
rounds, not successful match completions. There is no forced three-minute
timeout in the game. All desktop generated pilots retained full health. Four
Pi generated cases recorded nonfatal impact damage; final affected health
ranged from 47.31 to 95.75. The quiet reflected seed 2/P2 and seed 7/P1 traces
show airborne jetpack travel followed by material contact when health falls,
not a stationary support-force drain. One-second snapshots are not full
contact manifolds, so revisit the actual contact before adjusting the model.

Fourteen close-combat defeats ended with an impact, and two with a laser.
Ship loss, wounded-pilot recovery, rebuilding and continued combat occurred in
one desktop and two Pi cases. For example, desktop seed 42, unreflected, breaks
Off: P1 survives with 27.87 pilot health, rebuilds, and wins at 88.98s after P2
dies in its pod. These small samples establish lifecycle behavior; they do not
establish balanced outcomes or a preferred combat-break rate. Outcomes and
trajectories differ between architectures.

Use this close-combat reproduction to exercise a complete round directly:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --mirror false --break-interval 0 --match true \
  --seconds 180 --frames true --require-finish true --out /tmp/material-round-combat
```

Its report records pilot damage events, final recovery/health, terminal cause,
exact elapsed ticks and a final audit even when death falls between samples.
The `*-combat-summary.json` and `*-round-summary.json` files record every
command and runner hash; `report-manifest.json` identifies all 48 reports.
The Pi trials ran alongside the paused kiosk renderer, so their headless
timings are not isolated CPU measurements. A copy of both remote trial trees
is preserved in `material-match-before-update.tar.gz` before deployment.

The next balance investigation should distinguish lethal pod arrivals from
direct weapon defeat, and inspect hard jetpack touchdowns before relaxing
impact thresholds. Seed 42/reflected/desktop/breaks 8 ends only three ticks
after ejection protection expires. On the mission side, capture priorities
still deserve explicit competition with pursuit; simply extending all trial
clocks would hide that question. The parked landing investigations remain
available independently through their resumption guide.

## Verified Pi deployment

Source checkpoint `1797959` was installed through the A/B updater at
`spacewars.local` (`192.168.1.108`), booting slot B, `/dev/sda3`. The installed
client hash matches the archived image's extracted executable:

```text
image  7fcc899f9e6885b63e8c8e7210c73b157df91ee0c30618a0e0e1f796a2d16a6e
client 7200269f557793ba95042d3a2875644a75196698f03adde49f911b58489c9876
```

The rendered seed-42 arena duel ran beyond three simulated minutes with Mixed
asteroids every three seconds, raster scale 2, and no service restarts. Four
samples recorded 46.7–55.7 FPS and 59.0–60.6 UPS. Their exact capture times are
in `pi-live-summary.json`; screenshot names retain nominal 30/75/120/180s
markers, but the final capture was delayed and occurred after 180s.

Both bots continued capturing, boarding and travelling. The final screenshot
shows P2 back aboard with 47% pilot health, confirming that the damaged pilot
retains that health through boarding in the rendered game. The round remained
active. Screenshots and status logs are `pi-live-*.png` and `pi-live-*.log`.

Afterward the arena was restarted as human P1 versus mission-bot P2 with Mixed
asteroids every eight seconds and left paused for controller playtesting.
`pi-ready-state.json` and `pi-ready.png` record that fresh state. The new rules
have not been merged or promoted to the ordinary Spacewars registration.
