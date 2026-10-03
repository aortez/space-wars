# Actual hatch local-failure recovery

## Frozen question and policy

The [actual landing investigation](actual-landing-route.md) found that a failed
local walk/powered attempt finishes within the live request's lifetime, while
its full fallback cannot. Capture then waits through replacement requests until
the ship is lost. Test an opt-in response to that native completion, without
weakening exit checks or increasing planner allowances.

`actual_local_failure_abort_v1` aborts the current capture on the first current
receipt for a completed unsuccessful actual-pose walk/powered corridor attempt.
It acts only while aboard, landed and transfer-ready, after the required-site
gate and only without a positive current actual route. Missing, unsupported,
unfinished, stale or wrong-actor evidence cannot trigger it. Walking-only
planning supplies no powered-attempt receipt. The receipt binds the original
objective, generation, request/measurement clock and planet-local hatch pose.

This is a bounded waiting policy, not proof that all routes are impossible.
The native full fallback continues unchanged until ordinary mission retirement.
The abort emits neutral controls. The next mission observation uses the existing
failure/reconsider path, including its 30-second destination deferral; ordinary
mission guidance must demonstrate any actual departure. One capture can abort
once. No forced transfer, teleport, route permission or new flight forecast is
introduced. Defaults remain disabled.

The headless option `--actual-route-recovery-seats none|0|1|both` enables both
native feedback and the controller response. Reset retains configuration and
discards progress. The native shared allowance remains 4 graph operations and
384 queries per dispatch; source expiry remains 120 ticks. Existing pose,
geometry, flight and return-route checks are unchanged.

## Comparison fixed before outcomes

Use `tools/validate-actual-recovery.py` against the 17 completed candidate matches
in `target/covered-handoff/v1/summary.json`, whose SHA-256 is
`eb56a4341f5b6819ec5a9015ecdfb4a154822237bdd9f9ae5e888afba2f112b7`.
First replay all 17 with the new option disabled and require exact seven-stream,
physical, sensor, non-timing planner, initial-cover, handoff and allocation parity.
Then rerun the same 17, enabling only the evaluated seat in the 15 existing
cover-enabled cases; the two cover-off cases remain exact disabled controls.
Keep all existing world seeds, equipment, policy settings, 180/600-second limits
and ordinary match termination. At most two simulations run concurrently.

Freeze source, tests, this plan and a separately copied profiled release binary
before inspecting candidate outcomes. Preserve failures without tuning the
policy to the matches. The three health-enabled cases remain separate regression
diagnostics, not extra independent strength samples.

Audit every receipt and abort against the original/current consumed observations,
and retain complete source/abort witnesses. Any first control difference must
follow a recorded abort. Cases without an abort must remain exact apart from
the new option's telemetry, including sensor work and allocation ledgers. Audit
all existing publication, physical transfer, crossing and continuation gates.

Primary development checks: the world 1 P1 powered planet-0 stall should now
reach a bounded decision after the source-7635 attempt completes; record actual
liftoff, damage, later capture/departure and final match outcome. The world 0 P2
powered positive actual route and exit should remain unchanged unless an earlier
legitimate failure causes divergence. Report all eight primary armed results,
not just this visit. These retained cases establish regression and causal
evidence, not independent playing strength or Raspberry Pi performance.

## Results

Pending frozen comparison. No default promotion or remote publication is part
of this experiment.
