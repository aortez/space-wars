# V12 finished-match comparison

At runtime checkpoint `604df48`, the destination planner changes one of sixteen
generated match comparisons. That switch is slower than continuing the current
trip and changes a v10 win into a v12 loss. The other fifteen comparisons retain
identical recorded physical outcomes. These results support keeping v12
experimental; they do not establish stronger overall play.

The [comparison protocol](capture-destination-planner.md#finished-match-comparison-protocol)
was fixed before examining outcomes. Four fresh worlds each run with asteroids
off and every three seconds. Each condition has a v10/v10 control and two
v12-versus-v10 matches, with v12 in opposite seats. The control is reused for its
two seat comparisons. Native local sensing and the shared four evaluator units /
384 remote-query allowance remain fixed, with no parameter fitting.

## Completed matrix

All **24 runs finish**, covering **181.51 simulated minutes**: six reach the
ten-minute timer and eighteen end on pilot death. All physics audits pass, and
remote work peaks at **127 queries/tick**, below the shared 384 limit.

| Paired subject results | Same-seat v10 control | V12 |
| --- | ---: | ---: |
| Wins / losses / draws | 8 / 8 / 0 | 7 / 9 / 0 |
| Pilot deaths | 6 | 7 |
| Completed capture-and-return trips | 51 | 52 |
| Completed recoveries | 8 | 8 |
| Ship losses | 14 | 15 |

The extra completed trip occurs in the changed, longer match; it is not evidence
of greater throughput. These are sixteen seat/condition comparisons across four
worlds, not sixteen independent world samples or a precise win-rate estimate.
Every no-switch run matches its control's recorded visits, round, final pilots,
planets, physics audit, combat, asteroid events and physical damage history.
Damage comparisons exclude only embedded diagnostic mission telemetry, whose
policy label is expected to differ.

The [retained result record](data/capture-destination-finished-v1.json) includes
all commands, binary/report hashes, both players' visits and abandoned attempts,
survival, ownership, recovery, timings, decision coverage and comparisons.

## Why most matches do not change

The sixteen experimental seats publish 10,213 evaluator reports, of which 2,914
are inactive. There are 343 reports with multiple numeric destinations and 340
with a complete preference (some contain only one candidate). One preference
has no current target and therefore cannot propose replacing an existing trip.

Thirteen reports prefer a different destination from the current one. Twelve
save only **0.59–4.72 seconds**, below their five-second/twenty-percent switching
margin. The thirteenth causes the single switch described below. These repeated
reports are correlated evidence, not independent decision trials. Missing remote
or local surface measurements and missing/incomplete flag routes remain major
limits; no unknown route is assigned an invented duration to encourage switching.

## The changed match

Seed **186767996776005237**, no asteroids, P1 v10 versus P2 v12. At tick **9527**
(158.78 s), P2 switches from the enemy-owned planet 1 to neutral planet 2. Both
players own one planet at the decision. The completed source report is tick 9525;
both routes have supported costs and the switch satisfies the existing guards.

| From the decision to the next completed trip | Continue planet 1, v10 | Switch to planet 2, v12 |
| --- | ---: | ---: |
| Predicted remaining time | 33.39 s | 25.23 s |
| Actual remaining time through boarding/departure | 28.85 s | 32.63 s |
| Claim tick | 11031 | 11258 |
| Departure tick | 11258 | 11485 |
| First ownership sample after that claim, P1–P2 | 0–2 | 1–2 |

The forecast favors switching by **8.16 s**; the actual next trip takes **3.78 s
longer**. The alternative's nominal transfer allowance is only **0.46 s**. Its
actual transfer, including climbing, takes **5.37 s** before the new approach
begins. Other phase-reference errors also contribute: continuing the current
trip finishes faster than its estimate. This is an observed model error, not a
request to fit a new constant to this one world.

The choices also have different strategic effects. Continuing removes the
opponent's only flag while gaining ownership; taking the neutral planet leaves
that enemy foothold in place. The time-only score treats both as one additional
owned planet. It does not value the ownership swing or denying rebuild ground.

The control eventually wins for P2 at **350.08 s**, when P1 dies in an impact.
The experimental match instead ends at **551.55 s**, when P2 dies to a missile,
with ownership 3–0 against P2. Later combat and recovery trajectories diverge;
this single pair does not isolate which downstream strategic effect causes the
defeat. The immediate timing regression is directly measured.

Two separate 200-second diagnostic replays preserve **550 recorded observations,
encoded actions and normalized mission records** before the switch. The replay
is sparse outside the dense tick window 9515–9549, so this is not a claim of
every-tick equality over the entire prefix. The first different encoded action
inside that dense window is P2 at tick **9527**. Both replay audits pass, and the
replayed claim/departure milestones match the full runs. These diagnostic replays
are excluded from the predeclared 24-match matrix.

## Reproduction and follow-up

```sh
cargo build --release -p spacewars-ai --example surface_mission_soak
python3 tools/compare-capture-destinations.py --finished-matches \
  --out /tmp/capture-destination-finished
```

To inspect the changed decision without waiting for the final match result:

```sh
target/release/examples/surface_mission_soak \
  --world generated --seed 186767996776005237 --mode duel --match true \
  --seconds 200 --p1-policy material_mission_v10 --p2-policy material_mission_v12 \
  --asteroid-interval 0 --live-objective-planning true --live-objective-seats none \
  --objective-graph-budget 4 --objective-query-budget 384 \
  --evaluate-missions true --survey-capture-alternative true \
  --trace true --trace-start-tick 9515 --trace-end-tick 9550 \
  --out /tmp/capture-destination-regression-v12
```

Repeat with P2 `material_mission_v10` and a separate output directory for the
control. Inspect the source report at 9525, the action at 9527, the climb at
9603, and arrival at 9849; compare the claim and departure ticks above.

Local artifacts are under `target/capture-destination-planner/finished` and
`finished-replay`; the latter includes commands, report/trace hashes and the
prefix verification. The checked-in record also retains this replay verification
and all thirteen alternative preferences, including those below the margin.

The next planning slice should address **transfer cost and mission value**:
account for the required turn/climb/approach stages, and distinguish securing a
first rebuild foothold, taking neutral ground, and removing an enemy flag.
Retain this world as a regression, then evaluate changes on fresh worlds. A
lower switching threshold or a larger search budget alone does not resolve the
observed failure. v9/v10 remain comparison policies and v12 stays opt-in.

Independent review found and resolved the unsupported v12/successor-continuation
combination and damage-accounting issue. The follow-up review found no outstanding
code findings. Nineteen evaluator tests, five Python accounting tests (334 Python
tests overall), formatting and strict AI Clippy pass. Negative CLI checks reject
the unsupported combination in either seat before controls; the legacy command
still reaches its existing handoff validation. This follow-up changes headless
validation and tests, so the installed Picade gameplay remains the already-tested
P1 v10 / P2 v12 pairing.
