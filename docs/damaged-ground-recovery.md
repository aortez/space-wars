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
standing positions, at least two units apart, within the existing 24-unit walk
limit. Eight candidates are checked per scheduled observation, rotating through
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

Final validation and deployment results follow after the runs complete.
