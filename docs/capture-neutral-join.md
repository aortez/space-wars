# Source-bound neutral timing join

## Frozen plan

This slice joins the separate `neutral_capture_timing_v1` record into the
observational transfer comparison. It does not change a playing controller,
rank capture values, fit phase durations, populate an evaluator cache, or enable
deployment. The older covered-ground `local_reference` and `capture_costs`
remain separate and unchanged. Earlier transfer sources and hypothetical
destinations cannot receive evidence from a later actual arrival.

The new headless option is `--compare-neutral-timing true`, alongside the
existing fixed `--compare-transfer-sources` and shared allowance. Only the
actual controller's fresh native choice can produce a record. The final
existing charged ranking step emits a separate `neutral_timing_costs` row:
zero additional travel for an explicit current capture entry and the unchanged
conditional six-phase total at that source tick. Unknowns stay explicit;
publication delay is never subtracted from this total.

A numeric record adds bounded current-observation validation to Pending and
Ready. Existing actor, configuration, chronology, material, environment,
health, unassisted-flight/contact and 120-tick expiry guards still apply. The
new context pins the mission visit, attempt start, completed sorties, all eight
retry/invalidation counters, selected site/material and neutral claim rules.
Every observation must measure that same site with finite geometry and at
least one usable boarding hatch; Survey-to-Selected is normal, but deferred or
missing measurements cancel. Local site position allows 0.002 units of numeric
variation; world coordinates and ship gravity may change during normal flight.
The original solar plan and direction stay immutable. A separate validation
tick records a same-site solar assessment using the current circling/direct
phase. This is conditional route eligibility, not proof of trajectory safety.
Any newly exposed or nonfinite opponent observation cancels the contribution.
Cancellation cannot revive the same token, including when no graph work is
available. Nonnumeric/absent contributions add no new validity dependencies.

Before new replay outcomes, freeze code, runner, tests and this plan together.
`tools/join-neutral-capture-timing.py` consumes two existing archives:

- `neutral-timing-v1/summary.json`, SHA-256
  `805c2bc5d090a06b87b11e61cb8904cefadde1c9a101bddf08d614592e4a9333`:
  20 historical nominated continuations plus 16 ordinary fresh-world on-runs.
  Use each archived native first-choice tick, with no replacement or retry.
  This retains 35 choices, of which 32 had numeric timing and three were exposed
  unknowns. The one no-choice slot ends at its original 60-second window;
  its configured comparison tick is 3601, after termination, so no request or
  dispatch is invented. Actual queue admission may further withhold records.
- `local-composition-v1/summary.json`, SHA-256
  `ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`:
  all 13 old ordinary comparisons / 21 source groups / 62 candidates, replayed
  once with the join off and once on. Sources with an older existing site must
  not be backfilled. Compare old forecasts, local references, compositions,
  rankings, lifecycle and graph charges with the archived results.

All 62 runs retain their original seeds, controls, physical endpoints, flags,
capture windows, timing domains and uncertainties. Verify complete controller
trace bytes, sensor counters, upstream records, physical reports and playing
work against the archived executable outputs. Reconstruct each current neutral
validation from the raw source/observation rows, including solar geometry, and
retain all refusals and cancellations. A terminal observed row may cancel a
queue before the host's existing pre-physics break; it must not dispatch work
or manufacture an executed control tick.
Cancellation signs within the independent solar reconstruction's 0.01-unit
uncertainty stay explicitly unresolved. If a terminal physical-contact label
has no retained solver contact, record its subtype as unverified: the host also
sees remembered debris contacts, whose damage-observation field is not in these
traces. Neither case is permission to adjust runtime thresholds or fill missing
evidence.

Playing consumers retain their shared 4 graph / 384 query allowance. The
comparison receives the residual of one separate shared 64 graph allowance
after those consumers and makes zero physics queries. Construction, bounded
validation, serialization and diagnostic IO are outside the graph quota and
are not free CPU; this study makes no Pi performance or bot strength claim.
Combined nominated capture replays prohibit immediate and separately scheduled
transfer forecasts so neither can escape this diagnostic budget.

Regression coverage includes source-only arithmetic, old report/work identity,
unknown isolation, Pending/Ready cancellation at zero budget, selected-site
query cadence, one usable hatch, motion/gravity, retry and claim changes,
threat onset/nonfinite geometry, solar source/current clocks and current phase,
terminal-observation accounting, and the combined CLI exclusions. Independent
review precedes the replay; independently audit the resulting raw records and
document any limitation before pushing. Deployment remains paused.

## Results

Runtime, runner and plan froze at
`c1f1cad3623d25a493c6e19136a38d0cf2b6e514`. The profiled release binary is
`target/capture-flag-survey/surface-mission-soak-c1f1cad-profiled`, SHA-256
`9d5f33967941752b19489869d8948260399f184dbb6768d03c503f4048cfe34f`.
The complete raw summary is
`target/capture-flag-survey/neutral-join-v1-profiled/summary.json`, SHA-256
`4bd0e1232f81e720c537415aa51f005b8b2c207fe5a84bd7d8cb7ab5e2488285`.
The [tracked projection](data/neutral-timing-join-v1.json) retains commands,
source records, compositions, publication/termination states, budgets, full
parity digests and all raw file hashes; repeated candidates and motor trajectories
are omitted from later snapshots, rather than duplicated.

All **62 runs passed** with complete controller traces, sensor work, upstream
allocations and physical reports equal to the archived runs. The comparison
option made no change to playing behavior. There were 136,172 physical ticks
(37.825 simulated minutes), 272,416 controller rows and the same number of
sensor rows. Full trace comparisons covered 2,266,012,792 decompressed bytes.

| Group | Runs | Requests / published | New numeric records | Comparison graph work |
| --- | ---: | ---: | ---: | ---: |
| Historical first choices | 20 | 20 / 20 | 20 | 20 |
| Ordinary fresh first choices | 16 | 15 / 15 | 12 | 22,664 |
| Old source comparisons, off and on | 26 | 42 / 42 | 0 | 112,170 |

The 35 canonical requests retain all 32 numeric records and three exposed
unknowns. All 105 candidates remain in those comparisons; only the 32 actual
eligible sources receive a numeric neutral total. The no-choice slot submits
nothing and has no fabricated source, result or dispatch. Historical requests
finish in the source tick using one charged composition step; their alternatives
have no runnable motor forecast. Fresh requests publish at ages 0–65 ticks.
The three exposed records stay unknown throughout publication even when older
covered references are available.

All 13 older source comparisons match exactly in both arms after removing the
explicit new optional fields and wall times. This includes old forecasts,
local references, compositions, rankings, states and per-tick work. The 21
sources per arm retain 62 candidates and consume the same 56,085 graph steps,
publishing at ages 7–100 ticks. Older selected sites never acquire fresh timing
retroactively. Their option-on results have no neutral record and add no
neutral validation work.

The 32 numeric records receive **3,585 current validation observations** while
the queue remains live. The historical group expires all 20 at age 121.
Fresh cancellations are eight expirations, four material/ownership changes,
two ends of unassisted flight, and one newly unsafe conditional solar
assessment. The 26 compatibility runs retain their original 36 expirations,
four ends of unassisted flight and two material/ownership changes. These are
queue lifetimes, not failed physical captures; the ordinary capture observers
continue unchanged. The short queue lifetime remains at most two seconds,
well below a complete capture trip.

The solar case is `fresh-fresh0-asteroids3-s1-on`: choice/source tick 1932,
publication tick 1997, cancellation tick 2034. The source plan's predicted
departure clearance is 8.009628 units. The current same-direction circling
assessment is approximately 0.009949 at tick 2033 and -0.012985 at tick 2034.
The independent reconstruction retains tick 2033 as a sign-uncertain witness
within its 0.01-unit tolerance; the next negative value is outside that band.
The bot later departs normally at tick 3637, exactly as before. This demonstrates
that the diagnostic can withdraw an out-of-domain reference; it does not
establish an actual collision or justify altering the native safety controller.
There are no unverified terminal contact subtypes in this corpus.

Total diagnostic work is 134,854 graph operations and zero physics queries.
Playing/evaluation/survey/shadow work remains 0 / 283,831 / 10,211 / 1,002 graph
operations, with 514,779 live queries and 18,456 flag-survey queries. Every tick
stays within the original four graph / 384 query allowance; observed maxima
are four and 127. The comparison uses only the residual of shared 64 after
those consumers. None of these operation counts imply a Pi frame-rate gain.

One setup interruption is retained separately. The initial binary omitted
`sensor-profile`; its first physical run completed and matched controls and
reports, but the audit stopped when the mandatory sensor file was absent.
No completed audit was counted. The unchanged frozen sources were rebuilt
with that feature, and the whole fixed plan ran in a new output directory.
The original 7,002-tick output, binary hash, 14 file hashes and failure reason
remain in the projection's `interrupted_attempt`, outside the 62 profiled runs.
No source, threshold, constant or seed changed in response.

Local checks pass: 222 AI library tests, 18 mission-harness tests (including
the profiled build), 449 Python tests, formatting and strict all-target,
all-feature AI Clippy. Independent pre-run review led to the combined-host
forecast exclusion and stronger mutation audits for immutable publication,
raw source binding and terminal cancellation witnesses.

The independent post-run audit passes all 62 runs, 764 raw file hashes and
62 logs, plus the initial interruption. It imports only the preceding
independently written audits, not the production analysis helpers. It
reconstructs the fixed plan, complete parity, source bindings, immutable
publication records, queue lifetimes and current validation, shared budgets,
projection and written totals. Of the 134,854 diagnostic operations, 134,777
are motor steps and 77 are final composition steps. Maximum independent solar
reconstruction error is 0.000168 units; the one sign-uncertain observation stays
explicit. No unverified cancellation witnesses remain in this corpus.

The report is embedded in the tracked projection and retained as
`target/capture-flag-survey/neutral-join-post-audit.json`, SHA-256
`1163a7f4a93402180f836c60967e55e0d0fbb7fa16c32320d9cd121652c4b831`.
Its sibling script SHA-256 is
`70a492d41f63cde521593b6b7f305899294dd24cb763520d046b089f419899e4`.

## Reproduction and next boundary

With the two pinned reference archives available:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/join-neutral-capture-timing.py \
  --neutral target/capture-flag-survey/neutral-timing-v1 \
  --compatibility target/capture-flag-survey/local-composition-v1 \
  --out target/capture-flag-survey/neutral-join-repeat
```

Use a clean checkout and a new output directory; the runner rejects dirty
sources and existing output directories. Preserve unknowns and interrupted
runs. The new total is still a conditional reference from an actual chosen
site, not a continuously updated time-to-capture prediction. The next decision
gap is remote acquisition and threat uncertainty: price hypothetical arrivals
without borrowing a future actual site choice, before allowing these records
to influence live destination selection. Deployment remains paused.

The next [conditional remote arrival screen](capture-remote-arrival.md) now
projects source-measured sites into known handoffs while keeping acquisition
and threat unknown. It also identifies a raw-evidence retention gap for distant
alternatives.
