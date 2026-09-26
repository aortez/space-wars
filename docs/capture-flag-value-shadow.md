# Historical flag evidence in the value comparison

This slice adds `capture_flag_value_shadow_v1`, an observational comparison of
an existing v13 report with the same report supplied with historical enemy-flag
landing/walking costs. The ordinary evaluator, controls and survey scheduling
remain unchanged. Native v13 matches expose it in `mission_flag_value_shadow`;
the headless runner enables it with `--shadow-capture-flags true` alongside
`--survey-capture-flags true`. It is not a new selectable bot version.

## Admission and interpretation

Only an explicitly unmeasured surface cost for an enemy alternative can be
filled. Existing local evidence and unrelated rejection reasons are retained.
The route must be complete, walking-only, and within the existing phase
calibration. Both paths and the landing/hatch/climb measurement come from one
sample. For two eligible sites, the cheaper walking reference wins, followed
by newest source and bearing as deterministic tie breaks.

The identity checks retain the actual source objective and verify the actor,
request generation, bearing, ownership, terrain revision, radius, flag position,
range and claim calibration. Baseline dependencies retain flag identity at the
comparison source, independently of the current observation. The strict endpoint
interaction predicate is checked again. The local publication certificate must
be successful; the old circular diagnostic cannot confer acceptance.

The temporal contract is:

```
request generation <= survey source <= survey completion = validation
                   <= baseline source <= baseline completion <= admission
```

Source age is at most 1,800 ticks at admission. This never renews the survey's
source or publication timestamp. Baseline reports are at most 120 ticks old.
Completion checks both ages again and withholds both rankings if delayed work
crosses a limit. Each report records the immutable baseline, augmented report,
admission decisions and source/admission/completion times.

Publication validated geometry at survey completion. The later comparison is a
**historical-evidence counterfactual**, not newly certified geometry at the
baseline or admission tick. Queued reports are immutable and completed reports
remain historical even if the current mission changes. Cover, opponent readings,
survival and live feasibility remain unmodelled. The frozen baseline transfer
source, ownership value, remaining match time and current landing progress are
retained; unsupported transfers stay unknown. A changed ranking is not an
accepted destination switch or a forecast of a better match outcome.

## Bounded work

Each actor holds at most one pending and one completed report, with at most two
sample admission records. Submission is no more frequent than once per 60 ticks
and cannot repeat a baseline source. The existing evaluator reruns at most three
candidate calculations and one comparison, each charged as one graph step.
Dispatch is last, after ordinary evaluation and flag surveys, with only the
remaining shared 4 graph / 384 query allowance. It issues zero physical queries.
Observation, bounded report copying and trace serialization are outside graph
fuel; construction and dispatch timings are reported separately. This is not a
whole-bot CPU budget.

When the only newly usable publication is newer than the baseline, admission
waits for an ordinary evaluator refresh without consuming the cadence slot.
It neither includes future evidence nor requests extra baseline work. The
`deferred_source_observations` count measures waiting observations, not distinct
survey sources or independent decision opportunities.

## Predeclared validation

Unit tests cover successful ownership comparison, unchanged baseline inputs,
request/source identities, future and stale evidence, incomplete and powered
routes, cover exclusion, partial quotas, delayed publication, reset, unsupported
transfer and match-clock limits. The native three-minute observer parity test
also includes this new shadow path.

The replay uses all five previous local-validation conditions: the fixed v10/v13
regression and both generated v13/v13 worlds under quiet and three-second asteroid
pressure. These are engineering regressions, not new strength samples. The full
plan is written before execution, all matches must finish, and existing controls,
evaluator bytes, physical/mission outcomes, survey samples and per-tick survey
work must remain exact. Every shadow baseline is reconciled against the original
evaluation log, and every used source against the raw survey log. Added graph
work must fit the remaining allowance on every tick. Unknowns and cases with no
usable evidence are retained.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-flag-value-shadow.py \
  --reference target/capture-flag-survey/local-final \
  --out target/capture-flag-survey/value-shadow-final
```

Use a new output directory; the runner refuses to overwrite a study. Results
will distinguish admitted references, distinct surveys, newly numeric candidate
costs, complete comparisons and changed historical rankings.

## Initial replay and source scheduling

The [first record](data/capture-flag-value-shadow-initial-v1.json), at `e13ec54`,
preserved all five trajectories and existing survey work. It produced 38 reports,
15 admitted references to one distinct survey and no numeric alternative totals
or complete rankings. Every admitted alternative required an unmodelled moving
body transfer detour. Shadow work totaled 129 graph steps and zero queries.

The other generated positive exposed a scheduling gap. Its survey completed at
tick 5950; admission at 5951 used baseline source 5948, correctly withholding
future evidence but consuming the one-second cadence slot. Baseline source 5952
completed at 5953. The planet's revision changed from 14 to 15 at 5965, before
another slot. The explicit source wait described above fixes this sampling gap
without requesting more baseline work or relaxing the temporal rule. The same
five cases are replayed, rather than selecting just this case.

The initial `flag evidence unavailable at comparison source` reason included
one future publication and 44 encounters with two unpublished negative surveys.
Those 44 are not future samples. The final version reports unpublished evidence
separately. Independent review also strengthened the audit to reject dropped
shortlist members, altered phase constants/value units, repeated baseline
sources and unreconciled graph work. All five initial raw runs passed those
stronger checks as well.

## Final replay at `194270e`

All five matches completed, totaling 110,913 ticks / 30.81 simulated minutes.
The [final record](data/capture-flag-value-shadow-v1.json) preserves the full
plan, commands, runtime/binary/report hashes, raw-log hashes and audit accounting.
Controls, ordinary evaluator bytes, physical/mission outcomes, survey samples
and all earlier per-tick allocations are exact against the local-validation
reference.

| Condition | Completed shadow reports | Used references / distinct surveys | Added numeric alternative totals |
| --- | ---: | ---: | ---: |
| v10/v13 regression | 0 | 0 / 0 | 0 |
| World 0, quiet | 0 | 0 / 0 | 0 |
| World 0, three-second asteroids | 1 | 1 / 1 | 0 |
| World 1, quiet | 37 | 15 / 1 | 0 |
| World 1, three-second asteroids | 0 | 0 / 0 | 0 |

Both usable v13 publications now enter the comparison. World 0's positive is
admitted at 5954 using baseline source 5952 after one deferred observation; its
transfer requires an unmodelled planet detour. World 1 uses its single positive
in 15 comparisons, baseline sources 3816–4685, each requiring an unmodelled
moving-body detour. These are 16 uses of two surveys, not 16 independent trips.
There are **zero newly numeric alternative totals, complete value rankings or
changed preferences**. Surface costs alone do not establish a comparable trip
or stronger bot. The regression's two successful surveys belong to v10, which
does not enter this v13 shadow.

The other 44 admissions are repeated encounters with two unpublished negative
samples. Their separate rejection reason is now explicit. Every completed
report is tied to an exact original evaluation and each used route to its raw
survey; phase constants, value units, full shortlist, source uniqueness and
graph work are audited independently of the Rust implementation.

Added work is **129 graph steps and zero queries**, fully reconciled to completed
reports, with no pending work at the end of any run. Combined per-tick graph use
never exceeds four; ordinary physical-query allocations remain identical. The
weighted construction and dispatch totals are 12.37 ms and 2.56 ms across all
five desktop runs, dominated by idle observations. Concurrent tests/builds, sparse
positive coverage, excluded trace IO and non-isolated measurement make these
diagnostics unsuitable as a Pi FPS or worst-case CPU claim.

Local validation passed 164 AI tests, 347 Python tests, formatting and strict AI
Clippy. The native three-minute parity test now uses the quiet regression seed
and asserts that the shadow actually admits a survey while ordinary controls,
evaluations and physics remain exact. Earlier physics/scenario validation is
recorded in [local publication](capture-flag-local-validation.md).

Independent review caught and resolved baseline drift during a pending refresh,
a silently ignored CLI combination and gaps in the result auditor. Regressions
cover pinned source freshness, source expiry while waiting for graph work,
two-actor allocation/deduplication and waiting for a new baseline without spending
the cadence slot. The initial study remains available rather than being replaced
by the improved replay.

## Device validation

Runtime `194270e` was deployed to **sw-picade.local** with the application-only
updater. Installed client hash:
`1a62a026ddecc6cb193cf771eacd9bf425729cb4246e778582055339b1b1db4f`.
The CLI remains
`0d33f4d82a80df22cec0a56d74a3903d4fe05fe389100c2b174f752f4a4ca1f3`.
The updater verified both binaries and the service was active with zero automatic
restarts. P1 v10 / P2 v13 autoplay settings were retained, and a fresh automatic
match exposed the separate observational shadow model in status. That early live
sample had no completed shadow comparison; the positive-consumption check is
the native parity fixture, not a claim about that device sample.

Build/deployment/status evidence is under `target/capture-flag-survey/` in
`shadow-final-pi-build.log`, `shadow-final-deploy.log` and
`shadow-final-pi.status`. The build's host-distribution validation warning is
pre-existing; compilation and packaging succeeded.

## Next investigation

Keep this as a shadow until a complete comparison has physical support. The
quiet generated regression (seed `3491156488288037499`) supplies a concrete
starting point: P1's planet-1/bearing-6 survey, source 3471 and publication 3814,
and the subsequent reports containing its admission. The raw shadow log retains
the exact ship/frame velocities, gravity, body radii, flight limits, sun and
boundary in `baseline.transfer_source`.

Instrument the staged transfer's rejected leg and blocking body for those
sources. Compare that reference with the ordinary controller's physical route
in a controlled replay before extending support for one bounded detour or frame
transition. Keep missing or unsupported transfers unknown; changing value
weights cannot supply their costs. The selected current trip and other neutral
alternative must also remain comparable, so resolving the enemy transfer alone
does not automatically produce a complete shortlist. These worlds remain
regressions; use predeclared fresh worlds for a later behavior comparison.
