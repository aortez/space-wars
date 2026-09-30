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
