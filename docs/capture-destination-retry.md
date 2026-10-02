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

## Results and decision

**Retain the opt-in; leave defaults unchanged.** The preference prevents the
demonstrated value-switch loop from abandoning a useful neutral capture. It
does not solve the blocked enemy approach, and no broader strength gain is
established by the fresh matches.

Implementation, tests, runner and plan were frozen at `0bafeb5`. The preserved
binary is `target/destination-retry/surface_mission_soak-0bafeb5`, SHA-256
`7e93de98e6ded36ac1fe85326c52b967b8a8b2522c09a5468d50fa88cb819a00`.
All 64 runs completed in `target/destination-retry/v1`. The
[results manifest](data/capture-destination-retry-v1.json) contains aggregate
results, checks and two successful-retry context observations. The linked
[compressed full study](data/capture-destination-retry-v1.json.gz) preserves
commands, raw hashes, accepted forecasts, visits, memory and desktop timings.
No policy or trial parameters changed after viewing gameplay results.

| Measure, tested seat | Predecessor | Failure context |
| --- | ---: | ---: |
| Recorded completed capture sorties, 16 runs | 35 | 37 |
| Recorded ships lost | 11 | 11 |
| Recorded completed recoveries | 12 | 12 |
| Fresh wins, 16 runs | 8 | 8 |
| Fresh losses | 8 | 8 |
| Fresh completed capture sorties | 42 | 42 |
| Fresh ships lost | 12 | 12 |
| Fresh completed recoveries | 5 | 5 |

The recorded trajectories include reused worlds and both prior cover settings.
Only two of those 16 pairs change physical outcomes: the same directed value
fixture with cover response disabled and enabled. Each gains one completed
neutral capture. The other 14 recorded pairs are physically identical. Recorded
match outcomes remain six wins, four losses and six unfinished directed runs
per arm; the extra captures do not establish extra wins.

### The directed loop makes progress

| Event with failure context enabled | Cover response off | Cover response on |
| --- | ---: | ---: |
| First rejected value switch back to planet 1 | 7,264 | 6,482 |
| Planet 0 flag claimed | 8,596 | 7,880 |
| Planet 0 sortie departed | 8,824 | 8,108 |
| Initial picker returns to remaining planet 1 | 8,825 | 8,109 |
| That enemy approach still fails | 10,743 | 9,271 |

Both predecessors abandon planet 0 for planet 1 and complete no capture. The
candidate retains planet 0 through its native landing and departure, then
allows a fallback retry of planet 1 when no other unowned destination remains.
The enemy attempt still fails in both cover settings. The improvement is a
completed alternative objective, not a newly feasible enemy route.

The two changed runs record 69 and 84 rejected switch proposals, respectively.
These include repeated evaluations on neighboring control ticks; they are not
153 independent strategic decisions. Their retained traces match the
predecessors until the first rejection. No initial-picker choice changes in
any of the 64 gameplay runs; that shared path is exercised by unit tests.

### Successful retries are preserved

Both previously identified successful retries retain their exact selection,
claim and departure clocks. Material changes are observed before each return:

- In the recorded asteroid/cover-response case, planet 1's failed context is
  revision 9. At the first retained sample showing cleared memory, tick 4,545,
  the planet is revision 10. The return is selected at 12,155, claims at 14,631
  and departs at 14,884.
- In `world2-asteroids0-p1` with cover response, the incomplete-search failure
  is on revision 0 at 19,380. The first retained cleared-memory sample, 19,860,
  shows revision 1. The return is selected at 21,182, claims at 24,162 and departs
  at 24,383.

Those sample clocks are not exact invalidation clocks, and changed material
does not prove why the later captures succeed. The archived observations show
why unchanged-context failure should not be inferred from a same-planet return.
The fallback for a sole remaining target is also exercised in the directed
replays and by a unit test at the native cooldown boundary.

### Fresh matches and verification limits

All 16 fresh pairs are physically identical and their normalized retained
controller traces match for the whole run. The enabled arms record only two
unclaimed capture failures; both records are later invalidated. No initial
choice, switch or fallback admission changes in these fresh worlds. Thus the
unchanged eight wins and eight losses validate preservation on this sample,
not improved playing strength. The four worlds are reused across seats and
asteroid settings and remain correlated.

Fresh eligible travel/capture time is 258,971 ticks per arm; time accumulated
after 20 seconds without progress is 47,390 ticks in each. Recorded eligible
time increases from 265,066 to 267,556 ticks, and time without progress from
210,136 to 212,174; time after 20 seconds without progress stays at 29,423.
There is no demonstrated broad reduction in stalled time.

All 16 disabled recorded replays reproduce their original physical, mission,
progress and controller/planner records. Sensor parity covers 640,776 player
observations. The 64 runs audit 1,399,024 main dispatch ticks, with maxima of four
graph operations and 192 physics queries. Synchronous native sensing remains
outside that quota; these desktop results do not establish Pi frame time.

314 AI unit tests, four physical destination tests, 39 harness tests and 615
Python tests pass (972 total). Formatting, strict AI Clippy with `--no-deps`, and
both profiled and normal release harness builds pass. The new tests cover
native failure recording before task disposal, both selection paths, stale and
post-claim evidence, cooldown boundaries, fallback ordering, context changes,
reset behavior and forecast invalidation.

The next useful work is the remaining enemy approach. This preference can
protect another objective from an unnecessary return, but once every other
planet is secured, the controller still needs to complete that blocked approach.
