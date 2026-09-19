# Budgeted destination-cover evidence

This implements the diagnostic slice proposed after the
[boundary-escape trials](bot-boundary-escape.md), against `5de84d3`.
The optional escape bot can now request actual remote landing and material-cover
measurements under the existing shared planning quota. **It does not use them to
choose a mission yet.** Fourteen complete replays preserve the prior observations,
controls, outcomes, and local planning work after excluding added diagnostics.

The probe fills an important gap: the losing boundary-aware quiet P2 escape has
58 successful landing measurements, all exposed to the opponent. Other cases
have covered and exposed candidate sites on the same destination. These are
small, fixed shortlists, not proof that an entire planet lacks cover or that a
covered site has a feasible capture-and-return trip.

## Request and measurement contract

`MissionSensorRequest.destination_cover` is separate from the current approach
planet's site request. It is absent unless `--probe-destination-cover true` is
enabled for a planner seat with pursuit disengagement and live objective planning.
Ordinary interactive policies and existing headless options remain unchanged.

At escape start, the bot shortlists the two nearest eligible, unowned,
non-deferred destinations. Each gets a bearing opposite the observed opponent
and one toward the arriving ship, expressed in the planet's material frame. If
those bearings coincide, the second is offset a quarter turn. The shortlist is
held during that escape; nominal geometry only proposes candidates. It has at
most four entries and requests no ground graph.

The scenario reuses the real `vehicle_landing_site_with_queries` checks for
material feet, slope, complete hull, belly, hatch and boarding entrances. A
successful site receives the same three first-solid cover rays as local landing
observations, at heights 7, 30 and 60. Only a first hit on retained material of
that destination counts as cover. A fragment, ship, or arena wall cannot stand
in for that planet. With no armed ship opponent, the sample's cover is absent;
that situation does not fabricate a material shelter.

Samples contain their tick, terrain revision, planet pose/velocity, own ship
form, opponent pose/armed state, query count, site and cover result. A successful
site still has **unmeasured capture and return-route feasibility**. The result
states distinguish pending, deferred, incomplete, no landing, measured and
stale. Absence of a request means not requested. Denying any query discards all
partial site/cover evidence and reports incomplete, even if the interrupted
site helper returned `None`.

A candidate is measured atomically against one live physics state. The oldest
eligible candidate is checked first, with shortlist order breaking ties; a
sample can refresh after 30 ticks. Different candidates keep separate ticks.
Historical measurements remain visible, but are marked stale after the physics
tick advances. Dirty material queries also revoke an old result when observed
again, including edits at the same tick.

This conservative freshness rule is deliberate. The runner dispatches planning
after both bots have emitted their current controls and before the physics step.
`destination-cover.jsonl` records the newly measured evidence at that point;
the next controller observation contains historical, stale samples. The
read-only handoff report includes them with their actual ages. This slice does
not silently move measurement ahead of action selection or label an old ray
current. A later decision must request current validation before commitment.

Local surface/recovery demand cancels the remote request. Request replacement,
missing actors and reset retire retained work. Clone/repeated observations do
not repeat queries. One request per actor holds at most four latest samples;
there is no accumulated in-memory sample history.

## One shared budget, with local work first

The early-landing adapter's query fuel/accounting is now shared with remote
checks. Each query asks for fuel before executing. The atomic check cap remains
**192 queries**, with at most one such check per actor per tick. Existing
per-capacity reservations still apply: a two-actor adapter with fewer than 384
global query units defers these atomic checks. A low-budget resumable site job
is a future extension; reducing the configured quota does not silently grant
unmetered work.

Local early checks and incremental objective jobs spend first. Remote checks
use only the remaining physics-query allowance, in stable actor order, and
cannot consume a local actor's atomic-check slot. Unused fuel does not carry to
a later tick. Under sustained local load, remote work may remain explicitly
deferred. It receives no minimum progress guarantee at the expense of landing.

Atomic requests reserve unique tokens from the existing queue's identity
namespace without inserting a graph job, taking a queue slot, or changing its
round-robin cursor. This avoids ambiguous identities in combined allocation
reports. `live-planning.csv` now names `landing_objective` versus
`destination_cover` work. Per-job charges sum to the same shared totals; the
live planner's aggregate query count includes both. Cancellation cannot erase
charges already performed.

The budget meters operations, not elapsed time. Snapshot construction and other
existing synchronous sensors keep their documented scope. Remote site/cover
execution is included in dispatch timing; diagnostic trace writing is outside
that timing. These desktop runs do not establish a Pi frame-time bound.

## What the saved matches show

The existing six activated cases were replayed twice: the previous escape and
the optional boundary-aware escape, each with cover probing. Two disabled
controls cover ordinary v11 and the boundary experiment. All fourteen matches
finish under the existing 600-second rules. The main seed is
`7725194555774358125`, with quiet and three-second asteroid variants; seeds 2
and 7 retain the additional P2 asteroid cases.

Across these runs:

- **45,340 recorded rows** match the baseline after removing only the added
  cover request, observation and handoff fields. Actions, existing telemetry and
  complete match outcomes are unchanged.
- **27,329 local allocation rows** retain their tick, actor, age, phase and work.
  Probe-enabled comparisons exclude token generation numbers because atomic
  requests now reserve their own identities. Global query totals legitimately
  increase by the separately audited probe charges.
- **575 checks:** 341 valid landing measurements and 234 no-landing results.
  None exhausts fuel in this sample; a targeted test covers that failure mode.
- **21,196 additional queries**, averaging 36.86 per check, maximum 60, under
  the 192 cap. No remote work is deferred in these particular replays. Small
  budgets and local-job contention are covered separately by tests.
- Eleven successful-escape handoff reports contain sampled evidence aged
  1–30 ticks, explicitly stale. Boundary seed 7 enters recovery before a
  successful handoff; its probe stops with the escape.

Representative evidence at handoff:

| Escape case | Measured shortlist evidence | Implication |
| --- | --- | --- |
| Main quiet P2, boundary-aware | All four latest sites are exposed; all 58 successful samples during the escape are exposed | Separation alone does not establish a sheltered next landing |
| Main quiet P2, original | Latest planet-1 bearing 45 has cover at all three heights; the other three latest sites are exposed | The two trajectories change the opponent-to-site geometry; this is evidence to compare, not a causal explanation of the win |
| Main asteroid P1, either | Planet-2 bearing 41 is covered; another site on that planet and both measured planet-0 candidates are exposed | Score specific sites, rather than assigning one cover value to a whole planet |
| Main asteroid P2, either | Planet-2 bearing 22 is covered; bearing 56 is exposed; one planet-1 candidate has no landing | A landing opportunity and a covered landing opportunity are different facts |
| Seed 2 P2, either | One measured candidate on each shortlisted planet has cover; the other bearings lack a landing | A successful escape still needs compatible physical ground and an executable successor |

Cover refers to the opponent's position at the sample tick. It does not predict
that opponent's movement during travel, missile paths, a safe route around the
sun/wall, or survival on foot. Adjacent samples are correlated; these counts are
not independent estimates of a bot's strength. No behavior weights are tuned
from them.

## Validation and reproduction

The targeted tests cover two actors under one quota, clone/repeat/reset,
zero/small budgets, local allocation priority, missing actors, cancellation,
query exhaustion, real remote-footing removal, new physical obstruction, and a
first-hit terrain fragment that must not count as planet cover. Existing local
cover observations use the same helper and retain their read-only/material tests.

All 176 selected tests pass: 93 AI unit tests, 21 physical mission/combat tests,
39 live-planning tests, nine scheduler tests, and fourteen combat/mission sensor
tests. Client compilation and formatting pass. AI/core all-target clippy passes
with warnings denied. Scenario all-target clippy reports fourteen existing
warnings in unrelated code; its strict warnings-as-errors invocation remains
blocked by those warnings, with none in the new probe or query-accounting code.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --disengagement-boundary true \
  --probe-disengagement-handoff true --probe-destination-cover true \
  --seed 7725194555774358125 --asteroid-interval 0 --trace true \
  --trace-start-tick 10718 --trace-end-tick 11800 \
  --out /tmp/destination-cover-quiet-p2
```

Remove `--probe-destination-cover true` for the matched control. Remove the
boundary option as well to compare the previous escape. The runner writes
individual samples to `destination-cover.jsonl`, their charged work to
`live-planning.csv`, and aggregate counts to
`report.json.live_objective_planning.destination_cover`.

`target/destination-cover/` contains the exact commands, comparison script,
reports, traces, binaries and validation logs. A verified archive is retained
under `/home/oldman/.codex/visualizations/2026/09/19/bot-destination-cover/`.
Default bot selection and Pi deployment are unchanged.

## Next decision experiment

Keep this probe diagnostic while building a small comparison of complete
successors. Use measured candidate sites to propose endpoints, reject stale or
incomplete evidence as permission, and obtain current geometry/cover validation
before committing. Compare transfer, continued escape within its original
deadline, and deliberate combat using the opponent-response and boundary models.
Include current weapon supply and capture/recovery value.

A covered endpoint is only one ingredient. Its flight must be feasible, and its
capture/boarding route remains unknown until the existing joint-trip planner
measures it. The quiet P2 regression, seed 2 success and main asteroid P2 loss
remain acceptance cases. No distance threshold or cover-only rule should replace
that comparison.
