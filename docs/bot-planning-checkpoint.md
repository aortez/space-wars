# Powered capture planning checkpoint

This checkpoint extends the already merged v10 planner with experimental v11
planning for a complete landing, enemy-flag capture and return that can include
a jetpack crossing over the parked ship. It also records the physical fixes,
bounded work and comparisons needed to evaluate that action. It closes this
branch before general mission selection or deeper strategic search.

## What is available

| Area | Delivered | Runtime scope |
| --- | --- | --- |
| Access and physical movement | Boarding through either hatch; corrected CCD velocity for moving kinematic ground | Shared gameplay/physics |
| Capture execution | Own-flag handoff, retained joint endpoint, guarded walking and powered crossing execution | Capture controllers; v11-specific walking/crossings where documented |
| Powered planning | Proposed hull crossing, outbound/return flight forecasts, moving-planet validation | Opt-in `material_mission_v11` in headless comparisons |
| Incremental evidence | Local dependency checks, partial positive route delivery and current landing clearance | Opt-in live planner profiles under the existing shared quota |
| Site acquisition | Native reasons, thirty-second first-site deadline, local hold and mission deferral | Explicit `bounded_site_acquisition_v1` option |
| Mission evidence | Phase timing, interruption-risk estimates and short successor comparisons | Diagnostic tools and opt-in probes; no general strategic selector |

Launcher Planner remains v10. Experimental v11, live survey profiles and
bounded acquisition require explicit runner settings; this checkpoint does not
silently promote them. The retained policies and disabled-option comparisons
remain available. Headless examples report the selected policy and experiment
configuration rather than using wall-clock speed to choose different plans.

## Review guide

Start with [powered landing routes](bot-jetpack-landing.md) and
[moving flight validation](bot-moving-flight-planning.md), then the
[route handoff](bot-landing-survey-handoff.md) and
[early positive candidates](bot-early-objective-candidates.md). These explain the
measurement/validation boundary and why a finished search is not permission to
land or launch.

The [moving-ground CCD correction](moving-ground-ccd.md) is the shared physics
change: the vendored Rapier 0.34.0 source has a focused velocity-writeback fix.
The [vendor notes](../vendor/rapier2d/SPACEWARS_PATCH.md) identify its upstream
checksum, exact modification and removal condition. The vendor import accounts
for much of the patch size; it is not a replacement physics engine. The
[claim handoff](bot-claim-handoff.md), [return completion](bot-return-completion.md)
and [outbound walking](bot-outbound-walking.md) notes distinguish controller
changes from that physics fix and retain their regression evidence.

The last implementation is [bounded acquisition](bot-bounded-acquisition.md).
Seven known paired cases preserve disabled behavior; sixteen fresh matched
configurations keep their winners, reduce aggregate waiting and add one
completed trip. A known later recovery impact remains a survival regression.
The option therefore stays experimental, with the controlled long-wait success
and exact failing seed retained for follow-up.

The many timing/risk documents and Python tools are offline evidence, not a
claim that the live bot now predicts or optimizes every complete mission. Their
frozen profiles, source hashes, unsupported cases and rejected experiments are
intentional parts of the comparison record. Physical milestones and match
outcomes take precedence over improved diagnostic scores.

## Follow-up boundary

Future work can use the captured evidence to compare missions by expected time,
survival and ownership under a bounded budget. Before promotion, isolate the
known recovery impact and compare acquisition deadline-only versus hold
guidance. Remote transfer, uncertainty before visiting a planet, unsupported
powered/long-ground costs and the effect of combat still need explicit handling.
Broader strategy and any default-policy promotion should be separate changes
with their own comparison criteria and Pi playtesting.

The [long-term design](design/budgeted-bot-planning.md) remains the roadmap.
