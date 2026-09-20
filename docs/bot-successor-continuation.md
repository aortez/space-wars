# Physical continuations from escape handoffs

This follows the [budgeted successor comparison](bot-successor-comparison.md)
at `b622c25`. Twenty desktop matches compare explicitly chosen short flight
interventions with the same source states and unchanged physical rules. The
new runner records actual motion, weapons, health and subsequent match results.
**Default bots and the Pi configuration are unchanged.** No forecast chooses an
action; the command line selects the intervention before the replay starts.

## What we learned

The pursuit hypothesis predicts the first return inside 300 units within four
physics ticks (0.067 seconds) in all ten interventions that actually re-enter
that range before the trial or model ends. Short-term closing time is useful
evidence in these cases. It does not predict damage or a successful sortie.

Further escape preserves separation in the quiet regression and changes that
eventual loss into a win. In the asteroid case the opponent still closes before
the escape deadline; that intervention takes no ship damage during the six-second
measurement window but eventually loses. Deliberate combat also preserves much
more health there than the ordinary handoff, yet still loses. Immediate health
and eventual victory must remain separate measurements.

None of the nine site approaches reaches its staging point within six seconds.
The physically closest seed-2 proposal goes from 229.5 to 295.1 units away while
routing around its planet. A nearby or covered endpoint is not yet an executable
capture trip. Two asteroid-case approaches stop when their destination's material
revision changes. The normal mission resumes from the actual state.

## Experiment contract

The existing `surface_mission_soak` runner replays the whole match to an explicit
successful escape handoff. This retains both controllers' memory, the live
planning queue, world physics, ammunition and asteroid sequence. It does not
reconstruct a world from a sparse observation or reset the opponent.

The handoff tick's already-emitted command is common to all branches. Alternative
commands start on the following tick. The trial lasts at most six seconds
including that common tick; continued escape also respects its original attempt
deadline. Thus a full six-second trial emits 359 alternative commands. At its
end the normal coordinator chooses again, retaining the motor state actually
reached and without retaining permission to land at the proposed site.

`SuccessorContinuation` uses the same site-approach, escape and combat controllers
as the diagnostic comparison. Its entry point retains ordinary observation
version/owner validation, repeated-tick caching, ship recovery and solar escape.
It stops on unavailable controls, surface contact or tasks, a changed/missing or
newly owned destination, arrival at the staging point, or a reset/skipped tick.
Escape/combat also stop when the opponent is no longer an armed ship. A stopped
trial cannot restart itself. Ending at a staging point would require ordinary
fresh surface planning; no experiment issues landing, exit or capture permission.

The approach uses the sampled point in the material frame and each tick's current
planet pose. The revision check preserves that flight proposal, not a guarantee
of current landing clearance or cover. Those measurements are still required
before a surface commitment. The physical combat trial uses actual visibility,
weapon readiness, ammunition, recoil and hits; the forecast's clear-sight and
no-firing assumptions are not copied into the world.

Boundary/obstacle forecast margins stop *model comparisons*, not the physical
world. Existing route guidance and the enabled boundary controller remain in
charge. All trials use the single ordinary physics/gravity step. Historical
comparison jobs continue using only graph allowance left after local planning
and cover. Explicit interventions neither wait for those jobs nor claim to be
decisions made within their budget. Trial construction and diagnostic JSON are
outside policy/planning timings; actual control generation is in policy time.

## Results

All interventions control **P2 v11 against P1 v10**, using the existing boundary,
destination-cover and successor probes. The main seed is
`7725194555774358125`; the additional asteroid seed is `2`. Asteroids arrive every
three seconds in the two pressure cases. These are three selected source states
on two seeds, not held-out or seat-swapped policy evaluation.

Ranges and health below describe the first six seconds after the source,
including ordinary controls after an earlier intervention stop. Health starts at
39.48, 57.76 and 100 respectively. Every ship and pilot survives that window and
ownership is unchanged. Match results include the entire subsequent ordinary
mission, bounded by the existing 600-second match timer. Site IDs are
`planet:bearing`. The 300-unit range threshold is a diagnostic convention, not a
weapon-hit or safety boundary.

| Source | Intervention | Minimum range | Ship health at 6s | P2 result / match end |
| --- | --- | ---: | ---: | --- |
| Quiet, tick 11153 | Ordinary handoff | 196.7 | 39.48 | Loss / 203.07s |
| Quiet | Remaining escape | 347.7 | 39.48 | Win / 259.63s |
| Quiet | Combat | 203.4 | 39.48 | Loss / 352.55s |
| Quiet | Site 0:34 | 200.9 | 39.48 | Loss / 600s |
| Quiet | Site 0:0 | 164.4 | 39.48 | Loss / 526.07s |
| Quiet | Site 1:45 | 192.2 | 39.48 | Win / 509.87s |
| Quiet | Site 1:11 | 195.0 | 39.48 | Loss / 600s |
| Asteroids, tick 6802 | Ordinary handoff | 132.4 | 17.69 | Loss / 128.45s |
| Asteroids | Remaining escape | 189.2 | 57.76 | Loss / 164.03s |
| Asteroids | Combat | 145.3 | 56.84 | Loss / 179.88s |
| Asteroids | Site 2:22 | 146.4 | 54.20 | Win / 139.13s |
| Asteroids | Site 2:56 | 128.1 | 9.89 | Loss / 127.52s |
| Asteroids | Site 1:48 | 123.4 | 9.91 | Loss / 151.32s |
| Seed 2, tick 4686 | Ordinary handoff | 314.6 | 100 | Win / 131.80s |
| Seed 2 | More escape | 433.8 | 100 | Win / 578.95s |
| Seed 2 | Combat | 322.8 | 100 | Loss / 175.18s |
| Seed 2 | Site 2:7 | 368.6 | 100 | Win / 117.13s |
| Seed 2 | Site 0:44 | 320.5 | 100 | Win / 222.17s |

Quiet continued escape ends after 286 ticks (4.767s), at the original deadline
11439. Its minimum range *while escaping* is 419.0; the six-second minimum above
includes resumed mission controls. Asteroid escape ends after 220 ticks (3.667s),
at deadline 7022: its active minimum is 264.5 and it re-enters 300 units at tick
183 of the trial. Seed-2 escape runs the six-second experimental horizon; its
original deadline is 27 ticks later. These clocks have not been renewed.

An asteroid edits planet 2 at world tick 7085. Both approaches to it observe a
changed material revision and stop at 7086, after 283 alternative commands. Site
2:22 is still 878.8 units from its staging point then, versus 918.6 at the source.
Its eventual win therefore does **not** demonstrate reaching cover or capturing
that site. Site 2:56 still has its initial 57.76 health when interrupted; most
of its six-second health loss happens after normal control resumes.

Two quiet approaches last until the ownership timeout; all other rounds end
through pilot death. Quiet extra escape later loses its ship and enters recovery
before winning. Seed-2 extra escape keeps its win but takes 447 seconds longer
than the ordinary handoff. Neither result supports a universal “always escape”
rule. Switching to combat changes the seed-2 win into a loss.

## Where the forecasts agree, and where they stop helping

For quiet site flights, the own-ship position error at six seconds is 5.7–24.3
units. Quiet extra escape is 2.7 units off at its deadline. The asteroid site
flights that are later invalidated are only 1.1–1.2 units off at the last compared
four-second sample. The third asteroid approach is 18.2 units off at six seconds,
when real hits and changing gravity are already outside the forecast model.
These measurements do not isolate which omitted effect causes each error.

The pursuit hypothesis is much closer to the actual opponent than coasting or
braking in the main-seed trials. Nevertheless, range agreement can hide errors
in absolute position: quiet combat's pursuit branch is 39.0 units off for the
own ship and 46.8 for the opponent at six seconds, despite only 5.3 units of
range error. It is not a collision/cover certificate.

For seed-2 site flights the pursuit model encounters a nominal obstacle margin
at 172–173 ticks, and coasting reaches the opponent's boundary margin at 260.
Only the braking hypothesis lasts six seconds. Its opponent position is then
201–232 units wrong. The missing pursuit tail cannot be replaced by a safe-looking
braking forecast or treated as evidence that the opponent will remain far away.
The physical trajectories continue; they simply exceed those model branches'
validated scope. Error summaries exclude samples at or beyond a model-margin
stop and stop when the physical intervention yields to ordinary control.

The follow-up [bounded capture-trip experiment](bot-successor-sortie.md) now
extends the nominated-site flights through fresh landing, capture and return.
Three of nine contested trips reach their staging points, and one lands and
neutralizes an enemy flag but does not finish raising its own before a ground
timeout. An isolated physical fixture completes the full trip. A rolling forecast
must still retain an unsupported tail when a plausible opponent response reaches
a model limit. Keep the original escape clock and compare whole opportunities
before making an automatic choice. Do not rank these alternatives by the eventual
winners in this small matrix.

## Verification and reproduction

- 108 AI unit tests pass, including six new continuation lifecycle tests:
  committed prefix, repeat calls, original deadline, invalid proposals, material/
  ownership/contact/arrival guards, recovery, solar escape, live combat inputs,
  reset and skipped observations. These guard tests use synthetic observations;
  the runner supplies the new physical intervention evidence.
- The existing 20 physical mission tests and one physical combat test pass.
- All 20 new complete matches have clean material/physics audits. Fifteen apply
  interventions; three observation-only replays and two disabled controls match
  the parent in complete recorded traces, round results, cover and local work.
- Every intervention matches the parent's recorded prefix through the source
  command, and the following observation matches the state after its common
  physical step. Asteroid/pilot-damage events also match through that prefix.
  Source forecast results are identical to the parent. Shared graph quotas remain
  intact. The analysis checks 6,498 per-tick physical snapshots and 5,019 emitted
  alternative commands, without comparing unmodelled tails as predictions.
- Formatting, client compilation and AI/core all-target clippy with `--no-deps`
  and denied warnings pass. This is not a dependency-wide lint or Pi timing claim.

Build with Rust 1.89.0 and `--locked`; tests use `RUST_MIN_STACK=16777216`.
One reproduction is:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --seed 7725194555774358125 --asteroid-interval 0 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --disengagement-boundary true \
  --probe-disengagement-handoff true --probe-destination-cover true \
  --probe-successors true --successor-step-cap 128 \
  --continue-successor escape --continuation-seat 1 --continuation-tick 11153 \
  --trace true --out /tmp/quiet-successor-escape
```

`--continue-successor` accepts `none` (default), `observe` (identical controls with
dense measurements), `escape`, `combat`, or a proposed `planet:bearing`. A missed
handoff, absent/unusable proposal or wrong experiment configuration fails
explicitly. `continuation.jsonl` samples the source and each following physical
tick through six seconds; `report.json` includes the actual stop and final match
state. `successors.jsonl` remains the separate historical, budgeted forecast.

Artifacts live in `target/successor-continuation/`: exact commands, parent bindings,
matrix/analysis scripts, reports, traces, cover samples, allocation logs, builds,
test logs and source hashes. The durable archive is
`/home/oldman/.codex/visualizations/2026/09/19/bot-successor-continuation/`.
Its manifest binds the parent runtime to `b622c25`, the final source patch and the
executed binary. All archived members are verified by size and SHA-256. Large
sensor timing logs stay in `target`; all observations needed for the comparisons
are archived. No deployment, push or default promotion is part of this checkpoint.
