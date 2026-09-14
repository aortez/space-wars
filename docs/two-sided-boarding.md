# Boarding from either side

This follows the cockpit/scale and upright-running work merged in `eb41da5`.
It removes an unnecessary return crossing before adding prospective jetpack
maneuvers to the landing planner.

## Player rule

Interact boards the assigned landed ship or escape cockpit from either side.
Each entrance has its own short floor probes and standing-capsule clearance
check. Cyan markers show the available entrances. Excavated ground and blocked
capsules make that entrance unavailable; detached debris is not planet floor.
Boarding still requires a balanced, supported, settled pilot on the vehicle's
planet, within the existing three-unit boarding range. Ownership, identity,
neutral input release and the shared physics/gravity step are unchanged.

The normal exit remains on the original side. Returning through the other side
does not switch the exit. If the normal exit is blocked, exiting remains blocked.

## Bot integration

Pilot observations and proposed landing sites retain the original exit and add
two optional boarding positions. Both v9 and v10 use the new world rule. The
ordinary ground task searches the union of entrance envelopes, retains the
chosen target while following its route, and replans when that target is lost.
Recovery checks both entrances before treating a ship's hatch as unavailable.

The v10 round-trip job still starts at the normal exit. Its reverse Dijkstra
search now seeds all footings within range of either entrance. There is one
forward search and one reverse search, with the existing directed-edge costs
and deterministic ties. The two fixed entrance slots bound the extra work per
node; they do not multiply the planning quota or create a second job. Single
hatch wrappers retain the independent reference-solver regressions.

Live planning carries both entrances in the landed-request identity. Positive
route dependencies also include the two hatch probe/clearance corridors: the
last route node can be within boarding range without coinciding with the hatch
floor. This is conservative: a change at the unused entrance can invalidate a
retained result. Existing physical sensors and live validation remain outside
the incremental objective-search quota.

Rebuild-placement forecasts retain their conservative original hatch test.
After the rebuilt ship lands, its actual boarding and return task use both
entrances. Outbound hull crossings, terrain gaps, jetpack fuel/recharge planning
and the larger mission-planning work remain separate next steps.

## Verification, 2026-09-14

All **609 scenario/AI tests** and **35 focused client tests** pass (one opt-in
client test remains ignored). Clippy completes with existing warnings.

The focused physical cases cover both seats, both sides, all three material
surfaces, cockpit boarding, an obstruction or excavation at either entrance,
dirty terrain queries, unchanged exits and preserved pilot/vehicle identities.
The route cases cover a nearer disconnected entrance, loss of the selected
entrance, one-way edges and identical results across resumable search batches.

The shared-physics crossing regression passes for both seats, with and without
a real terrain edit during flight. Each case completes its outbound flight,
capture, and boarding at the opposite entrance: **one flight instead of two**.
The lost-ship/pod/capture/rebuild/board/depart regression also passes, including
its four three-minute simulations.

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  --no-fail-fast -p scenario-spacewars -p spacewars-ai --lib --tests
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p engine-client \
  --bin engine-client client_scenarios::surface_sortie
cargo +1.89.0 clippy --locked -p scenario-spacewars -p spacewars-ai --all-targets
```

For cabinet playtesting, leave the ship at its normal exit, cross to its other
side, and Interact at the second cyan marker. Mine or obstruct one entrance and
check that the other remains usable. Exit again to verify the original exit
selection. The old [landing-route investigation](bot-objective-route-failures.md)
predates this rule; its two-flight timings are historical evidence, not current
acceptance thresholds.

## Pi deployment

Deployed this working tree to `sw-picade.local` on 2026-09-14 using
`./update.sh --fast --target sw-picade.local`. The installed client SHA-256 is
`93df566768d7c86aee9e6cad6af25e6465defe158ef49804b5694d3a5f8723e8`.
The kiosk restarted cleanly and automatically began a match with P1
`material_mission_v9` and P2 `material_mission_v10`. Saved settings retain
repeating bot matches, a 30-second launcher idle delay and a 900-second match
limit. Status and a screenshot confirm active gameplay; the user subsequently
accepted cabinet playtesting of the boarding behavior. Local deployment logs and the screenshot are
under `target/two-sided-boarding/`.
