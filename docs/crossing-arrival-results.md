# Destination-footing check: correct arrival, failed recovery qualification

The candidate prevents the false terrain-gap completion and advances beyond
the original stalled crossing. It **does not qualify as a recovery
improvement**: the affected pilot still cannot rebuild, and its previous win
becomes a loss. Keep this branch as an unqualified local candidate; it is not
ready for promotion or deployment.

The [frozen plan](crossing-arrival-plan.md) is complete. All three control
games retain their original actions and physical outcomes exactly. The one
affected game first changes at the predicted false-completion tick, with the
same consumed observation. No candidate tuning, game retries, or audit repairs
were needed after freezing.

## Change and regression coverage

The [controller change](../crates/spacewars-ai/src/jetpack_crossing.rs) adds
one predicate for descending **terrain-gap** crossings: the pilot's feet
must be within the existing one-unit arrival radius of the destination in
the planet's local frame. The existing support, balance, speed and sideways
alignment checks still apply. Vehicle crossings, guidance, deadlines,
planning, physics and bot selection are unchanged.

The [four native fixtures](../crates/spacewars-ai/tests/fixtures/ground-gap-arrivals.json)
are bound to the preceding diagnosis bundle by hash. The new controller tests
accept the three genuine arrivals and reject the source-side landing, also
after world rotations, translations and common velocity shifts. Rejecting the
landing does not extend the existing deadline. Python verification checks the
fixture projection against the original observations and audits completion
footing in both capture and recovery tasks.

## Full-game comparison

All settings, seeds, policies, both actors, shared planning, asteroid pressure
and observer controls match the preceding four games. Each candidate game
runs to its native ending under the same 600-second cap. These are four
selected known cases and zero fresh games.

| Game | Candidate result | Comparison |
| --- | --- | --- |
| World 1/P2 integrated | Loss, tick 13,337 | Exact retention |
| World 3/P1 integrated | Win, tick 21,833 | Exact retention |
| World 3/P1 no-stop | Loss, tick 11,509 | Exact retention |
| World 1/P2 no-stop | **Loss, tick 29,596** | Previous time-limit win at 36,000 |

In the affected case, **39,927 complete actor trace rows** match before P2's
decision at **19,963**. At that tick the consumed observation is identical:
the original reports `Complete`, while the candidate stays in `Descend`.
The first changed movement action is on that same tick. The first three
completed gap flights retain their original completion clocks.

The corrected 121 → 124 gap reaches a valid destination footing at **20,181**,
218 ticks later than the false completion. The pilot's feet are then 0.998
units from the destination, within the existing one-unit contract. It
continues across three additional gaps:

| Gap nodes | Candidate completion | Foot distance to destination |
| --- | ---: | ---: |
| 55 → 62 | 18,562 | 0.072 |
| 85 → 90 | 19,180 | 0.264 |
| 116 → 120 | 19,762 | 0.018 |
| 121 → 124 | **20,181** | **0.998** |
| 150 → 154 | 20,720 | 0.116 |
| 228 → 233 | 22,224 | 0.080 |
| 236 → 241 | 22,548 | 0.126 |

There are eight crossing attempts: seven complete and one is interrupted
before the successful 228 → 233 retry. All seven completion records have
native support, balance and destination footing. The minimum charge across
the audited crossing records is 56.7%; fuel exhaustion is not the observed
terminal failure. Audit `launch_tick` fields mark entry into the `Lift` phase;
the retained actions identify the actual powered input.

## Where recovery still fails

After the seventh crossing, the pilot follows a partial ground route toward
the flag. At **22,965**, replan 14 reports a disconnected route from node 273.
Repeated surveys through replan 37 at **23,655** find essentially the same
closest reachable distance, **14.583 units** from the objective. Two short
partial routes from neighboring node 272 briefly resume walking at 23,209
and 23,509, then return to the disconnected component. They do not establish
a route to the flag. These are failures of the measured graph, not proof that
every possible physical route is impossible.

Both baseline and candidate hit the existing ground deadline at **23,679**,
5,401 ticks after ground recovery starts. The candidate's final task has
37 replans, 15 partial routes and one flight interruption. It completes no
recovery claim, rebuild or boarding. Its three earlier mission claims and
departures remain unchanged.

The candidate then spends 5,917 observed ticks in the blocked mission with
zero movement requests. It dies at **29,596** in an **on-foot world-boundary
impact**, still at 100 health immediately before that impact. Native contact
closing speed is 147.51. The initiating displacement is not identified by
this experiment; the assigned-pod impact stream is not an on-foot motion
probe. Do not attribute that displacement to a particular projectile without
additional native evidence.

| Evaluated P2 measure | Baseline | Candidate |
| --- | ---: | ---: |
| Reported gap completions | 4, including one false | 7, all footing-verified |
| Completed recoveries | 0 | 0 |
| Native rebuilds | 0 | 0 |
| Ship losses | 1 | 1 |
| Pilot deaths | 0 | **1** |
| Round result | **Win** | **Loss** |

The guard fixes an arrival contract, but the selected recovery still fails
and survival regresses. The recorded decision is
`correctness_only_recovery_unproven`, with both recovery-improvement and
survival-retention checks false. It does not justify merging the candidate as
a finished recovery fix or changing the frontier/default bot.

## Next hypothesis

Investigate a bounded fallback when a partial recovery route reaches a
disconnected component short of the flag. The next test should isolate the
node-272/273 route and verify whether a different measured corridor or
recovery objective is available within the existing deadline. Repeating the
same survey and briefly walking between neighboring nodes has not produced
claim access in this case.

Preserve this arrival check as the experimental baseline for that work, with
the old executable retained for comparison. Require native claim, rebuild
and boarding progress plus survival before treating the combination as an
improvement. Extending the timer or correcting a completion counter alone is
not the success criterion. The separate combat/pod-survival and World 3/P2
integration regressions remain outside this experiment.

## Verification and evidence

- Candidate source: `fd054579e8df518b1dcd8cf157a148f7aade1d7d` on
  `bot-crossing-arrival`. Candidate binary SHA-256:
  `a38633e18aacc47271fb3759753a7d01ffde5293467198c4ccef8f97cb14bafa`.
- The baseline executable and all prior archives remain intact. Exactly one
  prior runtime file changed. All **931 frozen inputs** match: 926 predecessor
  inputs plus the new Rust test, fixture, Python runner/tests and plan.
- **559 Rust library/integration tests**, **59 profiled harness tests**, and
  **930 Python tests** pass. Formatting and bot-scoped Clippy pass. The locked
  release build uses Rust 1.89.0 and `sensor-profile`; all commands and logs
  are retained. These are newly executed checks.
- All three controls match every deterministic stream and all non-timing
  report, sensor, physical and planning results. Both actors are audited,
  including voluntary scuttling and final-step native lifecycle state.
- **152,550 dense actor rows** join to the observer without overrides. The
  ground audit covers both capture and recovery scopes. The four games ran
  once each, with no audit resume or game retry.
- Four lossless archives preserve **77 files**, **3,420,666,155 raw bytes**,
  compressed to **448,498,098 bytes**, under
  `target/crossing-arrival/v1/archives/`.

The committed [manifest](data/crossing-arrival-v1.json) binds the
[portable review bundle](data/crossing-arrival-v1.json.gz). It contains the
frozen commands and sources, both baseline and candidate reports, exact first
change, complete crossing/sequence diagnostics, terminal-route observations,
validation logs, archive/member hashes and reproducible export/derivation
scripts. The full raw recordings remain in the local lossless archives.
