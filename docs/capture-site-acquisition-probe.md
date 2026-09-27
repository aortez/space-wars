# Following a source reference through site acquisition

The [source-local composition](capture-local-composition.md) leaves an explicit
gap between the transfer handoff and the selected landing site. Existing local
phase references start at site choice. This slice measures that boundary without
adding a duration constant, ranking missions or changing the playing policy.

## Frozen plan

Select **all 20 alternative candidates with numeric local evidence** from the
previous 62-candidate corpus. Selection uses only evidence available at their
source ticks, not physical outcomes. Preserve the denominator: 21 current
candidates, 20 numeric alternatives and 21 alternatives lacking local evidence.
All 20 selected references are remote neutral/no-flag measurements. They include
16 departure-frame alternatives (four nominations immediately enter capture)
and four historical transfers. Conditions, seats, source ticks, destinations
and historical reference sites remain fixed. These correlated cases cannot
establish enemy-flag acquisition costs or population success rates.

Run each nomination twice: the frozen previous executable stops at transfer
handoff as before, and the new executable enables
`--probe-acquisition-seconds 30`. Allocate the full 60-second transfer window
plus 30-second acquisition window and a final observation. A failed transfer
never starts the acquisition clock. Preserve every refusal, interruption and
censored trial; never replace one with a convenient success.

The transfer intervention retains the existing `defer_new` pursuit policy.
Pursuit deferral ends at actual arrival. The continuation executes that neutral
handoff command once, then uses the normal controller entry point, evaluator,
sensors, budgets and physics. No historical site is forced. Native
`--bounded-acquisition-seats none` remains explicit: the observer's horizon
censors the trial and is not a controller deadline.

Start from the actual coordinator `arrived` event. Stop at first site choice,
actual mission interruption/replacement/failure, physical progress without a
witnessed choice, match/runner end or the observation horizon. A changed capture
replan counter or departure from the destination's observation frame ends this
attempt. Solar avoidance can continue while that attempt and frame remain. A site choice requires
the current capture's started tick and a matching, current native acquisition
snapshot. A retained site during a solar override is not a fresh choice. Loss,
recovery and attempt replacement precede same-tick choice; a valid choice at the
horizon is observed. The endpoint controller command is unexecuted. A match
ending during physics has no synthetic final controller row.

Use dense traces to audit the half-open waiting interval and native wait reasons.
Keep absent native updates separate from measured rejections. Compare the actual
chosen site with the historical reference and track source, arrival and endpoint
planet identities and evidence ages. Matching bearing alone does not renew old
material/owner/flag/radius/claim evidence. Changes during the transfer remain
visible even if endpoint identity later matches again.

## Verification and reproduction

The driver freezes its complete plan and hashes before starting physics:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/probe-site-acquisition.py \
  --reference target/capture-flag-survey/local-composition-v1 \
  --baseline-binary target/capture-flag-survey/surface-mission-soak-5ab1644 \
  --out target/capture-flag-survey/acquisition-probe-v1
```

Prior executable SHA-256:
`26e6cf18fa9c6c1d0c7d9475d6f38cbeeb5fada34999b7686d696c153fe025f5`.
Source composition summary SHA-256:
`ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`.

Require the original ordinary prefix and exact source observations, nominations
and candidate controls. Compare old/new transfer traces and all controller
observations byte for byte through the old terminal observation, including both
players. Verify ordinary evaluator, survey and upstream-work prefixes. This
includes reconstructing evaluator charges from the live planner's usage and the
flag allocator's post-evaluation residual; transfer probes do not emit the
separate evaluator-work file used by scheduled comparisons. Reconcile the
reconstructed total to the evaluator report. This
establishes parity **through arrival**, not a post-arrival counterfactual: the old
executable stops there. Later intents follow the ordinary call path by construction.
Independently reconcile the new endpoint to native telemetry and lifecycle events,
retain raw hashes and exercise censoring/precedence/staleness in focused tests.

No deployment, live cost admission or bot-strength claim is part of this slice.
Results will be recorded below after the frozen study completes.

The first pair was produced at runtime/runner freeze `334a498`, then its audit
stopped on the absent evaluator-work file. Its raw artifacts are retained in
`target/capture-flag-survey/acquisition-probe-v1`. The correction reconstructs
that work from existing logs; it does not change runtime, sources, horizon or
outcome rules. The full unchanged matrix will run in
`target/capture-flag-survey/acquisition-probe-v1-rerun`, including the same first
pair, whose completed trajectories must also match the retained initial files.
