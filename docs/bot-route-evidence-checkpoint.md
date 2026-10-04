# Mission execution and projectile-response checkpoint

## Problem and resulting scope

A supported destination estimate did not reliably become a completed capture
trip. Following those failures exposed late route delivery, mismatches between
surveyed and actual hatch positions, interrupted powered crossings, and losses
after escape while travelling to the next destination. This branch records the
bounded execution candidates and the physical evidence needed to evaluate them.

The subsequent [review and integration record](bot-checkpoint-integration.md)
documents compatibility with the newer v14-v16 work on `main` and PR validation.

It extends the earlier [planning checkpoint](bot-planning-checkpoint.md) and
[mission-value work](capture-mission-value.md). It closes the current local
investigation with [one conservative projectile-response selector](projectile-response-selection.md).
It does not promote a new interactive policy or change device defaults. Every
experimental path retains an explicit option, original mission priorities and
the recorded predecessor comparison.

The closing comparison passed all 52 full-match audits. Its selector preserves
two previously successful braking continuations and rejects the recorded
clear-entry braking regression, but all 16 new candidate matches remain exactly
equal to their controls: one short warning is rejected and fifteen cases never
warn. The decision is **retain as experimental**. This closes the investigation
without claiming a stronger default bot. The original checkpoint passes 1,103 Rust
tests, 775 Python tests, formatting, strict AI Clippy and both release builds.

## Review map

| Area | Implementation and evidence to review | Limit retained at this checkpoint |
| --- | --- | --- |
| Mission evidence and failed visits | [Earlier current-destination costs](current-neutral-costs.md), [terminal visit accounting](capture-visit-endings.md), [destination retry context](capture-destination-retry.md) | Conditional forecasts and a useful directed switch do not establish stronger match play. Failed and interrupted trips remain in the results. |
| Timely route delivery | [Focused walking routes](focused-landing-routes.md), [requested corridors](requested-landing-routes.md), [local powered routes](local-powered-routes.md) | Declared graph/query work stays under the shared allowance; native sensing and some validation work remain outside it. More completed sorties have also produced worse match outcomes. |
| Actual capture execution | [Powered approach probes](capture-jetpack-approaches.md), [active-flight continuation](active-flight-continuation.md), [actual hatch investigation](actual-landing-route.md), [actual-route recovery](actual-route-recovery.md) | The successful crossing retains its original launch, fuel and expiry. Planning from a historical landing pose cannot authorize an actual hatch route. |
| Exposure and travel after failure | [Pursuit health gate](pursuit-health-gate.md), [initial cover](initial-cover-admission.md), [covered request handoff](covered-request-handoff.md), [capture escape](capture-escape.md), [post-escape travel](escape-travel.md), [transfer speed](transfer-speed.md) | These are optional candidates with retained regressions. Separation, longer survival and approach clearance alone do not prove useful capture or better play. |
| Projectile response | [Threat attribution](transfer-threat.md), [read-only diagnostics](projectile-diagnostics.md), [fixed response trials](projectile-response.md), [ordinary transfers](ordinary-transfer-response.md), [final selector comparison](projectile-response-selection.md) | A circle warning is not a guaranteed hit. Initial braking direction is not a collision or survival forecast. The selector can abstain from a useful response. |

Start with the final selector report for the closing decision, then inspect the
relevant physical execution reports above. The numbered experiment commits,
plans, hashes, failed candidates and archived outcomes are retained deliberately.
They should not be pooled into an independent win-rate estimate. Generated
worlds, swapped seats, repeated configurations and replay-parity checks have
different evidential roles.

## Runtime and evidence boundaries

The rule bots continue to issue canonical controls; candidate controllers do
not teleport bodies, edit velocity directly, grant a capture or renew the
original mission clock. Current support, clearance, actor identity, native
capture/recovery priorities and evidence expiry govern physical execution.

The latest selector lives in the headless `surface_mission_soak` probe. It
selects one existing half-second brake pulse or keeps native controls at the
first eligible warning. A refusal consumes the attempt. Neither repeated
warnings nor a later convenient missile can restart it. Its source calculation
is independently reconstructed from consumed observations and action bytes.
Legacy response modes retain byte-parity checks.

The branch also fixes experimental reporting: the first terminal event closes
a capture visit, jetpack fuel measurements survive the boarding-report boundary,
and reuse accounting includes focused handoffs. These are measurement fixes,
not newly granted physical fuel or capture progress. The reports distinguish
actual landing, exit, claim, boarding and departure from planner admission and
arrival estimates.

Tracked manifests and compressed archives preserve plans, source observations,
outcomes and raw-file hashes. Larger per-tick streams stay at the hashed local
`target/` paths. Reproduction commands in each report require fresh output paths
and its recorded configuration. Frozen implementation and outcome commits remain
separate; correcting an auditor does not erase its original failure record.

## Relation to the remaining bot work

The foundations in the [AI roadmap](https://github.com/aortez/space-wars/issues/14)
and [planning umbrella](https://github.com/aortez/space-wars/issues/81) already
exist. This checkpoint contributes execution and failure evidence to
[#142's policy-quality gate](https://github.com/aortez/space-wars/issues/142).
It does not close that gate or establish current whole-path Picade latency.

After local review and integration, the next broader milestone is one clearly
identified integrated bot compared with retained policies on complete matches,
followed by current device measurements and playtesting. Mission choice,
aggression and response quality must be assessed together without discarding
the losses recorded here. Neural/evolutionary training remains a separate
track, not a prerequisite for that programmed-bot milestone.

This is the stopping point for the present investigation. Additional prediction
models, more response tuning or further seed searches belong in a separately
scoped change. Publication and integration follow separately in the linked
review record; the frozen investigation itself did not push, merge, deploy or
edit GitHub issues.
