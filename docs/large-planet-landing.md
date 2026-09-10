# Landing on generated material planets

This follows the [generated arena validation](generated-material-arena.md).
The bounded goal is reliable landing, exit and capture in the seed-0/P2 quiet
cases that previously made no completed trip within three minutes. The existing
fixed-world, generated-route, recovery and combat regressions remain acceptance
checks; this does not integrate match victory or elimination into ordinary
Spacewars.

## Findings and changes

An immutable desktop replay reproduced every per-second physical audit from the
previous build. Both Pi reflections reproduced their missing departures too.
On planet 0 the pilot reached the surface, but kept using hover thrust while
waiting to align with its proposed landing plane. Gravity and that plane differ
on orbiting, stepped ground. A failed touchdown then reset the local controller
and selected the same site again. Other attempts physically landed but could
not use the predicted hatch floor.

Current capture uses `tactical_sortie_v6` with `material_landing_v2` for the
final approach:

- A slow ship close above the selected site can commit to touchdown while
  still turning toward the surface normal. It releases hover thrust and brake,
  using ordinary landing assist to descend. Both solver-supported feet, settled
  motion and real hatch clearance are still required before exiting.
- A stalled approach or unusable landed hatch rejects that bearing for the
  current material revision. The ship lifts clear before surveying another
  site. A terrain revision can make a rejected bearing eligible again; resetting
  the mission clears its rejection history. Existing time and retry budgets
  remain bounded.
- Unexposed ships prefer nearby valid ground. Exposed ships retain the cover
  penalty. This avoids a long cover detour during a local retry when the ship
  is already unexposed, including detours that leave a moving destination's
  approach frame.
- Ship landing surveys use the real transfer gate's radial hatch search and
  capsule orientation at the proposed pose. The former ray along the estimated
  foot plane could predict an exit that the actual ship could not use.

The historical flight policy's control decisions remain available, while all
ship surveys share the corrected hatch prediction. Pod surveys, human controls,
physical landing/transfer gates and the shared physics/gravity step are unchanged.
Trial changes to retry timing and per-foot clearance steering did not resolve
the complete regression set and were removed.

## Validation

Decision tests cover the touchdown envelope, neutral thrust during settling,
bounded failure, repeated observations, cloning and reset, rejection until a
material revision, lift-before-survey, and exposed versus unexposed site choice.
A new physical regression follows seed 0/P2 in both reflections through real
landing, exit, on-foot ownership, boarding and departure within three minutes.
The existing generated three-planet and fixed-world route tests remain in place.

The headless matrix retains 48 generated missions, 52 fixed-world missions,
36 impact recoveries and 24 ship returns per platform, each lasting three
simulated minutes. Reports distinguish completed trips, final ownership,
blocked tasks and physical audits. The previous `abd29ec` reports provide the
comparison. Workspace checks and both frozen ordinary-game AI suites cover the
broader integration; live Pi rendering is assessed separately.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 0 --seat 1 --mirror true --mode quiet \
  --seconds 180 --trace true --frames true --out /tmp/large-planet-landing
```

Artifacts, immutable trial runners, diagnostic traces and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/large-planet-landing-20260910/
```
