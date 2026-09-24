# Opponent responses, forecast validity, and the missing arena wall

This follows the [disengagement handoff investigation](bot-disengagement-handoff.md)
at `d28fd5a`. The retained change extends the optional, read-only probe. It adds
three opponent responses, records arena-boundary warnings, and exposes the real
enclosing boundary in the mission observation. **No active escape, transfer or
combat decision changes.** The previous automatic handoff rule remains rejected.

The pursuit response catches the earlier false admission: the planet-2 transfer
previously predicted a minimum separation of **345.16** units, while physics
reached **267.72**. The new pursuit hypothesis predicts **279.12**. However, the
wider comparison shows why this is still a diagnostic, not a safety test:
several transfers hit the arena wall, the response spread does not consistently
contain actual motion, and the handoff has no measured destination-cover data.

## What is forecast, and how much work it takes

For each of at most four eligible destinations, the existing cloned own-flight
controller runs for at most six seconds at 60 Hz. The same own trajectory feeds
three independent opponent responses:

- **Coast:** retain observed velocity, preserving the earlier reference exactly.
- **Brake to aim:** apply ordinary braking and bounded rotation toward the
  shared combat lead solution, without thrust.
- **Pursue:** use the mission's free-flight approach speed and velocity guidance,
  then switch to a local aiming/braking response inside 250 units. Thrust and
  turning use the ordinary motor limits, including acceleration and cruise taper.

These models use observed position, velocity, angle and spin. They do not read
the enemy brain or its private target, cooldown, PWM phase or route state.
Enemy gravity and wing state are absent from the current combat observation;
the two powered models explicitly assume zero gravity, open wings and a fresh
PWM accumulator. The nearest nominal planet estimates their braking frame.
Neither model includes weapon recoil, damage, contacts, climb/solar routing or
combat breaks. Our own forecast retains its previous gravity and orbital
approximations.

Work is capped at **1,440 own-controller steps plus 4,320 opponent steps** per
successful disengagement. The latter includes the cheap analytic coast branch.
It adds no physics query, graph expansion, authoritative world clone or physics
step. These diagnostic steps are separately counted; they are not charged to
the live landing planner's graph/query allowance. There is no new UI option or
default policy change. A future runtime planner must schedule this work within
its own allowance before adopting it.

Each response reports its minimum range, first entry into the **300-unit risk
radius**, nominal planet/sun clearance and up to six one-second motion samples. This
radius reproduces the earlier admission threshold; it does not prove weapon
contact, aim, ammunition or line of sight. `range_entry` gives earliest entry
among the hypotheses and latest entry only when every response enters during
the evaluated horizon. A null latest time is an unobserved entry, not safety.

## Calibration results

The original seed is `7725194555774358125`. Six activated handoffs cover quiet
and three-second asteroid conditions, both v11 seats, and two additional seeds.
Only the v11 seat enables escape and the probe; the opponent remains v10.

| Flight after escape | Coast minimum | Pursuit minimum | Observed minimum / qualification |
|---|---:|---:|---|
| Original seed, asteroid P1, normal planet 0 | 375.33 | 289.77 | 276.77 over six seconds |
| Same handoff, rejected planet-2 alternative | 345.16 | 279.12 | 267.72 over six seconds |
| Original seed, asteroid P2, planet 2 | 38.55 | 125.97 | 177.3 before own controller switches to combat |
| Original seed, quiet P1 | 275.68 | 265.72 | Forecast invalidated by boundary margin at 1.27 s |
| Original seed, quiet P2 | 109.73 | 52.38 | Forecast invalidated by boundary margin at 0.42 s |
| Seed 2, asteroid P2 | 485.69 | 317.57 | Already within boundary margin at handoff |
| Seed 7, asteroid P2 | 144.01 | 188.07 | Own controller switches to combat at 2.50 s |

The alternative planet-2 trajectory comes from the archived rejected prototype.
Its observation at tick 5,672 is identical to the current replay after excluding
the newly exposed boundary. Thus its unchosen forecast can be checked against
the actual alternative flight without reintroducing that policy.

On the normal asteroid P1 transfer, entry into 300 units occurs after **335
ticks**. Coast predicts no entry; pursuit predicts **350**, 0.25 seconds late.
For the rejected alternative, pursuit predicts **340**, five ticks late. On
asteroid P2 it predicts entry at **138 ticks**, matching the physical replay;
coast predicts 130. These examples improve the diagnosis but are not a general
accuracy or win-rate estimate.

Across all 34 one-second samples before our controller changes mission, mean
opponent-position error is 70.08 for coast and 32.69 for pursuit. Own-position
error is 56.53 because this set includes flight after unmodeled wall impacts.
Among the **19 samples before the boundary warning**, mean own-position error
is 4.24, coast error is 52.51, and pursuit error is 7.98. These are correlated
samples from seven transfers at six handoff states, not independent trials.

Do not interpret the three responses as conservative reachability bounds.
Only **6 of those 19** observed ranges fall between the smallest and largest
predicted ranges; the most optimistic lower endpoint is about 13 units too
large. The alternate planet-2 pursuit also enters the existing 65-unit nominal
planet margin by 7.2 units. Such margins are diagnostic routing bounds, not
material collision certificates. No new automatic admission rule is justified.

## The wall explains the large own-flight errors

The previous mission observation exposed planets and the sun, but omitted the
enclosing arena. All three suspect trajectories abruptly lose outward velocity
near its edge:

| Case | Warning after handoff | Observed velocity jump | Speed-vector change |
|---|---:|---:|---:|
| Quiet P1 | 76 ticks | 94 ticks / world tick 9,925 | 80.74 units/s |
| Quiet P2 | 25 ticks | 37 ticks / world tick 11,197 | 117.47 units/s |
| Seed 2 P2 | 0 ticks | 17 ticks / world tick 4,618 | 74.74 units/s |

Quiet P2 and seed 2 retain hull contact with `PhysicsId(1)`, the world boundary,
in the saved landing diagnostics. Quiet P1 retains no live contact in that
snapshot; its origin is 6.41 units inside the wall at the velocity reversal.
The wall explanation for that third case is therefore a strong geometric
inference, not a recorded contact event. Hull health stays unchanged at all
three jumps.

`MissionObservationV1.boundary` now reports the actual configured arena center
and radius. Its interior is navigable, unlike the exterior of a planet. Reading
it adds no query and works in fixed fixtures as well as generated matches.

Own and enemy forecasts report their minimum boundary clearance and first entry
into a **20-unit navigation margin**. This is deliberately not an exact collision
time: the physical wall is a 192-segment polygon and the hull rotates. The probe
keeps the six-second diagnostic tail, but its boundary marker identifies where
a free-flight prediction must no longer imply safe separation. It does not
simulate a bounce, move the ship, or add boundary avoidance to the live pilot.

## Why a covered successor cannot yet be selected

All six handoff observations have **zero landing sites, zero cover samples,
and `site_query: not_requested`**. That is missing information, not proof that
the planets lack cover. The current live planner deliberately requests local
landing details only when needed for the current approach planet.

Existing `LandingCover` checks ray hits against the surviving material of the
candidate's own planet at grounded, approach and departure heights. Detached
fragments and the old circular outline do not qualify. Those measurements are
useful to reuse, but cover against the enemy's current position does not certify
an entire future transfer or a landing while the enemy moves.

The next bounded slice should therefore:

1. Make the optional escape/transfer guidance account for the enclosing wall,
   including braking room. Preserve the three wall cases and both prior
   win-to-loss regressions as acceptance cases. A large separation near the
   wall must not automatically authorize the next flight.
2. Request a small destination shortlist through the shared planning budget.
   Mark unavailable, deferred or invalidated cover explicitly. Reuse measured
   material cover and existing landing/return-route feasibility rather than
   promoting nominal bounds into landing evidence.
3. Compare the complete successors: boundary-feasible escape into measured
   cover, deliberate combat with current weapon supply, and continued escape
   within the original deadline. Score capture/recovery value and progress
   beyond the six-second endpoint. Revalidate before physical commitment.

The pursuit hypothesis is worth retaining. Expanding the number of nominal
responses or tuning another distance threshold is not the highest-value next
step while wall constraints and destination evidence are missing.

Follow-up: the [boundary-guidance experiment](bot-boundary-escape.md) avoids
the three recorded wall cases but regresses a quiet match. It remains separately
opt-in, with both the previous escape controls and ordinary defaults preserved.
That report also specifies the next bounded destination-cover probe.

## Reproduction and verification

The same opt-in flag enables the expanded probe:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v11 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 0 --probe-disengagement-handoff true \
  --seed 7725194555774358125 --asteroid-interval 3 --trace true \
  --trace-start-tick 5671 --trace-end-tick 6033 \
  --out /tmp/opponent-response-forecast
```

Two seven-game passes were run: the three-response probe, then the final probe
with boundary diagnostics. Both reproduce the six prior activated matches and
the ordinary escape-disabled control. All 14 new full-match reports pass the
physics and shared-quota audits. The final comparisons match **12,456 previous
raw records and 12,132 allocation rows**, excluding only added diagnostic
telemetry, the new boundary field and dispatch timing. Extra dense trace rows
are used for calibration; the original recorded observations/actions are all
still checked. The motor refactor also preserves the old coasting minima.

Final checks: 88 AI unit tests, the new boundary-observation scenario test, and
21 physical combat/mission integration tests pass. The new tests exercise
finite acceleration/turning, pursuit versus coasting, censored range-entry
times, bounded work, and boundary-warning direction and persistence. Client
compilation, formatting and lint checks pass; existing dependency warnings
remain.

`target/opponent-response-forecast/` retains the commands, comparison script,
reports, traces and binaries. A verified archive is retained at
`/home/oldman/.codex/visualizations/2026/09/19/bot-opponent-response-forecast/`.
The archived earlier planet-2 replay and prototype are included for the
counterfactual calibration. No Pi performance or gameplay improvement claim is
made from these desktop diagnostic runs.
