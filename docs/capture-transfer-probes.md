# Transfer rejection probes

The flag-value shadow admitted two distinct historical flag surveys in sixteen
comparisons, but no alternative obtained a supported whole-trip cost. This
experiment investigates the rejected transfers before extending the model.
It does not change a selectable bot, the ordinary evaluator, or live budgets.

## Fixed experiment

The input is the complete final flag-shadow record at `194270e`, retained under
`target/capture-flag-survey/value-shadow-final`. For every report that used flag
evidence, nominate every non-current shortlist destination with known local
costs. This yields **31 probes at 16 sources**:

- World 0, three-second asteroids: source 5952, enemy planet 0.
- World 1, quiet: sources 3816, 3876, 3934, 3997, 4057, 4129, 4199, 4261, 4321,
  4377, 4440, 4503, 4559, 4622, 4685; enemy planet 1 and neutral planet 2 each.

These sources are correlated observations of two worlds, not independent
strength samples. Later sources may already be committed to landing. Preserve
all refusals rather than searching for a more convenient intervention tick.

Each run replays the original physics, opponent, asteroids, sensors, evaluator
and surveys. Before the source tick's intent, issue one external destination
nomination through the normal coordinator. This intentionally bypasses evidence
and value-margin admission; it retains commitment, altitude, deferred-target,
once-per-trip, recovery, solar and combat priorities. Rejected nominations fall
back to the normal controller. Accepted nominations use normal transfer guidance
on subsequent ticks; they are never forced again. No supported costs or evaluated
switch telemetry are fabricated.

Require exact original trace bytes strictly before the source, exact observations
for both players at the source, the exact pinned `TransferSource` (bit identity for its f32 fields), and unchanged
ordinary evaluator reports before the source. The opponent's source controls
must also match; for a refusal both source control records must match. The bot's
same-tick command cache makes this pre-intent intervention boundary essential.

The cap is sixty simulated seconds. Stop at the actual `arrived` event (including
`queries_ready`), ship/pilot loss, recovery, retarget, match end, or timeout.
Conservatively stop on any solver surface contact or new debris contact as well.
Solver contacts include speculative positive separations and do **not** establish
impact. Loss/contact takes precedence over arrival on the same observation.
A match ending after a physics step is recorded before another control tick.
Terminal controls in the trace are not executed. Timeouts/interruption/refusals
are censored outcomes, never numeric costs.

Arrival means the coordinator hands off to its landing task: the destination is
the current frame, range is below radius+105, and relative speed below 18. It does
not mean landed/captured. The staged reference instead aims at rest-to-rest near
radius+85. Any timing comparison is descriptive, not error against an identical
endpoint.

## Diagnostic scope

One opt-in analytic diagnostic runs at the nominated source. It uses the exact
ordinary transfer calculation, recording the first rejected leg/body and its
geometry. The existing bound is eight bodies per pass across up to three passes.
No physical query or rollout is added to ordinary planning. Probe diagnostics,
contact inspection and trace IO are explicitly outside live planner fuel.

The moving check measures geometric separation between the entire reference
corridor and a body's linear sweep relative to the target's velocity. It is not
a synchronized closest approach. Radius+65 is a planning margin, not hull
clearance. Initial settle/climb arithmetic and completed stage arithmetic are
separate; a rejected sum never becomes an accepted cost. Neither successful
controller detours nor these few probes alone justify a new cost calibration.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/probe-transfer-references.py \
  --reference target/capture-flag-survey/value-shadow-final \
  --out target/capture-flag-survey/transfer-probes-v1
```

The runner refuses an existing output directory and writes all 31 cases before
execution. Reports retain source/binary hashes, commands, per-tick trajectories,
first rejection geometry, prefix/source audit results and every outcome.

## Results at `18fcddc`

[The complete record](data/capture-transfer-probes-v1.json) retains all 31 probes,
commands, pinned sources, diagnostic geometry, raw-log hashes and audit results.
All **267,008 pre-intervention seat-control records** match the frozen traces.
Both source observations, f32 transfer inputs and ordinary evaluator prefixes
match for every case. These counts include deliberately repeated prefixes.

| Alternative | Nominated | Allowed to switch | Arrived | Interrupted by pursuit | Refused by controller |
| --- | ---: | ---: | ---: | ---: | ---: |
| World 0, enemy planet 0 | 1 | 1 | 1 | 0 | 0 |
| World 1, enemy planet 1 | 15 | 4 | 0 | 4 | 11 |
| World 1, neutral planet 2 | 15 | 4 | 0 | 4 | 11 |

Sixteen refusals came from landing/support gates, six from switching altitude.
All nine accepted nominations stop before the cap: there are no timeout,
contact, recovery or match-ending outcomes in this set.

### A conservative static rejection

World 0 reaches the ordinary landing handoff at tick 6343, **391 ticks / 6.52
seconds** after nomination. Its source reference rejects the *departure* planet
1 on the transfer leg: segment separation 207.31 versus a radius-plus-margin of
211.00. The source's complete stage arithmetic sums to 8.93 seconds, but remains
rejected and is not a usable cost or an equal-endpoint timing prediction.

The physical branch issues no explicit avoidance waypoint, changes approach
frame once, preserves ship health (36.29), and records no solver/debris contact.
Its closest sampled ship-center distance above the departure planet's nominal
radius is 66.96, above the reference's 65-unit margin. This establishes one
conservative rejection relative to the observed controller path. It does not
prove that reducing the margin or accepting other straight references is safe.

### Moving-body timing remains censored

Every World 1 rejection involves departure planet 0. Enemy-planet trips reject
its moving sweep; neutral-planet trips reject its static overlap with the direct
corridor. All eight eligible branches switch to hunting a nearby vulnerable
opponent at tick 4189, after 3.20–6.22 seconds. The ordinary coordinator's events
explicitly record `pausing travel for nearby opponent` and `pursuit_started`.
These are priority interruptions, not failed transfers or arrival-time samples.
No branch has changed its approach frame by the interruption.

The four neutral branches record avoidance of planet 0 for 133, 101, 63 and 29
observations respectively. Their controls and trajectories differ from the
paired enemy branches; they are not duplicate trajectories despite sharing the
interruption tick. Launching still occupies most observations in both groups.
Three of those four neutral references already have stage arithmetic above the
30-second model horizon, although the earlier static rejection is the first
reported failure. Fixing one rejection therefore need not make a cost numeric.

### Interpretation and next experiment

Keep current acceptance and rankings unchanged. The useful next calibration is
an explicitly controlled transfer trial that distinguishes climb, detour and
frame handoff, with combat interruption recorded separately. Use the frozen
cases as regressions, add switch-eligible sources outside these two worlds, and
compare a bounded candidate model against ordinary physical guidance before it
can influence v13. If a later experiment suppresses pursuit to measure transfer
in isolation, label that as a second intervention; it cannot replace these live
priority outcomes or establish match strength.

This slice supplies exact failure geometry and a reusable physical probe. It
adds no live planning steps, no new finite mission costs and no policy version.

## Validation and audit corrections

168 AI tests, 352 Python tests, formatting and strict AI Clippy pass. The native
three-minute flag/shadow observer test passes with exact ordinary controls,
evaluation and physics and an actual shadow admission.

Independent review identified speculative contact labeling, incomplete terminal
precedence checks, and insufficient reconciliation of blocker geometry. The
final audit checks all nonterminal rows, source-body identity, radius margin,
launch/entry endpoints, target-relative sweep and independently recomputed
segment separation. Geometry comparison allows 0.002 units for double versus
f32 arithmetic; source identity uses exact f32 bits, without a tolerance.

The first execution (`transfer-probes-v1`, runtime `9130447`) stopped after its
first physical branch when the source audit compared short typed-f32 JSON
numbers with promoted-f64 JSON numbers. Controls and observations matched; all
24 differing decimal fields represented identical f32 bits. The incomplete
record remains available. The full unchanged 31-case plan was rerun at
`18fcddc` into `transfer-probes-v2` after correcting serialization identity and
adding the terminal audit fields; the committed record is that complete rerun.

Independent final review rechecked all 31 report and raw trace hashes, all frozen
prefixes, and all 2,688 probe observations against the ordinary controller trace.
The arrival endpoint is at range 229.89 versus the 235.76 handoff limit, with
relative speed 16.16 versus the limit of 18. Outcome counts, refusal reasons,
pursuit events and detour counts match the raw records.

Runtime `18fcddc` was built and deployed to **sw-picade.local** with the fast
application updater. Installed client SHA-256 is
`8d910131b31483bb74e990352b3bc284c1326c64b25168c63df77594c0716548`;
the unchanged CLI hash is
`0d33f4d82a80df22cec0a56d74a3903d4fe05fe389100c2b174f752f4a4ca1f3`.
The kiosk is active (PID 587, zero service restarts after the update). Status
confirms P1 v10 / P2 v13, an automatic running Spacewars session and the existing
flag/value-shadow diagnostics. The new nomination and geometry probes are
headless options, so there is no new live behavior or device setting to enable.
