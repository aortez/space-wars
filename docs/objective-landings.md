# Landing near the capture objective

The deferred failures have a [resumption guide](landing-investigation-guide.md)
with exact cases, commands, evidence paths, rejected hypotheses and regression
checks. Work is moving to complete material matches.

This slice builds on [usable landings](usable-landings.md). It selects a landing
with a measured round trip to an existing enemy flag and gives a cramped
spaceling a bounded way to resume recovery. Ordinary-match outcomes and promotion
from the material arena into Spacewars remain later milestones.

Current policy identities are `material_mission_v7`, `tactical_sortie_v10`,
`ground_navigation_v10` and `recover_ship_v9`. The touchdown controller remains
`material_landing_v3`; historical V1 policies retain their existing behavior.

## Observations and control

The new `LandingObjectiveSurvey` reads surviving material in the planet's local
frame. It forecasts the proposed landed hull and feet, excluding the current
position of that same ship. Other ships and debris remain obstacles. A full
512-node ground map supplies walking and jumping edges. Gravity for prospective
jumps is measured at the flag, including other planets and the sun.

At most eight proposed sites are checked: four nearest the flag, then sheltered
alternatives and remaining nearby sites. Both hatch-to-flag and flag-to-hatch
routes must complete. Missing, partial, stale, one-way or malformed measurements
cannot authorize a candidate. Scoring combines flight approach, surface travel
and cover; a short exposed walk cannot outweigh shelter from a nearby opponent.
The map forecasts outer ground, not caves or arbitrary excavated interiors.

The observation is refreshed every thirty physics ticks, with the two pilots
staggered. Neutral planets without an enemy flag incur no objective survey.
The actual touchdown pose receives a separate route check before exit. Changes
to the flag invalidate the objective immediately. A material revision triggers
fresh route validation; a distant edit alone does not discard a still-usable
route. All landing, hatch, support, claim and ownership permissions continue to
come from the existing physical gameplay rules.

The forecast currently excludes jetpack edges. Actual ground navigation still
uses its measured jetpack corridors. This deliberate first boundary can reject
sites a human or an already-landed bot could traverse with a jetpack; it is not
proof that those sites are physically unreachable.

## Cramped return recovery

The previous Pi duel left a supported spaceling under its own hull, farther
than three units from any upright ground-map node. It was balanced, but the
upright capsule did not fit. An ordinary route search repeatedly reported
`NoStartFooting`.

Fresh posture observations now measure upright headroom and short movements
along the actual support contact. The bot can attempt a measured partial step
using ordinary left/right input. Each attempt is bounded by eight seconds and
six units; a retained step expires after fifteen ticks or a material change.
The original ninety-second ground-task deadline still applies. A regained
measured route resumes normal navigation.

The tight Pi case does not reliably escape through short steps alone. After an
unsuccessful hatch-return attempt, a typed `NoStandingRoute` failure allows the
existing ship-replacement fallback. Fresh headroom and ground evidence must
still confirm the trap. Regained footing, transfer readiness, lost support or
stale/changed material cancels the hold. Replacement uses the same three-second
scuttle chord and physical rebuild actions available to a human. It does not
move the actor, grant ownership or spawn a ship through a controller API.

## Controlled trials and findings

`surface_flag_soak --survey-landing true --mode capture --jetpacks true` starts
the attacker through normal takeoff after the defender physically lands, exits
and raises its flag. Quiet trials omit the combat target observation while
retaining the defender's physical hull. `--landing-threat true` retains that
observation to exercise the separate cover problem. These are scripted initial
conditions, not a full autonomous match or a weapon duel.

Eight quiet approaches cover seeds 7/42, both seats and initial offsets ±0.8.
The baseline controller completes eight desktop routes and seven Pi routes.
Desktop departures take 95.30–101.25 seconds from trial start. The final policy
completes all eight routes on each platform. Desktop departures take
39.48–40.48 seconds; Pi departures take 39.48–40.25 seconds.
Completion requires both observed physical departure and the capture pilot's
completion milestone, not merely ownership or takeoff.

New approach edits remove real material two seconds after approach begins.
Destroying the flag footing completes all four desktop cases. Cratering the
selected landing area completes one of four. The other three retry obstructed
or unsettled touchdowns, exhaust their accepted ground-access alternatives,
and fail within the original 180-second trial. An exploratory survey of all
64 sites produces the same outcome: the accepted walking/jumping alternatives
are only the two neighboring sites in those cases. Increasing sampling alone
does not resolve this limit.

The exposed controlled approaches also remain incomplete: the baseline captures
8/8 but completes 0/8 full sorties; the candidate captures 6/8 and completes
0/8. This capture regression remains explicit. It motivates adding prospective
jetpack routes and improving the cover/ground-access tradeoff before claiming
reliable contested landing under fire.

The current desktop 25-case mission matrix preserves the preceding neutral
capture/hunt and quiet-route results. Seven of eight generated asteroid duels
visit and depart more distinct planets; the remaining reflected seed 42 case
still has zero recorded departures. Distinct departures describe travel and
capture activity across both bots, not wins or completed matches.

An intermediate Pi seed 42 unreflected duel reproduces the cramped return.
With the bounded fallback, P1 starts replacement at 106.52 seconds, rebuilds,
and records a completed recovery at 119.75 seconds. It later completes another
capture sortie and resumes transfer. This is a recovery success using a
replacement ship. In the final policy, retaining still-valid routes after
material revisions changes the duel trajectory: P1 avoids that trap and departs
planet 2 at 108.68 seconds with its original ship. The intermediate reproduction
and final trial are separately archived; short walking alone did not solve the
tight trap.

## Validation and deployment record

Implementation checkpoint: `37106c6`. Source-file hashes recorded before the
runner builds match that checkpoint. The baseline is `7b57342`, whose production
implementation is `7726c09`. Baseline runners use an isolated Cargo target and
only the physical launch-fixture overlay; an initial build contaminated by a
shared target cache was rejected before its trials and is archived separately.

The workspace passes 1,128 tests, plus eight example tests and one mission-metric
test (1,137 automated tests total). All ten explicit terrain UI workflows pass,
with 32 retained screenshots across raster and vector renderers. Formatting,
Clippy and the six navigation/twelve strategy frozen baselines pass. Clippy
continues to report existing repository warnings.

| Final trial family | Desktop | Pi |
| --- | --- | --- |
| Quiet contested landing, capture and departure | 8/8 | 8/8 |
| Flag removed during approach | 4/4 | 4/4 |
| Landing area cratered during approach | 1/4 | 1/4 |
| Capture itinerary reaches pursuit within 180s | 12/13 | 12/13 |
| Same itinerary reaches weapon contact within 180s | 11/13 | 11/13 |
| Additional quiet generated itineraries | 4/4 | 4/4 |
| Asteroid duels with more distinct departures than baseline | 7/8 | 7/8 |

The full 25-case mission matrices pass all physical/material audits on both
platforms. All sixteen edited approaches also pass those audits, including the
six incomplete crater cases. A failed mission remains a failure even when its
physics audit passes.

Objective-refresh observation p95 ranges from 4.63–8.03ms on desktop and
14.62–19.95ms on Pi; maxima are 8.41ms and 21.46ms. These measurements include the
whole observation on refresh ticks, not only the added route calculation. The
paired headless trials run concurrently (four desktop workers/two Pi workers).
They do not establish rendered frame rate. Some Pi refreshes exceed one 60Hz
frame budget; the staggered twice-per-second cadence limits their frequency.

The existing twenty controlled ground/recovery trials complete on each
platform: 40/40 accepted. One old desktop expectation incorrectly required a
blocked P1 two-cut route. Both the baseline and current policy physically
complete it with two measured jetpack crossings at the same recorded ticks.
The runner now requires completion for both equipped pilots; the original
failed assertion and confirming baseline reproduction remain archived.

The final families comprise 122 three-minute trials, or 6.1 simulated hours.
All 122 physical/material audits pass. The six cratered-approach failures above
remain failures of mission completion. Intermediate candidates and baselines
are additional investigations, excluded from that final total.

## Verified Pi image and controller scene

The image was built with the five previously accepted Yocto layer revisions,
pinned in the archived configuration. It was deployed to `spacewars.local` on
slot A (`/dev/sda2`). Independent checks before and after the live run match the
installed client to the extracted image binary:

```text
source: 37106c649cd89350ec1f5b75e083d74af9a99234
client: a2f3d6709507ad51df17d4a3bb409f9a68f72df6113ea0682b3c1a510db1af27
image:  ebe4579c047e8f39c846f115b98938714d4df544fb32e3ca1dc931cde851dffa
```

The three-minute rendered duel used seed 42, Mixed asteroid arrivals every
three seconds, the raster renderer and 2× scale. Four screenshots at 30, 75,
120 and 180 seconds recorded 48.3–59.2 FPS, 59.8–60.2 updates per second and zero
service restarts. P1 raises a flag at 30 seconds and approaches another planet
at 75 seconds. Both pilots are airborne at 120 seconds. At 180 seconds P1 is
departing its owned planet toward planet 0, while P2 settles there. The previous
persistent on-foot hull trap does not appear in these captures. This is one
successful operational check, not proof that every route or match completes.

A fresh human-P1 versus mission-bot-P2 material arena was then restarted and
paused, with Mixed arrivals every eight seconds. The installed hash, service
and restored UI state were checked independently. The run wrapper reported exit
143 after its log recorded all captures and restoration; its cause is unknown.
Independent verification passed and the complete evidence is retained.

Before reboot, the Pi trial archive was checked against 152 local report/trace
files and the final runner hashes. The archive contains 98 reports and 54
traces, including prior candidates; those are separate from the final 122-trial
count. The desktop runner uses Rust 1.89.0, the Pi runner 1.94.1 and the Yocto
image 1.94.0. The Pi runner links against a frozen installed `libm`; the rendered
image has its own verification above.

## Reproduction

A quiet contested approach can be reproduced with:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_flag_soak -- \
  --seed 42 --seat 0 --offset -0.8 --mode capture --jetpacks true \
  --survey-landing true --landing-threat false --edit none \
  --expect complete --out /tmp/spacewars-objective-landing
```

Use `--edit flag` or `--edit crater` for approach destruction, and
`--landing-threat true` for the exposed variant. The original
`tools/run-ground-navigation-trials.py --jetpacks` suite retains its existing
physical launch setup. The new `--survey-landing` option belongs to the example
runner above.

The automated contracts cover round-trip selection, shelter, observation
freshness, ownership/material changes, actual touchdown access, bounded cramped
movement and cancellation of ship replacement. Physical flag/return trials keep
claims, destruction, pod recovery, rebuilding and departure in the shared step.
The mission runner separately records observation cost on objective-refresh
ticks, since their thirty-tick cadence can hide them from overall p95 timing.
`--probe-ground-start true` runs diagnostic controls in cloned worlds outside
the main trial's timing and state.

Artifacts, including baseline fixture overlays, rejected exploratory builds,
reports and source manifests:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/objective-landings-20260910/
```
