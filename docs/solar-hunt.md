# Solar collisions, heat and post-capture pursuit

The material arena now treats the sun as a solid hazard and follows secured
planets with pursuit of the opponent. This extends the
[parked-ship return checkpoint](parked-ship-return.md) on the integration branch.

## Sun collision defect

Surface ships deliberately excluded `GROUP_BODY` so the old circular planet
colliders could not block excavated ground. The sun belonged only to that group,
so ships and pods passed through it. Spacelings also excluded it. A regression
against the previous implementation reproduces a full ship entering the sun at
60 units/s without contact.

The sun now has an additional collision category accepted by surface hulls,
feet, spacelings and replacement-clearance queries. Its existing body category
preserves ordinary ships, debris and weapon queries. This does not restore the
old circular planet surfaces. Sun contacts never count as support for a planet,
landing, claiming or rebuilding.

## Solar heat

Material combat scenes add a 24-unit corona, drawn in the world and minimap.
Heat rises linearly from zero at its outside edge to 20% of maximum ship health
per second at the sun's surface. Exposure is measured at the full ship's pivot;
the HUD reports hull health and the current loss rate. Leaving the corona stops
heat immediately. There is no additional force or velocity correction.

Heat reaches both occupied and empty ships. Destruction uses the existing shared
breakup/recovery path: an occupied ship ejects its pod; an external spaceling
keeps its identity. Pods and spacelings retain the arena's invulnerability.
Earlier noncombat fixtures and ordinary Spacewars retain their damage rules.

## Mission behavior

`material_mission_v3` keeps capture as its priority. After every planet is owned,
it pursues the opponent's observed world position, using the existing planetary
and solar arc waypoints. Inside combat range, with ground no longer obstructing
the target, it delegates to the existing combat policy. Weapon energy, missile
reloads, firing gates and configured exhibition breaks are unchanged.

During the opponent's pod or on-foot recovery, the bot follows their actual
position and waits nearby for an eligible ship target. Losing ownership creates
a new capture objective immediately. This still is not a match victory or pilot
elimination system. Invulnerable recovery actors are not weapon targets.

A solar escape overrides flight when the next two seconds of linear motion
predict entry into the heat margin. It finishes clearing that margin before
returning to the interrupted task. An initial distance-based guard repeatedly
interrupted safe transfers and landings beside the inner planet; the path check
preserves safe tangential passes and restores the existing mission regressions.
Routing remains local obstacle avoidance, not a global path planner.

## Validation and reproduction

Focused tests cover both seats' ships and pods hitting the sun at 60/140 units/s,
spaceling collision without false planet support, heat falloff and cessation,
occupied/empty ship loss, the HUD, and exclusion of noncombat fixtures. Decision
tests cover pursuit, combat handoff, tracking recovery actors, recapture, solar
escape, safe passes, repeated ticks and cloned state. Two generated missions
must physically capture and then land an actual weapon hit within three minutes.

The mission runner adds `--mode hunt`, which holds weapons off until all planets
are secured. `--require-hunt true` requires a real weapon contact in that mode.
Other modes keep their existing controls. Reports now also retain weapon hits,
damage sources, exposure and the delegated combat policy's telemetry.

```sh
cargo run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 7 --seat 0 --mirror true --seconds 180 \
  --mode hunt --require-hunt true --trace true --frames true --out /tmp/solar-hunt
```

Artifacts, logs, images and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/solar-hunt-20260910/
```
