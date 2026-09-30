# Predicted-arrival terrain survey

## Frozen experiment plan

The four body-motion comparisons locate arrival position at material bearing 33,
which the later native landing choice also selects. Older retained surveys cover
bearings 63 and 31. The next question is whether a forecast available during real
play can obtain actual terrain evidence near that arrival position, with correct
measurement epochs and bounded work. This is a coverage experiment, not a new
capture-value policy or a forecast of capture success.

Add opt-in `--survey-predicted-arrival true` to the fixed comparison harness. A
request needs a completed comparison from an earlier tick, validated against the
current observation, actor, vehicle, episode, material and mission context. The
queue retains its existing 120-tick source lifetime. Completion at age 120 cannot
issue a later request. Prefer a numeric handoff to the current neutral destination;
otherwise choose the lowest-index neutral alternative with a numeric handoff.
Quantize the forecast endpoint's ship POSITION relative to the rotating planet
into the existing 64 material bearings. Heading does not choose the sample.

Request only one site per actor. Its generation starts at the first eligible
request and stays fixed through temporary deferral until the source is cancelled
or replaced. Local landing work and unavailable material queries defer requests.
The first harness requires synchronous local sensing (`--live-objective-seats
none`), so a temporary survey planner cannot overlook persistent local jobs.
An isolated, short-lived destination planner uses a cloned observation after all
existing live, evaluator, flag, shadow and comparison work. It consumes only the
remaining physical query quota, sequentially across seats, with at most 192
queries per atomic sample and no graph work. Skip actors already charged physical
work and throttle every charged attempt, including negatives, to once per 30
ticks per actor. Duplicate dispatch cannot spend twice. No new geometry enters
playing observations, the evaluator, its remote cache, or an older frozen source.

Keep comparison forecasts on their existing point model. Both motion models
already locate bearing 33 in the prior cases, so promoting the body-origin model
simultaneously would confound this experiment. Archive source, completion,
request, predicted-arrival and actual measurement ticks, the comparison token,
model name and every deferral/negative/incomplete result. A temporary planner's
own token is not a cross-attempt identity.

Freeze code, tests, this plan and expanded commands before replaying:

- The four original ordinary sources: seat 0, source ticks 3816, 3876, 3934 and
  3997, seed 3491156488288037499, no asteroids, comparison allowance 64, playing
  graph allowance 4 and physical query allowance 384. Each runs survey off/on.
  These sources are already doing local landing work; correct deferral is useful
  evidence, not a reason to relax local priority.
- The same four existing controlled nominations to destination 2, retaining
  the full acquisition and capture/departure continuation, also off/on. Attach
  the already-supported neutral comparison observer; no other control or model
  setting changes. The controlled transfer provides an eligible survey window.

All 16 runs must preserve historical and paired complete control/observation
traces, normalized sensor work, existing upstream budgets/evidence, native choices
and physical outcomes. Paired comparison reports and graph ledgers must match.
Audit new physical charges separately against the residual budget, including
busy actors and all clock/lifecycle boundaries. Independently reconstruct radial
bearings and compare measured geometry with the later native choice only when
material/actor identity and age remain valid. That retrospective comparison must
not alter either the original forecast or the measurement epoch. These are four
correlated sources from one quiet world; report that limitation explicitly.

Commit failures and unknowns as well as successes. Review code and raw evidence,
then preserve exact-head CI on PR #122. No device deployment is warranted for
this observational-only harness. A later source may consume genuinely earlier
measurements under its own admission rules; this slice does not do that join.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/survey-predicted-arrivals.py \
  --out target/capture-flag-survey/arrival-survey-repeat
```

Use a clean checkout and a new output directory. The earlier retained ordinary
archive is required and its summary hash is verified against the tracked record.

## Results

Runtime, fixed cases and the experiment plan froze at
`525af8076dfd8cac6b3967fe716324190c2b4c6b`. The profiled binary SHA-256 is
`7a70ca5d159a27311a25047b1835a6ab65ee0a30622481333db424b2535227b2`.
All **16 runs** passed their historical and paired physical/control/observation,
sensor and existing-work audits, covering **88,118 physical ticks**. The eight
controlled replays preserve the original native choices, captures and departures.
Their current-route point forecasts exactly match the earlier distant forecasts.
Paired comparison outputs and normalized graph ledgers are unchanged.

The four ordinary runs correctly defer throughout their available forecast
windows: **85, 88, 78 and 16 ticks**, all for local landing demand. They spend
**zero new queries**. The controlled transfers obtain three measurements each:

| Source tick | Comparison complete | Request generation | Actual measurement ticks | Native choice tick |
| --- | --- | --- | --- | --- |
| 3816 | 3852 | 3853 | 3853, 3883, 3913 | 5509 |
| 3876 | 3909 | 3910 | 3913, 3943, 3973 | 5493 |
| 3934 | 3966 | 3967 | 3967, 3997, 4027 | 5509 |
| 3997 | 4027 | 4028 | 4036, 4066, 4096 | 5535 |

All **12 measurements** cover planet 2, bearing 33, the later native selected
site. All have measured geometry and clear sampled climb positions. Each costs
**60 physical queries**, for **720 total** and zero new graph work. Earlier
physical work defers the controlled observer on 19, 22, 23 and 24 ticks;
the remaining deferrals enforce the 30-tick sampling interval. The full existing
plus new physical-query ledger never exceeds **126 queries in one tick** in
these runs, within the configured 384-query allowance. These are query counts,
not device timing or frame-rate measurements.

At native choice the samples are **1439–1656 ticks old**, within the existing
1800-tick historical-evidence limit, with unchanged actor/material identity.
Rigid transport of the recorded geometry into the actual choice frame agrees
with fresh native geometry within **0.000387 world units**, below the preexisting
0.002 reconstruction tolerance. This comparison is retrospective: it neither
backdates new terrain evidence to the old forecast source nor admits a complete
remote capture cost. The sample has not established a route to a flag and back,
future threat exposure, or capture success. These twelve samples are repeated
measurements across four correlated cases, not twelve independent trials.

The [tracked projection](data/capture-arrival-survey-v1.json) preserves expanded
commands, hashes, historical checks, all deferrals and each actual sample. Raw
summary: `target/capture-flag-survey/arrival-survey-v1/summary.json`, SHA-256
`513abcf46b1690695c3ef175ac65e8717362ddd0c6fb44c3e0ca46842d065b5a`.
There were no interrupted runs, failed runtime audits or dropped cases.

Review identified a gap in the analysis checker after runtime freeze: it checked
that request generation followed completion and stayed stable, but did not require
the first request's generation to equal its first eligible tick after initial
deferrals. Audit-only commit `d382ab3` adds that requirement and a mutation test.
All eight enabled logs pass the strengthened checker with exactly the same
measurement/budget/geometry results. The original raw summary remains untouched;
the [separate re-audit](data/capture-arrival-survey-generation-audit-v1.json)
records the checker hash and provenance. No runtime, case, threshold or result was
changed after seeing outcomes.

The [independent audit](data/capture-arrival-survey-audit-v1.json) checks the raw
evidence without importing the new runner. It verifies all 208 raw files and 16
logs, 176,252 controller rows and the same number of sensor rows, 1,562,311,426
decompressed trace bytes, and all 616 survey-ledger rows. Its checks include
historical parity, forecast provenance, first request generations, real material,
ship and opponent epochs, query accounting and independent geometry reconstruction.
The audit script is retained at
`target/capture-flag-survey/arrival-survey-post-audit.py`; the report pins that
script and its older helpers. No review blockers remain.

Local validation passes **987 Rust tests** across engine-rapier,
scenario-spacewars and spacewars-ai with all targets and sensor profiling,
**478 Python analysis tests**, and formatting. Clippy for spacewars-ai and its
targets completes with eight existing dependency warnings; none concern the new
AI or harness code. Focused tests cover request lifecycle, rotating-frame
bearings, neutral candidate selection, local deferral/resumption, expiry,
two-actor residual budgets, repeated dispatch and refresh limits.

The next slice can freeze a new comparison source after these real measurements,
then test whether the better-covered historical geometry supports the existing
remote arrival screen. It must preserve the samples' actual ages and identities,
keep unavailable costs unknown, and stay observational until admission is tested.
This slice changes neither default bot behavior nor the device build.
