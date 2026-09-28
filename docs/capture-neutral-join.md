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

Pending the frozen replay and independent audit.
