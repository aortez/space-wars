# Spacewars on destructible planets

The normal `spacewars` launcher entry uses the generated three-planet material
match. Each player independently selects **human** or **rule bot** in Settings:
two humans, either human/bot arrangement, or two bots watching the same round.
The default is two humans. Choices persist through restart and relaunch.

The subsequent [world-selection flow](match-worlds.md) adds New Match for a
fresh seed, Rematch for the current world, and visible seeds in the menus.
The checkpoint results below retain the original promotion evidence.

This is the same simulation, mission policy and world profile as the material
arena, with one physics step and gravity solve. This promotion changes the
client entry point and player selection, without changing terrain, damage,
landing permissions, weapon balance or AI decisions.

## Play a round

Fly to a planet, brake and land on both rear feet. Exit at the cyan hatch and
stand still on surviving ground for three seconds to raise a flag where you
stand. An enemy flag must first be approached and lowered. Destroying or
detaching its footing removes the flag and makes the planet neutral.

A ship's destruction leaves its living pilot in an escape pod or on foot.
Land the pod, exit, claim ground if necessary, then stand still on an owned
planet for eight seconds to rebuild. Board the replacement after it settles.
Losing every ship and flag does not eliminate a living pilot. Pilot health
persists through transfers and rebuilding; pilot death ends that player's
round, even with owned planets. Simultaneous deaths draw. **Rematch** starts
the same seed with fresh health, terrain, ownership and bot state.

| Action | Assigned gamepad | P1 keyboard | P2 keyboard |
| --- | --- | --- | --- |
| Turn aboard / walk or steer on foot | Left/right | A/D | Numpad 4/6 |
| Thrust / jump or get up | A | Space | Numpad 8 |
| Brake | D-pad Down | S | Numpad 5 |
| Exit / board own settled vehicle | B | X | Numpad 2 |
| Sweep wings for cruise | Hold RB | Hold J | Hold PageDown |
| Laser aboard / mining on foot | RT or LB | E | End |
| Missile | X (west face) | K | Home |
| Aim mining | Right stick | Arrow keys | Facing direction |
| Change mining cut size | Y | T | PageDown on foot |
| Jetpack lift | Hold A while airborne | Hold Space | Hold Numpad 8 |
| Pause / resume | Start | Esc | Esc |

Release controls after boarding, exiting and vehicle replacement. A held
launch/transfer input cannot immediately fire or thrust in the new state.
The jetpack recharges while standing still with jump released. A tipped pod
can briefly lift using brake plus thrust. Holding thrust, brake and transfer
for three seconds scuttles a stranded full ship and allows ordinary recovery.

Each physical pad retains its assigned seat; choosing a bot does not reassign
the other pad. Human controls for a bot seat are ignored. A bot's view shows
its current task. Mission bots select planets, land, walk or fly to flags and
hatches, fight and recover using the same action interface as humans.

## Settings and historical comparisons

Normal Spacewars exposes both player choices, renderer/raster scale, bot combat
break interval/duration, and asteroid arrival interval/strength. Breaks leave
the bot moving and vulnerable with its weapons off. The first promoted match
keeps three planets and the established health/world profile.

`spacewars-classic` retains the prior berth-based game, its world presets,
health/planet switches, legacy bot and benchmark. Its old ship/planet-based
elimination rule is distinct from the material match's pilot survival rule.
The historical visual/headless benchmark explicitly uses Classic;
`--benchmark` without a scenario also selects it. All terrain, arena, travel,
recovery and surface lab IDs remain available with their previous controllers
and rules. In particular, the arena lab still forces human P1/bot P2, while
its duel variant forces two bots independently of normal match settings.

Saved settings add `spacewars.player_1_controller`, defaulting to `human` when
reading an older file. The existing `player_2_controller` and historical world
settings are preserved. Choosing two bots stores `rule-bot` for both fields.

```sh
cargo run --locked --release -p engine-client -- --scenario spacewars
cargo run --locked --release -p engine-client -- --scenario spacewars-classic --benchmark
```

## Validation and remaining work

Client contracts cover all four seat combinations, pad ownership/disconnection,
rejection of direct human actions for bot seats, paused policy updates, bot HUD
labels, same-seed restart and saved-setting migration. Host checks cover the
normal entry's pause and reset. The physical result fixture retains pilot death,
frozen state and fresh health rather than injecting a winner.

An explicit comparison runs both the normal entry and the arena for seeds
0/7/42 with quiet or Mixed three-second asteroids, up to 180 simulated seconds.
It compares state and policy observations, outcome and material/physical audits.
Unfinished rounds remain unfinished at the cap. Run with:

```sh
SPACEWARS_MATCH_ARTIFACTS=/tmp/spacewars-match \
  cargo test --locked --release -p engine-client --bin engine-client \
  normal_match_reproduces_the_arena -- --ignored
```

The rendered functional workflows exercise every seat choice, both renderers,
saved settings, pause/controls/restart, and a real two-bot seed-7 round followed
by Play Again. The historical Classic benchmark workflow remains separate.
Artifacts are stored under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/spacewars-promotion-20260910/
```

### Checkpoint validation (2026-09-10)

Runtime checkpoint: `b856949449bce79ec6f08da3461cbe7500f1fcd0`.

- All 1,164 workspace/all-target tests passed in both optimized and unoptimized
  builds. Large unoptimized world fixtures need `RUST_MIN_STACK=16777216`; CI
  now supplies that test-thread stack size.
- All 28 rendered UI workflows passed. The final player-selection workflow
  also passed after fitting the complete controls text into the help panel;
  it verifies all four seat choices and persistence of bot/asteroid settings.
- Formatting and Clippy passed; Clippy still reports advisory warnings.
- The retained Classic navigation baseline matched all six episodes, and the
  strategy baseline matched all twelve episodes.
- All six normal/arena comparisons matched state, bot policy and outcome at
  every sampled second and terminal tick, with clean physical/material audits.
- Six fresh Pi headless runs passed their physical audits. Their finish ticks
  and termination reasons matched the previous Pi runs for the same cases.

Each material case has a 180-second simulation budget. Finished cases stop at
the real terminal event; unfinished cases remain recorded as budget exhausted.
The pairs below compare the normal client to its arena counterpart on desktop;
the Pi column uses the headless material-match runner.

| Seed | Asteroids | Desktop normal / arena | Pi headless |
| --- | --- | --- | --- |
| 0 | Off | Both unfinished at 180s | Unfinished at 180s |
| 0 | Mixed / 3s | Both unfinished at 180s | Unfinished at 180s |
| 7 | Off | Both P2 win at tick 7,896 | Finished at tick 6,768 |
| 7 | Mixed / 3s | Both unfinished at 180s | Finished at tick 9,807 |
| 42 | Off | Both unfinished at 180s | Unfinished at 180s |
| 42 | Mixed / 3s | Both P2 win at tick 8,198 | Finished at tick 4,480 |

These checks establish that the new entry point and player selection preserve
the established match behavior. They do not establish balance or reliable
completion for every seed. Desktop and Pi finish times are not identical.
The real desktop result screenshot and Play Again capture are in
`ui-finished-match/`; final settings and complete help captures are in
`final-ui-player-choices/`. Commands, exit statuses, per-second comparisons and
checksummed Pi reports are retained alongside them.

### Installed Pi image

The pinned Pi 5/HyperPixel image built successfully (6,608 tasks), was archived
before deployment, and booted `spacewars.local` on slot A (`/dev/sda2`). The
installed `/usr/bin/engine-client` hash matches the client extracted from that
archive; the kiosk service was active with zero restarts after boot.

```text
Runtime: b856949449bce79ec6f08da3461cbe7500f1fcd0
Image:   spacewars-image-b856949.ext4.gz
Image SHA256: 97abe730c9c74e3d1ade7563118f5c8f86198327d252d38bb66fa26b5d097fbf
Client SHA256: d357165912cce8f662a849165ae5be15514b2e9fc8782624be1802658a31538f
```

The build uses the artifact directory's `kas-pinned-image.json`. The image
manifest, build/deploy logs and `installed-build.json` retain the source,
archive, root partition and executable identity. The host-distro compatibility
warning remains in the successful image build log.

All four player combinations were selected through the installed launcher's
public UI controls and launched as `spacewars`. Device screenshots confirm the
independent player choices, appropriate bot HUDs, and complete controls text at
800×480. No gameplay state was injected for this check.

The live two-bot seed-42 round used Mixed asteroids every three seconds and
four-second combat breaks at the 15-second interval. At the 45.95-second wall
capture, P1 owned Planet 1 and was hunting the opponent. The 90.91-second wall
capture showed **Player 1 wins / opposing pilot lost**, P1 pilot health 100%,
P2 pilot health 0%, and 4,480 completed simulation updates (74.67 simulated
seconds). Winner and finish tick match the Pi headless case. The exact wall
time of the terminal event was not sampled. **Play Again** then restored both
pilots and ships to full health, neutral ownership and fresh bot state.

The two active-round captures reported 60.1 and 51.7 FPS, with approximately
60 updates/second. The result-screen capture reported 25.5 FPS and zero
updates/second; these sparse samples are not a performance benchmark. The
kiosk service retained zero restarts throughout the device checks.

Handoff: a fresh paused normal match, seed 42, **human P1 / bot P2**, raster at
2×, Mixed asteroids every eight seconds. Press Start to resume. Return to
Launcher → Settings to choose a bot for either player. `pi-handoff.json` and
`pi-handoff-settings.toml` record this state. The installed UI history,
`pi-settings-bots.png`, `pi-controls.png`, `pi-bots-90.png`, `pi-restarted.png`
and `pi-human-paused.png` preserve the device verification.

This does not close the [parked landing investigations](landing-investigation-guide.md)
or the retained pod-landing regression and hard-touchdown findings in
[pilot impact survival](pilot-impact-survival.md). The previous Pi display
captures also remain below 60 FPS in some views. Larger worlds, broader damaged
ground navigation and further balance/performance tuning remain follow-ups.
Merging remains deferred pending the integrated controller playtest.
