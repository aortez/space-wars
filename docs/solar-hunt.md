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

## Results at `1f89ec8`

The full workspace passed **1,105 tests**, with 26 display-dependent tests
ignored in that invocation. Eight example tests also passed: **1,113 total**.
All ten terrain UI workflows passed explicitly under Xvfb, including both
renderers; 82 artifact files were archived. Formatting and Clippy passed, with
existing advisories outside the changed code. The six frozen navigation and
twelve strategy episodes matched. A rendered heat probe verifies the visible
corona and the HUD at 90% hull health and 10%/s exposure.

Desktop and Pi each completed 24 three-minute runs: twelve capture-then-hunt
trials (eight generated, four fixed), eight generated two-bot trials with
3-second Mixed asteroid arrivals, and four additional quiet generated routes.
That is **48 runs / 2.4 simulated hours**, all passing finite-motion, speed and
retained-plus-removed material audits. Reports, traces and 336 Pi frames were
archived before deployment.

| Capture-then-hunt result | Desktop | Pi |
| --- | ---: | ---: |
| Completed runs | 12/12 | 12/12 |
| All planet capture/boarding/departure trips | 11/12 | 11/12 |
| Actual weapon contact after securing all planets | 10/12 | 10/12 |
| Mandatory hunt acceptance cases | 6/6 | 6/6 |

Both seats and both reflections are represented. The fixed trials all scored
contacts. Generated seed 0/P1 began pursuit at 116.68s desktop and 116.62s Pi;
both scored their first per-second recorded hit at 135s. Its desktop opponent
lost a ship and the hunter switched to tracking the pod. The two-bot asteroid
trials caused no ship losses in this set, so they add environmental/mission
coverage rather than additional recovery milestones. Timing was collected with
concurrent runners and compilation; it is not a controlled performance comparison.

### Remaining route work exposed by a solid sun

The two missed hunt trials have distinct causes:

- Seed 2/P2 entered pursuit at 179.18s desktop and 179.70s Pi,
  leaving less than a second for pursuit before the cutoff.
- Seed 7/P2 still struggled to land on planet 0's sunward side and completed no
  departure. The previous successful trajectory went **20.08 units inside the
  sun on desktop and 18.20 on Pi**. The new trajectory's minimum sampled
  clearance was 23.37 and 28.94 units respectively, but local landing selection
  still proposed approaches that repeatedly triggered solar escape. Desktop
  took a small amount of heat damage; Pi retained full hull health. This is a
  newly exposed need for solar-aware landing-site selection, not a successful
  capture or pursuit result.

The extra quiet cases also retain unfinished ground/landing routes at 180s.
Three of four desktop and two of four Pi cases completed all three departures.
Match victory/elimination and broader difficult-ground navigation remain ahead.

## Pi deployment and live check

Yocto completed all 6,608 tasks successfully (21 rerun), with the existing
unvalidated-host warning. The archived image is
`spacewars-image-1f89ec8.ext4.gz`, SHA-256
`ff6524d238b4a654fd520eedcfa75617006e45482000af6a4c19824b5ab4859a`.
The installed `/usr/bin/engine-client` hash was verified as
`1b5a964f96601f25dfaeb044d47acf863dbca7f4d9b602127626af54237ea375`.
The Pi booted slot A, `/dev/sda2`, with `spacewars-kiosk.service` active and zero
restarts. Source implementation: `1f89ec86c17a105a756c4effc16a8a98f7d06989`.

A 180-wall-second live arena duel used the verified raster renderer at scale 2
and 3-second Mixed asteroid arrivals. At 30/75/120/180 seconds, sampled FPS was
48.7/51.9/52.6/52.8 and UPS was 59.6/59.9/59.5/59.8, with zero service restarts.
Screenshots show normal landing, claiming and jetpack ground travel, and the
solar corona during transfer. At 180 seconds P1 was still circling into cover
at neutral planet 0 and P2 was on foot on P1-owned planet 1. This live run did
not reach the post-capture pursuit phase; the headless hunt trials cover it.

The final state is a fresh `spacewars-terrain-arena` round, P1 human versus P2
mission bot, with 8-second Mixed arrivals. It is paused at revision 38; B or
Start resumes. Final UI state, settings, screenshots and installed-build records
are saved with the artifacts above. No branch merge or push was performed.
