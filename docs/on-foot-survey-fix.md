# Skip unused landing surveys in on-foot missions

The [Pi investigation](material-match-pi-profile.md) found that a spaceling
returning to its rebuilt ship could trigger all 64 landing-site checks every
simulation update. The recovery policy does not consume these candidates while
on foot. The production fix is in `SurfaceSortieState::mission_observation`:
an on-foot player with a full ship and no selected site uses the existing no-site
request. Explicit selected sites still receive physical validation. Boarding,
support, navigation, rebuilding and escape-pod sensors retain their existing
checks. Returning aboard immediately restores the normal survey behavior.

Lower-level pilot/recovery survey APIs retain their semantics for diagnostic and
historical policies. No physics rate, terrain representation, renderer or bot
decision policy changes. Profiling scopes remain absent from normal builds.

## Validation

- 359 scenario library tests pass, including new exit/walk/reboard and pod-survey
  regressions. The on-foot test compares the entire recovery observation with
  the old survey after removing only the two candidate lists, verifies ground
  navigation remains present, and checks read-only behavior and selected sites.
- 79 tests pass across `surface_mission`, `surface_recovery`, `ground_navigation`,
  `ground_jetpack` and `jetpack_crossing`. These cover real ship loss/rebuilding,
  footing destruction, both crossing directions and physical return/boarding.
- The 28,787-tick desktop replay matches all non-timing report and per-tick CSV
  fields from the archived baseline. All 57,574 player controls match independent
  reference policies. This desktop trajectory never triggers the omitted survey;
  the Pi replay is the direct regression case for the performance fix.
- The Pi replay ends at the same 18,544 ticks as the archived original, with all
  non-timing report and per-tick CSV fields identical. Its reference bots restore
  4,528 full on-foot surveys and still produce identical controls for all 37,088
  player updates. Physical/material audits pass. During simulation seconds
  210–240, player 2's mean sensor time falls from **12.403 to 0.824 ms/update**.
  Whole-run sensor mean falls from **2.064 to 0.648 ms/actor**; shared simulation
  step mean is essentially unchanged (**1.316 to 1.320 ms/update**). Ground-map
  work still causes 693 sensor samples over 16.67 ms. This is headless query
  evidence, not a rendered FPS speedup measurement.

## Cabinet deployment

Deployed to `sw-picade.local` on 2026-09-12 with the Yocto Pi kiosk build and
restricted fast-update helper. This replaces the client/CLI and restarts the
application without a reboot. The helper preserves the previous binaries for
rollback. Both the installed executable and `/proc/1646/exe` match SHA256
`333f2f5c52c5873f432437f23b94c8f7aa0dee724c5ad275fd4b1a2a12fe6716`.
The service is active with zero crash restarts; the UI reports ordinary
`spacewars` gameplay, unpaused, with no error. Automatic two-bot play resumed.

A 32-second post-deployment sample measured **18.11 FPS / 59.62 UPS** from
frame/update counter differences. Rolling mean simulation/control cost was
2.51 ms per frame; scene/preparation/KMS rendering totaled about 50.60 ms.
The pre-deployment sample measured 16.06 FPS / 59.28 UPS, but these were different
generated matches and phases. These live samples establish operation and the
remaining rendering limit, not an isolated before/after FPS speedup. The original
late-game five-FPS incident is not recreated by starting a fresh match.

For playtesting, let bot-versus-bot matches reach ship loss, rebuilding and the
walk back to board. Also check manual exit, ground movement and reboarding. If
another slowdown appears, record its world seed and FPS/UPS counters; retain the
ground-map and rendering costs identified in the investigation as separate leads.

## Reproduction

```sh
cargo +1.89.0 test --locked --release -p scenario-spacewars --lib
cargo +1.89.0 test --locked --release -p spacewars-ai \
  --test surface_mission --test surface_recovery --test ground_navigation \
  --test ground_jetpack --test jetpack_crossing
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --seed 12448715435361755911 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --profile-physics true --timing-csv true --verify-on-foot-surveys true \
  --out /tmp/on-foot-survey-replay
```

`--verify-on-foot-surveys` keeps independent reference bot state and restores full
on-foot surveys only in its observations. It asserts identical encoded controls
every player tick. Reference work and output writes are outside measured stages,
but may affect caches; do not interpret total wall duration as production speed.
The report records how many full surveys were restored, so a replay that never
exercises the fix can be distinguished from direct coverage. Leave the flag off
for ordinary performance runs.

Artifacts for this fix are outside Git at
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/match-fix/`.
The earlier original/experimental executables and reports remain intact in
the sibling `match-profile/` directory.
