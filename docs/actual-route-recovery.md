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

The bounded decision works, but does not yet secure a safe escape. Both copies
of the retained stalled visit receive the native `arrival_window` receipt at
**7656**, abort capture once, and lift off at **7662**. The mission immediately
chooses its incoming-fire combat response and loses the ship to laser fire at
**8071**, 46 ticks earlier than the waiting baseline. Later recovery produces
one additional claim and departure in each copy. Neither match becomes a win.
Keep this policy opt-in; the evidence supports a waiting fix, not a general
strength improvement.

All **17 disabled replays** are exact. In the **17 comparisons**, 15 matches
have no receipt or abort and remain exact apart from the added option telemetry:
seven streams, physical outcomes, ordinary sensor work, non-timing native
planner telemetry and allocation ledgers. The remaining two are world 1 P1
powered and its separate health-enabled duplicate. Each has exactly one receipt
and one abort, with first control difference at **7658**. These are correlated
observations of the same landing, not two independent successes.

### The stalled visit reaches a decision and physically lifts off

| Tick | Native/physical observation |
| ---: | --- |
| 7635 | Actual landing; source generation 29 begins at 31.862 hull. |
| 7656 | Completed local walk/powered attempt reports `arrival_window`; capture aborts with neutral controls. |
| 7657 | Mission records capture failure and invokes its existing reconsider/deferral path. |
| 7658 | First changed controls; mission starts a pursuit in response to incoming fire. |
| 7662 | Ship is flying with no supported feet, still at 31.862 hull. |
| 7770 | Observed landing altitude exceeds 25, still at 31.862 hull. |
| 8071 | Ship is lost; current native combat evidence records laser damage. |

There is no hatch exit, claim or boarding on this abandoned visit. Before losing
the ship, the mission never selects a different destination or leaves planet 0's
nearest frame. The health option does not prevent this incoming-fire response;
its discretionary-pursuit rule intentionally leaves that response eligible.
The local failure remains a historical result from source tick 7635, with no
refreshed clock and no negative full-graph certificate.

In the primary match, recovery finishes at **11794**. The bot later claims and
departs planet 1 at **17904**, then planet 2 at **20431**, for three completed
departures including the original early planet-1 capture. It loses at **22016**
to a missile while recovering in a pod. The prior match had two completed
departures and ended at 20910. The improvement in later completions does not
change the immediate failed escape or the final loss.

### Complete retained outcomes

The eight primary armed cases have **2 wins / 28 claims / 27 completed
departures**, compared with **2 / 27 / 26** before this change. The other seven
primary armed control sequences are unchanged.

| Primary case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 4 → 4 | 4 → 4 | 21907 → 21907 |
| World 0 P1 powered | win → win | 5 → 5 | 5 → 5 | 36000 → 36000 |
| World 0 P2 walking | loss → loss | 4 → 4 | 4 → 4 | 26591 → 26591 |
| World 0 P2 powered | loss → loss | 4 → 4 | 3 → 3 | 30536 → 30536 |
| World 1 P1 walking | loss → loss | 1 → 1 | 1 → 1 | 36000 → 36000 |
| World 1 P1 powered | loss → loss | 2 → 3 | 2 → 3 | 20910 → 22016 |
| World 1 P2 walking | win → win | 4 → 4 | 4 → 4 | 36000 → 36000 |
| World 1 P2 powered | loss → loss | 3 → 3 | 3 → 3 | 36000 → 36000 |

The successful-exit comparison, world 0 P2 powered, remains exact for all
61,072 pilot observations. It still publishes the positive actual route at
18288 and exits at 18289 despite five preceding hatch resets. Its later failure
to return, board and depart remains unresolved.

The health regressions stay separate: both world-0 cases are unchanged, losing
at 16190 and 15014 with three departures each. World 1 P1 powered repeats the
same abort/liftoff/first-loss sequence, finishes with three departures instead
of two, and loses at **21354** instead of 20910. Its later trajectory differs
from the primary match. No health default is changed.

### Verification and evidence

Source, tests and the plan were frozen in **`8f72401`**. All **1,047 Rust tests**
and **702 Python tests** pass. Tests cover native completion versus unknown
work, unchanged queue work/physics, source pose/age/actor/objective gates,
positive-route priority, neutral one-time abort and configuration propagation
through capture creation, clone and reset. Formatting, strict AI Clippy with
`--no-deps`, and profiled/ordinary release builds pass. Scenario Clippy retains
the same seven pre-existing findings. All 34 complete replay/comparison audits
pass, including the unchanged maximum **4 graph operations / 384 queries** and
maximum publication age **120 ticks**.

The frozen profiled binary is
`target/actual-recovery/surface_mission_soak-8f72401`, SHA-256
`c0b838d8cdaed842dfdf5fa4833643721d53565ba732ef11bfea57279be90933`.
The complete summary at `target/actual-recovery/v1/summary.json` has SHA-256
`e1810fbe8fbd22d9d488381bee62ed8f5e62ba38dc93988d5fa78065b378e186`.

[The manifest](data/actual-route-recovery-v1.json) records all outcomes and checks.
[The archive](data/actual-route-recovery-v1.json.gz) retains the frozen plan,
complete summaries, original receipt/abort observations, physical/route/flight
witnesses, first changed controls, post-abort physical transitions and native
loss evidence, validation logs, reproduction scripts and raw-file hashes.
All 125 embedded documents, 585 raw files, the retained source inputs and both
frozen binaries passed hash verification.
Defaults, live budgets and transfer permissions remain unchanged. No remote
publication, deployment or Raspberry Pi performance claim is made.

### Next investigation

The immediate gap is the mission transition after abort: a damaged ship becomes
eligible for a new incoming-fire pursuit two ticks later. Investigate a bounded
departure or disengagement response that preserves defensive weapons and
physical safety priorities while measuring whether it actually gains separation.
Do not treat suppressing pursuit as proof of escape, or tune that next policy
against only this already-known match.
