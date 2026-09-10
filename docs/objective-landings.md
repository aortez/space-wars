# Landing near the capture objective

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
Desktop departures take 95.30–101.25 seconds from trial start. The current
candidate completes all eight desktop routes, departing at 39.48–40.48 seconds.
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

The Pi seed 42 unreflected duel reproduces the cramped return. With the bounded
fallback, P1 starts replacement at 106.52 seconds, rebuilds, and records a
completed recovery at 119.75 seconds. It later completes another capture sortie
and resumes transfer. This is a recovery success using a replacement ship,
not evidence that the original hull trap has been solved by walking.

## Validation and deployment record

Final source verification, paired runners and the installed-image playtest are
in progress. Their results will be recorded here before this slice is delivered.

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
