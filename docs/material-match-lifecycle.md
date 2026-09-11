# Pilot health and finished material rounds

The generated material arena now opts into a pilot-based round lifecycle.
The confirmed rule is that a living pilot can survive loss of every ship and
flag, reclaim a planet and rebuild. Pilot death ends that player's round even
if they still own planets. There is no respawn entitlement from a flag.

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

Final measured results are added after validation completes.
