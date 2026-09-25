# Bounded alternative capture evidence

This extends the [observational capture evaluator](capture-mission-evaluation.md)
with measurements of one alternative neutral planet. Controls and destination
selection remain unchanged. The acceptance boundary is useful comparisons in
ordinary matches, bounded extra work, unchanged local planning, and preserved
physical execution—not a claim of stronger play.

## Measurement and evidence contract

After the controller chooses its action, the evaluator requests the nearest
neutral, flagless alternative in its existing three-destination shortlist.
Only the calibrated three-second claim setting is eligible. There are two fixed
material bearings: the side away from the observed opponent and the arrival
side, offset a quarter turn if they coincide. The request is replaced when the
selected visit, candidate planet, material revision, ownership or claim state
changes. No ground graph is requested.

The host reuses the destination-cover dispatcher and the actual landing-site
helper. It checks retained material under the feet, slope, the full hull,
belly, the normal exit and boarding entrances. The new optional climb check
tests the full hull at 7, 30 and 60 units above the proposed landed pose. These
are **three clearance samples**, not a swept corridor or a simulated departure.
They can reject a blocked sample; passing them cannot guarantee the whole path.
The existing opponent-cover rays remain separately recorded at the measurement
tick. Exposure does not become a success probability or a timing penalty.

A numeric alternative reference requires a measured landing, a return entrance
and all three clear climb samples. It uses the unchanged frozen no-flag phase
medians plus nominal transfer distance/38. This is conditional duration if the
trip succeeds. It does not predict survival, future cover, opponent motion,
solar/arena detours, gravity changes or successful flight. In particular, an
exposed remote site can have a duration reference while combat risk remains
explicitly unmodelled. Existing current-destination evidence rules are unchanged.

Original sample ticks survive repeated delivery; reading a sample does not
renew its age. Samples expire after 30 seconds and are immediately withheld on
incompatible planet/material/ownership/flag/claim state or dirty material queries.
New failed measurements replace older evidence for that site. If the other
sampled site still has compatible historical evidence it can supply the
reference. Exhaustion and missing clearance remain unknown, not impossible.
Remote records always say that live feasibility is unknown. The evaluator's
two-second report expiry is independent of its pending refresh.

There is at most one remote request per actor, with two candidate samples and
the evaluator's existing eight-planet evidence bound. Recovery, landed capture,
finished matches and local landing demand stop remote requests. Retaining a
historical reference does not interrupt a return/recovery commitment or give
permission to switch targets.

## Work and integration

Each physical query asks for fuel before executing. One atomic site check costs
at most 192 queries, including cover and the optional climb checks; exhaustion
discards all partial evidence. Each actor can spend one atomic check per tick.
The oldest candidate refreshes first, no sooner than 30 ticks after its sample.
There is no accumulated credit or guaranteed remote progress under local load.

In the headless incremental-planning mode, local landing/objective jobs spend
first. Alternative work uses only the remaining **existing** physics allowance
and cannot take a local actor's atomic-check slot. The evaluator subsequently
uses leftover graph allowance, at most four units total and two per actor.

The live client still uses its ordinary synchronous v9/v10 tactical sensors.
It gives the remote-only dispatcher a shared cap of **384 queries per tick for
the two seats**. Those synchronous sensors are outside this cap; this is not a
whole-bot query or wall-time budget. Remote request construction and dispatch
are included in `mission_planning_*_ms` diagnostics; the cap meters operations.

Both hosts register the observational demand after controls. The headless host
also waits until after writing the controller trace. The client exposes
`mission_alternative_survey` counters beside the existing evaluator JSON. No UI
choice or additional brain is introduced. Headless surveying is opt-in through
`--survey-capture-alternative true`, requiring mission evaluation and live
planning. Existing escape-probe demand takes precedence over this survey.

## Validation and results

The matrix uses two fresh seeds derived from
`native-capture-alternative-v1:0` and `native-capture-alternative-v1:1`, v10/v11
seats swapped between worlds, asteroids off/every three seconds, and 600-second
match limits. Both sides run the evaluator; only alternative surveying changes.
There are four on/off pairs, eight complete matches, with all attempts retained.

Remote work reserves token IDs from the existing allocator without changing its
queue cursor. Consequently raw diagnostic IDs differ. The trace comparison
normalizes only present IDs at
`observation.local.objective_evidence.generation` and
`mission.capture.acquisition.generation`; final mission telemetry uses the same
acquisition exclusion. Missing IDs, all timestamps, requests, payloads, actions
and every other field must match. Raw hashes are retained. The independent
reviewer's separate scan confirmed equality of all other fields across 242,126
paired rows. The native synchronous v9/v10 client also passed a paired physical
round with zero versus normal remote fuel, matching pilot observations and
complete controller telemetry on every step.

All eight matches finished with healthy physical audits, covering 67.26 simulated
minutes. Local allocations match in all four pairs (51,549 allocation rows);
global charge totals equal their per-job sums and remain within the configured
quota. The compact record is
[capture-alternative-survey-v1.json](data/capture-alternative-survey-v1.json).

| World / asteroid interval | Baseline reports comparing multiple numeric destinations | Survey-enabled reports | Numeric alternative records | Physical checks / queries |
| --- | ---: | ---: | ---: | ---: |
| 0 / off | 0 | 15 | 165 | 193 / 12,215 |
| 0 / 3 s | 0 | 16 | 145 | 194 / 12,278 |
| 1 / off | 0 | 495 | 594 | 198 / 12,410 |
| 1 / 3 s | 0 | 494 | 618 | 222 / 13,945 |

There are **1,020 reports comparing multiple numeric destinations**, of which
564 cover the entire shortlist. Every complete comparison still prefers the
current destination. These repeated reports are correlated observations, not
1,020 independent decisions or evidence of better play. The important change
is that ordinary matches now supply comparable evidence; the nearest neutral
alternative need not be preferable.

The 807 checks used 50,848 extra physical queries: mean 63.01 and maximum 65 per
check, below the 192-query cap. These runs never deferred or exhausted remote
fuel; focused tests cover zero/small budgets and competition from local jobs.
Landing checks all succeeded in this small matrix, while sampled climb blockage
can still withhold a cost. The measurements do not establish coverage of arbitrary
terrain. Combined active local/remote dispatch averaged 0.49–0.62 ms on this
desktop, maximum 4.54 ms. Runs used two workers alongside tests; these are
workload measurements, not isolated timings or a Pi frame-time guarantee.

First numeric current-visit coverage rose from 31 to 34 of 55 visits. Of those
34 forecasts, 20 completed, 13 were abandoned and one was unfinished. Completed
median absolute error was 6.13 seconds, maximum 96.93 seconds. Earlier evidence
does not fix the reference model's inability to predict stalls or combat.
Alternatives have no executed outcome and are excluded from trip-error claims.

The full Rust workspace passed 1,916 tests with 47 existing ignored tests. The
additional native paired-client regression passed separately. AI all-targets,
all-features Clippy passed with warnings denied. Targeted tests cover bounds,
clone/reset, age/provenance, dirty queries, ownership and material changes,
failed/partial measurements, a clear landing with obstructed climb, and local
priority. Python accounting tests cover partial comparisons and reject trace
changes beyond the two declared ID fields. Independent review found no remaining
runtime blocker and prompted the explicit client-budget distinction above.

## Reproduction

```sh
cargo +1.89.0 build --locked --profile ci -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-capture-evaluation.py run --survey-alternatives \
  --binary target/ci/examples/surface_mission_soak \
  --out target/capture-alternative-survey/new-matrix --workers 2
```

The output directory must be new. For an uncommitted source tree, include new
files in `git diff HEAD` before starting (for example with `git add -N`). The
runner saves the source patch, binary hash, commands, full compressed controller
traces, remote measurements, allocation records and evaluator reports. Analyze
existing runs with `analyze --out <directory>`. Local artifacts from this slice
are retained in `target/capture-alternative-survey/`.

## Next boundary

The evidence can support a deliberately limited, opt-in destination-selection
experiment. It does not yet justify general mission utility or success claims.
Keep unknown alternatives unknown, preserve return/recovery commitments, and
compare any behavioral change against the unchanged controllers. Flagged
destinations still need actual ground round-trip evidence; combat utility and
powered routes remain separate work. No model constants were fitted here.
