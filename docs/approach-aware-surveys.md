# Approach-aware neutral surveys

This follows [v15's landing handoff study](costed-landing-handoff.md). V15
connected a forecast bearing to native arrival, but all four completed neutral
references departed later than v14. Two enemy references lost cover during
approach and fell back; one fallback added 9.48 seconds to departure.

## Investigation

The remote neutral survey chooses two material bearings once per journey.
Subsequent measurements refresh those same bearings, even as the ship moves.
Every valid neutral site receives the same phase constants, so recency (then
bearing number) chooses the reference. It does not compare approach geometry.

In the P1 bearing-0.8 neutral fixture, the original survey offered bearings 1
and 48. The accepted source at tick 445 selected 48. By native arrival at 497,
v14 chose 40; v15 held 48. Both entered SeekCover at 498, but v14 started
Approach at 501 and v15 at 858. Departure was 1,998 versus 2,211. The extra
circling explains most of this slowdown. Other neutral cases show the same
phase pattern. These are observations from the archived corpus, not a fitted
landing-time model.

The enemy cases are a different problem. At initial acquisition the opponent
was behind ground, and the chosen sites already lacked complete approach and
departure cover. Later exposure invalidated both references. In the 1.2 case,
v15's fallback actually landed 30 ticks earlier than v14, then took much longer
to claim and board: departure moved from 5,091 to 5,660. Shorter angular flight
alone cannot account for enemy exposure, flag walking or boarding.

Source: `target/costed-landing-handoff/frozen-v1`, bound by the tracked summary
SHA-256 `03e2ca83139228fcb78fe998df4afa2f3c4fcc4e671a5c08d8080800e8813f3c`.
Use native acquisition/milestone ticks for joins; event snapshots can be logged
on the preceding world-step tick. Relevant records are `report.json`,
`destination-cover.jsonl`, `landing-handoffs.jsonl` and the complete compressed
`destination-behavior.jsonl.gz`. `sensors.jsonl` contains profiles, not full
observations.

## Candidate contract

V16 (`material_mission_v16`, **Approach survey bot v16**) changes only the
neutral survey's requested bearings and ranking. The one-planet shortlist and
two-site bound remain. When the bearings change, requests may refresh at most
once per 30 ticks within the same journey/material identity. Local landing
sensing still takes priority. Unchanged bearings keep the existing generation.

Rank valid historical measurements by absolute angular separation from the
currently observed ship position, then measurement recency and bearing number.
Reproject each measured vehicle point from its original planet pose into the
current observed material frame. Reject degenerate/nonfinite/overflowing
geometry and preserve the original measurement tick. This is an approach
proxy in radians, not predicted arrival time, route feasibility or future cover.
Existing valid evidence may survive while a replacement request is pending;
its age is never renewed by retargeting.

Keep v15's landing handoff, source expiry, native reacquisition/refusal,
flag-evidence admission, phase constants, ownership weights and switch margins.
Ranking adds no world queries; all remote work uses the existing shared
4-graph-operation / 384-query dispatcher. Refreshes may increase request churn
and total queries, so an unchanged per-tick cap is not an equal-CPU-work claim.
Synchronous native sensors, snapshot construction and trace I/O remain outside
that cap. V16 is headless; UI and device defaults are unchanged.

## Development observations

Six exploratory replays are retained under
`target/approach-aware-surveys/exploration-v1`, including source/diff/binary
hashes and complete commands. No constants were tuned. Three neutral departures
improved versus v15 by 52, 132 and 138 ticks; P1 1.2 slowed by 53 ticks.
All four neutral cases reached all-planets-owned earlier. Both enemy cases
retained their v15 visits and refusals. These known fixtures are development
evidence, not independent strength samples or sufficient grounds for promotion.

Review found a finite-vector squared-length overflow and a publication-order
test that observed the same tick twice. The candidate rejects nonfinite norms;
the test now advances the tick. Flag consumption and pending/ready handoff
invalidation tests exercise v16 alongside v15.

## Frozen plan

Freeze code, tests, this plan and `tools/compare-approach-surveys.py` before
execution. Keep every failure, refusal, incomplete trip and regression:

- 32 directed runs: both original-destination and owned-base fixtures, both
  seats, bearings 0.0/0.4/0.8/1.2, v15 and v16, 180 seconds per run.
- The known seed 186767996776005237 regression, v15 and v16 in seat two versus
  v10, combat enabled, no asteroids, normal ten-minute match deadline.
- Twelve finished-match smoke runs over two fresh SHA-256-derived worlds from
  `approach-survey-behavior-v1:{0..1}`: no asteroids / three-second asteroids,
  v15/v15 controls and v16 in each seat against v15, rotating execution order.
  These are two independent worlds with correlated variants, not twelve
  independent strength trials.

All 17 directed/regression v15 controls must reproduce archived exact evaluator
bytes, mission telemetry and recorded physical outcomes. Paired complete action,
range and ownership traces must match before the first destination switch;
pairs with no switches must retain their physical outcomes. Audit shared work,
published flag references, accepted switch sources, native handoffs and fallback
cost invalidation for both policies. Report phase milestones, wins/losses,
captures/departures, transfer progress, survey queries/churn and unknowns.

`--trace-neutral-approaches true` records the v16 evaluator's immutable input
before shared dispatch, including both measurements, the current journey and
all observed planet/ship poses and identities. The optional headless trace adds
no queries. Check the full report-derived actor/tick sequence, both requested
slots and the refresh bound for uninterrupted demand. Independently join every
v16 evaluation to that input, verify original measurement ages and neutral phase
constants, and check angular ranking where current measurements exist. Separately
count retained references while replacement measurements are absent. Track
positive admission separately from the raw measurement registry; a negative
replacement, missing query readiness, expiry or changed material/ownership
cannot authorize a later retained numeric reference. A
post-dispatch `destination-cover.jsonl` publication is not evidence that the
same-tick decision had that sample available.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-approach-surveys.py \
  --predecessor target/costed-landing-handoff/frozen-v1 \
  --out target/approach-aware-surveys/frozen-v1
```

Use a clean checkout and a new output directory. Keep commands and all
source/binary/artifact hashes. Review the complete results before deciding
whether to retain, reject or promote; leave defaults unchanged meanwhile.
