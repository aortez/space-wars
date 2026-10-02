# Destination failure context experiment

The [destination-return audit](capture-destination-revisits.md) found that a
30-second pursuit can consume the existing destination cooldown. Both initial
selection and value switching then reconsider the failed planet without its
failure details. Some of those retries succeed, including a return immediately
after an incomplete cover search. This experiment prefers alternatives without
treating a failed attempt as permanent unreachability.

## Opt-in contract

`--destination-retry-seats none|0|1|both` defaults to `none`, requires v13 for an
enabled seat, and reports profile `destination_failure_context_v1`. It is
independent of the cover-response option. Ordinary bot settings are unchanged.

Only an unclaimed native capture failure observed on its actual failure tick
records memory. Bind it to the current visit, capture start, local planet,
material revision, ownership and flag identity. Retain the failure reason,
site, acquisition evidence, cover replans and any bounded cover-search record.
Transfer failures, recovery and leaving an approach frame do not create new
capture evidence. Memory survives those transitions and pursuit; an episode
reset retains the option but clears learned state. At most one current failure
is retained per observed planet.

A shared preference applies after ordinary eligibility/cooldown checks in both
the initial picker and destination-switch gate, including experimental probes:

1. Prefer eligible destinations without a failure in the same context.
2. If all eligible destinations have failed, prefer incomplete searches,
   execution limits or unclassified failures over observed cover constraints
   and the native fresh-ground-route failure.
3. Within a failed category, retry the oldest failure first. Native distance and
   local-frame preferences break remaining initial-selection ties. A switch
   still needs its original accepted forecast, margin and commitment gates.

Material revision, ownership or local flag identity changes invalidate the old
context; a missing or secured destination also clears its record. Ordinary
planet motion does not. These changes permit reassessment and do not certify
that a new route is safe. A newly published cost certificate or elapsed time
alone does not erase memory. Current local landing and ground controllers still
validate every physical action.

The fallback permits retries when alternatives are also failed, owned or still
deferred. Each attempt retains the existing capture limits and 30-second native
cooldown; this adds no global retry cap and does not guarantee eventual success.
Pursuit, recovery, solar safety, descent commitment and planner dispatch budgets
remain unchanged. No new sensors or route requests are introduced.

Telemetry retains failure records and cumulative counts across capture tasks.
`first_effect_tick` marks only an actual initial-choice change or rejection of
an otherwise admissible switch/probe. Merely remembering a failure or admitting
an unchanged fallback retry is not a behavioral intervention. The last rejected
choice and last admitted retry retain their originating failure.

## Frozen validation plan

Freeze implementation, tests, plan and runner before viewing candidate gameplay.
Preserve commands, binary and source hashes and any failed validation attempt.
Do not fit priorities, limits or seeds after seeing outcomes.

- Replay both cover-response arms of the five recorded trajectories from the
  completed cover-response study, plus `world2-asteroids0-p1` (successful retry),
  `world1-asteroids0-p1` (ground timeout) and
  `destination-p1-bearing-0.8` (only remaining destination). Each source gets a
  destination-memory-disabled and enabled replay: 32 runs. The original cover
  setting and every gameplay argument remain fixed within each pair.
- Require disabled physical/mission/progress reports, controller/planner streams
  and sensor counters to reproduce the frozen source. Added trace output is
  diagnostic only. Check candidate retained trace parity before the first
  persistent recorded intervention, using world ticks. In the absence of an
  intervention, require whole-run physical and normalized mission parity.
- Run 32 new finished matches: SHA-256 seed namespaces
  `destination-failure-context-v1:0` through `:3`, asteroid intervals zero and
  three seconds, both tested seats, and both memory settings. Alternate arm
  order. Both arms keep the published-flag/current-neutral cost experiments,
  cover response disabled, and the same v10 opponent. The four reused worlds
  are correlated, not 16 independent world samples.

Audit option identity, visit endings, failure evidence, counters and original
shared dispatch quotas. Retain all failures, wins/losses, completed sorties,
ships lost, recoveries, progress and separate desktop timing. Assess the two
successful retries from the previous audit alongside the repeated-return cases.
This experiment does not authorize a default promotion or a Pi performance
claim.

```sh
python3 tools/validate-destination-retry.py \
  --study target/cover-response/v2 \
  --binary target/destination-retry/surface_mission_soak-COMMIT \
  --out target/destination-retry/v1
```
