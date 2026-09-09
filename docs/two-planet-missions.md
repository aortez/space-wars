# Controlled two-planet missions

This extends the [damaged-ground recovery checkpoint](damaged-ground-recovery.md)
with `material_mission_v1`: a coordinator around the existing capture, ground,
recovery and combat policies. It is a controlled experiment on two fixed
destructible planets, preceding generated-world match integration.

## Playtest

- `spacewars-terrain-travel`: P1 human, P2 mission bot.
- `spacewars-terrain-travel-duel`: two independent mission bots.

Both pilots start with full ships beside separate neutral planets. The bot
prefers another unowned planet, travels there, lands, exits, claims, boards and
departs. It then chooses another unowned planet. When both are owned it patrols
and fights using the existing combat policy; losing ownership creates another
capture objective. The HUD shows the destination and current task. Planet IDs
are 0 and 1, consistent with the existing surface HUD.

The human has the existing flight, weapons, mining and jetpack controls. Launcher
Settings expose asteroid arrivals and strength; the settings persist across
restart. Both planets can receive random arrivals. The shared combat-break
settings continue to apply during patrol. Start pauses/resumes; R restarts the
world and both brains. This scene does not yet have a victory screen or generated
worlds, and retains the laboratory's invulnerable pods/spacelings.

## Boundaries

`MissionObservationV1` adds cheap planet motion, conservative radius, terrain
revision and ownership data around the existing local observation. The target
planet is AI intent. It never changes the physical approach frame, support,
landing eligibility, hatch access or claim/rebuild authority. Detailed surveys
remain local; transit requests no landing survey. Losing a full ship triggers a
fresh pod survey, and a site from another approach frame cannot leak into the
current local observation.

Travel climbs clear, guides toward the destination's outer approach area and
routes around intervening conservative body bounds. It compensates for the
observed shared gravity solve and applies ordinary held controls. This short
transfer uses open-wing guidance capped at a desired 55 units/s; longer-distance
cruise tuning remains a later experiment. Local capture receives a neutral
handoff only after the destination is the actual approach planet and the ship
is slow enough. It retains all material landing and ground-travel gates.

The coordinator records departure after the existing task observes actual claim
and boarding milestones and the ship clears the target's radius by 70 units, or
the existing capture task completes. This lets the next objective take over
before a change of nearest planet could confuse the previous local task.

Ownership changes can cancel a pending destination; site damage is handled by
the local task. A failed transfer has a sixty-second total budget and a
twenty-second progress budget. Failed destinations are deferred thirty seconds
while other objectives are reconsidered. Capture/recovery keep their existing
bounded retries and explicit failure reasons. Ship loss interrupts travel or
capture, delegates to `RecoverShipTask`, and resumes objective selection after
the replacement is boarded and flown clear. A surviving but unreachable ship
can still strand its pilot; this does not introduce a remote rescue shortcut.

There is one shared physics step and gravity solve for both planets and pilots.
The asteroid source samples both retained material bodies independently of
actors; one-planet fixtures retain their original random stream.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 1 --mirror false --seconds 180 \
  --mode quiet --bearing 0 --require-route true --frames true \
  --out /tmp/two-planet-mission
```

Modes are `quiet` (idle opponent and weapon output suppressed), `intercept`
(existing combat bot opposes the mission), and `duel` (two mission bots).
`--asteroid-interval 3` adds Mixed arrivals, distributed across both planets.
`--strike-after-departure true` schedules one physical heavy asteroid after the
first departure to test interruption/recovery; this belongs to the runner and
is not an ability used by the bot. `--bearing` changes only initial launch faces.

Reports separate physical audits, distinct planet departures, later recovery,
blocked/unfinished work and sensor/policy/step timings. A pressure physics pass
does not establish a completed mission. Frames and per-second observations are
retained, with bounded mission event history and explicit reasons for replans.

Final validation and device evidence will be recorded after the runs finish.
