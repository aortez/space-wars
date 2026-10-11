# Frozen observation plan: acquisition-defense survival regression

Replay `fresh-world1-v10-asteroids3-p2-{off,on}` from the completed
[24-game acquisition-defense screen](acquisition-defense-results.md).
This is diagnosis of a selected counterexample, not a new strength sample.
The retained loss-to-win qualification and all other screen results stay intact.

Use the original `c59fac6` runtime binary, SHA-256
`fb3e2feb2c7bed2ffb68841942f98291af3789ff41de6e4fd96f06f31e11375c`:
seed **14699744800433948105**, integrated v13 in P2, v10 in P1, asteroids every
three seconds, climb laser enabled only for P2. Preserve each arm's original
acquisition-defense flag and every gameplay, host and budget setting. Change
only the output path and add the native impact observer for both actors over
`[0,36001)`, with `impact-pod-control=bot`. Keep the existing full dense trace.
Run at most two games concurrently, to their original native match endings.

Commit this plan, runner and tests before the replays. Bind the original
summary, binary, selected archives, all imported auditors and relevant source
files by hash. There is no runtime rebuild, gameplay intervention, control
substitution, threshold tuning, default change, push or deployment.

Require each replay to preserve all nine original non-timing streams exactly,
plus equal non-timing reports, sensor records and charged planning ledgers.
Re-run the existing physical visit/route/budget and acquisition-defense/laser
audits; all derived gameplay summaries must agree with the retained screen.
Join every native impact row to its exact actor, clock, motion and actions,
with zero overrides and matching final native round receipts. Save raw hashes
before auditing, drain both jobs on failure, and never rerun a game to repair an
auditor. Stop interpretation if identity fails.

Inspect the full chain, including:

- The first control and physical differences after the handoff at 15,455.
- The prior damage that leaves P2 with about 25% hull at the trigger.
- Source-planet and arena clearances, outward/relative motion, boundary guard
  activation, braking, turn and thrust through ship loss. Distinguish vehicle
  origin from center-of-mass geometry and use actual motion for pod analysis.
- Current ship-damage and debris-contact clocks at 15,498 and the later baseline
  loss. Bind projectile kind/spawn time only to current contact evidence, and
  inspect native firing requests and actual shell-counter increments at spawn.
- Pilot damage, protection intervals, pod motion, recovery and final death in
  both arms; preserve the two common completed capture/departure visits.

The existing observer reports aggregate ship damage and bounded last-contact
provenance. It does not record every full-ship contact manifold or split hull
loss among simultaneous contacts. Respect that limit: a cannon label alone
cannot establish exact contact impulse or exclude additional damage sources.
Read the native collision/damage implementation to separate measured facts
from possible explanations. A negative escape forecast is not proof that every
physical alternative fails; no counterfactual control policy is tested here.

Archive generated evidence losslessly and verify every member before removing
uncompressed copies. Keep original archives unchanged. Report the causal limits
and the next bounded investigation or implementation hypothesis supported by
these observations, while retaining the failed experiment and its fixed screen.

```sh
python3 tools/diagnose-acquisition-defense-loss.py \
  --prior target/acquisition-defense/v1/summary.json \
  --out target/acquisition-defense/loss-v1
```
