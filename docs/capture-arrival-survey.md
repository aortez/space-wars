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
