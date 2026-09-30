# Value bot promotion gate

This is the #142 evaluation accompanying #141 in the same PR. The candidate
is the existing **Value bot v13**: staged transfer references and capture
ownership value, compared with time-oriented v12 and v10 controls. All three
remain independently selectable. This PR does not introduce a new playing
policy or feed the conditional first-scan reference into live ranking.
The focused candidate change is v13's staged transfer and ownership-swing
ranking relative to v12. The previous zero-switch matches remain baseline
evidence; this gate adds directed owned-base decisions and prediction-to-trip
joins that those results did not establish.

## Frozen plan

Freeze the fixtures, runner and plan before executing this matrix:

- Three replays of the existing regression, seed 186767996776005237, candidate
  in seat two, no asteroids: v10, v12 and v13. These are regressions, not new
  strength evidence.
- Forty-eight directed physical trials: the existing destination fixture and
  a new three-planet value fixture, both seats, flag bearings 0.0/0.4/0.8/1.2,
  and v10/v12/v13. Seed 42, quiet, 180 seconds per trial. The new fixture gives
  the active player an actual owned/flagged third planet, leaving one neutral
  and one enemy destination. This distinguishes ownership value from the
  first-foothold priority. Mirror seat two. Do not choose a bearing after
  seeing a desirable outcome; retain every declared run and failure.
- Forty finished generated matches: four seeds derived from the first eight
  little-endian SHA-256 bytes of `capture-value-promotion-v1:{0..3}`, quiet or
  three-second asteroid pressure, v10/v10 controls plus v12 and v13 in either
  seat against v10. Each has the ordinary ten-minute match deadline. The four
  worlds, reused controls and mirrored policy seats are correlated.

The native sensor cadence, shared 4-graph/384-query allowance, controls and
policy thresholds stay unchanged. No cost coefficient is fitted to these
outcomes. Record build/policy identities, commands, raw hashes, work, timings,
physical audits, claims/boarding/departure, recovery/losses, switches and
finished-match outcomes. The existing tests cover commitment, recovery,
stale/unknown evidence and first-foothold value; new physical tests cover the
owned-base fixture and exact v10 fallback near match expiry.

Freeze each visit's first numeric current-mission prediction. For every
accepted switch, bind the exact source report and actual selected visit;
retain incomplete and abandoned trips. Unchosen alternatives receive no
actual-outcome credit. Full material-change histories are not collected by
this suite, so the corresponding prediction qualifier remains unknown. A
value/time preference disagreement alone does not prove an admitted control
decision.

Compare the frozen switch candidate's cumulative transfer/local costs against
arrival, landing, claim, boarding and departure at that same destination.
Keep the original source epoch throughout. These v12/v13 costs omit native
acquisition delay and combat exposure; recording their error does not remove
those limitations. Every recorded dispatch tick must reconcile its charged
work and stay within the common graph/query contract.

The [first-scan replay](capture-first-scan-success.md) separately audits
conditional geometry, scan and capture joins while preserving full controls
and sensor traces. Its diagnostic composition cannot serve as a general
acquisition guarantee or justify filling unsupported live costs.

```sh
CARGO_TARGET_DIR=target cargo +1.89.0 build --locked --release \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/validate-capture-value.py --out target/capture-value-promotion/v1
```

Use a fresh output directory. Publish a promotion-or-retain decision after
the run. Any zero-switch matches establish fallback behavior, not improved
destination selection; unknown enemy-route support or broad timing error
must remain visible in that decision.

## Audit correction before the completed study

The initial `v1` execution at `2d44819` retained the v10 regression and stopped
while auditing the v12 regression. Both matches completed, but exact Python
decimal equality rejected two representations of the same Rust `f32`:
`33.385185` in direct evaluation JSON and `33.38518524169922` in the report's
`serde_json::Value`. The audit now requires identical IEEE-754 f32 bits;
it does not introduce an error tolerance. A mutation test checks this boundary.
The failed summary and raw runs remain under `target/capture-value-promotion/v1`.
Rerun the entire unchanged plan in a fresh `v2` directory after committing
this audit fix. No policy, fixture, seed, outcome or duration was changed.

While reviewing `v2`, the duel command also incorrectly tied `--seat` to the
candidate policy seat. In duel mode both bots run; this flag selects the
observer used to serialize initial/final world observations. Pairing a seat-one
observation with a seat-two observation correctly fails the exact world check.
The corrected runner fixes the observer at seat one for all generated duels,
while the explicit P1/P2 policy arguments still swap the candidate. Directed
quiet fixtures retain their active seat. A command test covers every plan row.
Keep `v2` as failed pairing evidence, then rerun all 91 cases in `v3`; none of
its outcomes selects, removes or changes a case, threshold or coefficient.

## Results at `cdee3e0`

The corrected `v3` run completed all **91 cases and 99 paired comparisons**,
covering 1,251,913 physical ticks (347.75 simulated minutes). All physical
audits and per-tick shared-budget reconciliations passed. The largest remote
allocation was 127 queries in a tick, below the 384-query ceiling. All forty
held-out matches finished: thirty by pilot death and ten by the deadline.

The [compact result](data/capture-value-promotion-v1.json) retains every plan
row, command, outcome, visit, frozen prediction attempt, accepted switch's
source report, unknown count, raw hash, work audit and timing measurement. Its
projection manifest lists the omitted repeated fields and unchosen comparison
examples and binds the complete raw summary by SHA-256. It is not an unchanged
copy of that larger summary. The
[failed-audit manifest](data/capture-value-promotion-audit-attempts.json) binds
the two earlier executions and the unrecorded v12 report from the first failure.
No failed attempt is pooled into the corrected sample or silently discarded.

### Supported physical switches

The original two-planet fixture produced four v12 switches and two v13
switches; all six selected trips captured, boarded and departed. At flag
bearing 0.8, v13's first capture was 13.57 seconds earlier than v10 in seat
one and 7.90 seconds earlier in seat two. First departure was 29.75 and 24.22
seconds earlier respectively. The frozen switch references had these errors:

| Policy / active seat | Switch tick | Predicted minus actual departure seconds |
| --- | ---: | ---: |
| v13 / one | 760 | +1.994 |
| v13 / two | 789 | −6.563 |

The benefit is not universal. At bearing 1.2 in seat two, v12 delayed its first
capture by 3.38 seconds, although it departed 0.75 seconds earlier; v13 kept
v10's physical outcome. Bearing-zero runs in both seats made no capture with
any policy. All these cases remain in the record. The six phase/whole-trip
comparisons preserve their original source time and actual visit identities;
the six switches are correlated fixture results, not independent strength wins.

The known generated regression reproduced v12's switch at tick 9527 and its
loss. Its frozen 25.230-second alternative cost preceded a 32.667-second actual
trip from the prediction source. V13 made no switch and reproduced v10's win.
As in the earlier study, that is conservative fallback evidence, not proof that
ownership value or the conditional scan composition caused the win.

### Owned-base coverage and held-out matches

The three-planet fixture is a useful refusal case, not a demonstrated
enemy-versus-neutral value decision. Across its eight v13 trials, 17,216
reports contained **zero comparisons with multiple numeric destinations** and
zero time/value disagreements. Missing surface evidence accounted for 13,207
unknown candidate records; incomplete round trips and objective routes remained
unknown too. All three policies produced the same physical outcomes. These
report counts are repeated observations, not independent decision opportunities.

| Held-out same-seat measurements | v10 control | v12 | v13 |
| --- | ---: | ---: | ---: |
| Wins / losses | 8 / 8 | 8 / 8 | 8 / 8 |
| Destination switches | 0 | 0 | 0 |
| Completed sorties / recoveries | 37 / 4 | 37 / 4 | 37 / 4 |
| Ships lost / pilot deaths | 9 / 6 | 9 / 6 | 9 / 6 |
| First numeric current-trip predictions | 35 | 35 | 35 |
| Completed / abandoned / unfinished predictions | 28 / 5 / 2 | 28 / 5 / 2 | 28 / 5 / 2 |
| Completed median absolute error, seconds | 4.20 | 4.20 | 5.40 |
| Completed maximum absolute error, seconds | 44.73 | 44.73 | 44.73 |

Both candidates matched v10's recorded physical outcomes in all sixteen
same-seat comparisons. The eight v10/v10 controls are reused across candidate
seats; these are four world seeds, not sixteen independent worlds. None of the
generated v13 value preferences selected an alternative. The different timing
errors describe conditional references on unchanged trips, not better playing
outcomes. Acquisition, exposure and post-source material changes retain their
documented unknowns; failures remain in the denominator and receive no invented
completion duration.

The longest uninterrupted mission phase had a 37.78-second median and a
466.55-second maximum in each held-out group. This is a phase-duration proxy,
not a measurement proving that the ship made no physical progress throughout.
Per-player phase records and recovery/loss details are retained. Wall timings
are instrumented desktop measurements; part of the corrected execution
overlapped the earlier retained run. They are not a controlled CPU comparison
or Pi FPS benchmark. Native synchronous sensors, snapshot construction and
serialization remain outside the shared graph/query allowance.

### Decision and issue status

**Retain the existing defaults and keep v12/v13 experimental.** The directed
neutral cases establish some useful switches, but timing error remains broad,
and the owned-base cases do not yet establish an executable ownership-value
tradeoff. The held-out zero-switch results establish fallback parity only.
The #141 composition stays observational and does not fill unsupported costs.

This PR delivers #141 and this validation work for #142 together. Keep **#142
open**: a focused new evidence/policy candidate, a physical supported
enemy-versus-neutral decision, and direct no-progress instrumentation remain
unresolved. Repeating more zero-switch seeds is not the next implementation.
Preserve this corpus as regression evidence; a future candidate needs a new
predeclared held-out plan after those decision domains are supported.

Validation passed: 273 AI unit tests, 31 soak-harness tests, four physical
destination tests, all 489 scenario unit tests and 561 Python analysis tests;
Rust formatting and strict Clippy for the AI library, harness and changed
integration test. Strict scenario Clippy still reports seven existing warnings
in `pilot.rs`, `render.rs`, `surface_sortie.rs` and `lib.rs`, all unchanged from
the PR base. The release binary used Rust 1.89.0. No device deployment or
default-policy change is part of this diagnostic and evaluation work.
