# Disengagement handoff: forecast the next flight, but model the opponent too

This follows [pursuit disengagement](bot-pursuit-disengagement.md) at `6929e64`.
The automatic successor rule tested here is **rejected and archived**. Two
previous wins become losses in wider comparisons. The retained change is an
optional, read-only forecast probe: it records the next flight without changing
the bot's decisions. Normal policies and the existing optional escape maneuver
retain their behavior.

In the tables below, **before** means the previous *opt-in escape experiment*,
not the default bot. In the difficult P2 asteroid case the original default
pilot survives 459.58 seconds; the escape experiment ends at 128.45 seconds.
Early combat improves that experimental result to 179.88 seconds, but does not
recover the original outcome.

## Correcting the visibility explanation

`CombatTarget.visible` comes from a first-solid ray toward the target center,
independent of ship aim. `ground_occluded` means that ray hit a body or terrain
fragment; it can include moving debris. Neither a missing target hit nor fragment
occlusion proves a durable protected route. The earlier notes and code comment
incorrectly attributed visibility to firing alignment; they are corrected.

At tick **7,022** in the retained P2 replay, the pursuit cooldown expires and
the opponent is visible. P2 still does not pursue: it owns no planet, the enemy
has 54% hull, and its last hit was at 6,561, outside the three-second response
window. Damage at **7,080** qualifies it for combat. Visibility returns at
**7,082**, when P2 resumes fighting with only 20.02 hull points. Waiting for
another qualifying hit is the relevant decision problem here.

## The bounded forecast

On successful disengagement, the optional probe considers at most four eligible
planets, ordered by current distance. Owned and currently deferred destinations
are excluded. It clones the mission pilot and observation, suppresses new combat
opportunities in the clone, and runs the existing transfer/launch guidance for
at most **360 fixed steps: six seconds at 60 Hz** per destination.

The approximate motor applies the observed thrust, turn, braking and wing
transition rules to those ordinary controls. Planet centers translate at their
observed velocities. Gravity stays at the current observed vector. The opponent
coasts at its observed velocity. Forecasting stops if the simulated mission
leaves transfer/launch, rather than fabricating local landing measurements.

Each candidate reports minimum enemy range, minimum nominal body clearance,
final destination distance, completed step count and six one-second motion
samples. The maximum work is **1,440 approximate controller/integration steps**
per successful escape. This is separate from the live graph/query allowance and
adds no world physics queries or graph expansions. The authoritative simulation
is never cloned or advanced by this probe.

This is a diagnostic model, not a clearance certificate. It omits contact,
asteroid strikes, changing gravity, prescribed orbital curvature, material cover
and opponent decisions. Its approach-frame estimate also omits the physical
support queries and hysteresis used by the real host. The live controller keeps
all existing landing, query, recovery and solar guards.

## The rejected decision rule and control comparison

The archived prototype has three modes beyond the old behavior:

- **Shadow:** record the forecast and retain the existing transfer decision.
- **Forecast:** select the closest forecast endpoint whose full six-second leg
  stays at least 300 units from the coasting enemy and outside nominal flight
  margins. If none qualifies, re-engage when weapons are available. A brief
  weapon wait retains the original escape deadline; incomplete forecasts can
  defer to the old decision.
- **Combat:** always resume a fresh bounded combat action immediately after
  separation. This tests whether forecasting adds value over unconditional
  early combat.

The decision uses only existing observations. The forecast makes no prediction
of damage, ammunition expenditure or the combat action's outcome. Thus this is
a transfer-admission experiment, not a complete utility comparison of missions.
Neither active mode remains callable in the current source; its source patch,
binary, tests and full-match commands are archived. Dense replay commands are
reconstructed from their saved report settings and marked accordingly.

## Physical results

The retained world seed is `7725194555774358125`. Asteroids arrive every three
seconds; quiet cases omit random asteroids but still contain opponent combat.
Only the v11 seat receives the experiment. Match limits are 600 seconds and the
live allowance remains 16,384 graph steps / 1,024 physics queries per update.

| Condition | Before: existing escape | Forecast rule | Immediate combat |
|---|---|---|---|
| Retained seed, quiet v11 P1 | P1 win, 201.10 s | P1 win, 195.78 s | Same outcome/time |
| Retained seed, quiet v11 P2 | **P2 win, 203.28 s** | **P1 time-limit win, 600 s** | Same outcome/time |
| Retained seed, asteroids v11 P1 | P1 win, 313.13 s | P1 win, 169.55 s | Time-limit draw, 600 s |
| Retained seed, asteroids v11 P2 | P1 win, 128.45 s | P1 win, 179.88 s | Same outcome/time |
| Seed 2, asteroids v11 P1 | P2 win, 94.70 s | Identical; no escape | — |
| Seed 2, asteroids v11 P2 | **P2 time-limit win, 600 s** | **P1 win, 540.03 s** | — |
| Seed 7, asteroids v11 P1 | P2 win, 568.07 s | Identical; no escape | — |
| Seed 7, asteroids v11 P2 | P2 win, 180.95 s | P2 win, 210.50 s | — |

Unlike the earlier extra seeds, **both new P2 cases activate the decision**:
seed 2 at tick 4,601 and seed 7 at 9,053. Together with the four original
contexts this gives six activated examples across three world seeds. That is
useful diagnostic coverage, not a general win-rate estimate.

The forecast chooses combat in the quiet cases, retained asteroid P2 and seed 7
P2. It chooses a different transfer in retained asteroid P1 and seed 2 P2. In
seed 2 the previous bot completes five sorties and wins on time; the candidate
completes one and eventually loses its pilot. Short endpoint distance alone is
not a sufficient measure of the successor mission's value.

## Calibration explains why a range threshold is insufficient

The P2 shadow replay reproduces the previous controls while recording the
forecast at tick **6,802**. For the actual planet-2 transfer, own-position errors
at one through four seconds are **1.07, 0.65, 0.10 and 0.92 units**. Opponent
errors over the same interval grow from **0.17 to 113.18 units** as it brakes and
turns. After combat starts at 7,082, own-position comparisons no longer describe
the same controller, so later errors are not pure integration error.

The opposite error occurs in the retained asteroid P1 trial at **5,672**. The
rule admits planet 2 with a predicted six-second minimum range of **345.16**.
The physical replay reaches **267.72** at exactly six seconds, while still
transferring. Own-position error is 11.67 units; opponent-position error is
84.86. This is an observed false admission, not merely a hypothetical concern
about the model. A coasting opponent can both exaggerate and understate danger.

The forecasts correctly expose the cost of braking and turning. They do not yet
predict the opponent's reaction reliably enough to authorize the next mission.
The unconditional combat comparison also loses a previously winning quiet
match, so replacing the failed admission rule with a blanket fight command is
not justified.

## Retained behavior and verification

Enable only the diagnostic probe with:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --probe-disengagement-handoff true \
  --seed 7725194555774358125 --asteroid-interval 3 --trace true \
  --out /tmp/disengagement-handoff
```

The probe requires an explicitly enabled escape seat. It is off by default and
is not exposed in the Picade UI. For a dense P2 calibration replay use
`--seconds 180` without `--require-finish`, plus
`--trace-start-tick 6600 --trace-end-tick 7500`.

Twenty full prototype matches and four focused replays were run, followed by
seven final controls. All reports pass physics and shared-quota audits. The
four original shadow matches reproduce the old recorded observations, actions
and allocation rows after excluding added diagnostics and dispatch timing.

The final retained probe reproduces all six activated baseline matches exactly
under those same comparisons. The normal, escape-disabled control also matches.
Across these seven controls, **12,456 raw records and 12,132 allocation rows**
match. This validates noninterference for the retained diagnostic behavior.

The prototype passes 87 AI unit tests plus 21 physical combat/mission integration
tests. After removing the active rule, all 85 final AI unit tests pass, including
three new probe tests for motion discrimination, read-only clone/reset behavior,
and the four-destination work cap. Final client compilation, formatting and
lint checks pass; lint warnings are confined to unchanged dependencies.

`target/disengagement-handoff/` retains commands, scripts, reports, raw traces,
calibration samples and binaries. `rejected-handoff-rule.patch` applies to
`6929e64`; only that prototype accepts `--disengagement-handoff
shadow|forecast|combat`. The current probe flag is different intentionally.
A verified archive is retained under
`/home/oldman/.codex/visualizations/2026/09/19/bot-disengagement-handoff/`.
Concurrent desktop timings are not Pi performance measurements. No deployment
or push is part of this investigation.

## Next bounded investigation

Follow-up: [opponent-response calibration](bot-opponent-response-forecast.md)
adds the three hypotheses and catches the earlier false admission. It also
identifies unmodeled arena-wall impacts and absent destination-cover surveys;
the probe remains read-only and now marks boundary validity limits.

Keep the own-flight forecast and compare a small set of plausible opponent
responses: coasting, braking to aim, and continuing pursuit. Calibrate those
against these dense traces before using them to admit a transfer. Report a range
of plausible contact times rather than one apparently precise safe distance.

Then compare complete successors: a feasible transfer into measured, durable
cover; deliberate re-engagement; or continued escape within the existing bound.
Include own weapon readiness, capture/recovery value and progress beyond the
short forecast endpoint. Nominal circular bounds and passing fragments cannot
stand in for intact material cover. Preserve the two win-to-loss regressions
and the 345-to-268 false admission as acceptance cases.
