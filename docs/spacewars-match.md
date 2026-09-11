# Spacewars on destructible planets

The normal `spacewars` launcher entry uses the generated three-planet material
match. Each player independently selects **human** or **rule bot** in Settings:
two humans, either human/bot arrangement, or two bots watching the same round.
The default is two humans. Choices persist through restart and relaunch.

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
round, even with owned planets. Simultaneous deaths draw. **Play Again** starts
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
Validation results and the identified Pi installation are recorded below when
completed. Artifacts are stored under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/spacewars-promotion-20260910/
```

This does not close the [parked landing investigations](landing-investigation-guide.md)
or the retained pod-landing regression and hard-touchdown findings in
[pilot impact survival](pilot-impact-survival.md). The previous Pi display
captures also remain below 60 FPS in some views. Larger worlds, broader damaged
ground navigation and further balance/performance tuning remain follow-ups.
Merging remains deferred pending the integrated controller playtest.
