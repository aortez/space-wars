# Separating live-claim stopping from powered planning

The [factor diagnosis](integration-factors-results.md) found that powered
execution reproduced both losses and both retained gains in four selected
games. The losses first changed when the ground controller stopped during an
already valid native flag raise; the gains first changed when the shared host
delivered a route earlier. This experiment tests that distinction on current
main, based at `08e730f1ed276db06aada75fef253bb0d20b16dd`.

## Frozen comparison

Add a diagnostic `--live-claim-stopping false` option to the headless mission
harness. It disables only the supported-own-raise early return in
`planned_flag_target`. Powered sensing and planning, route/jetpack capability,
continuous walking, active-flight checks, valuation and destination retry remain
enabled in the ablated candidate. The option reaches newly created capture and
ground tasks and survives reset. Normal policy and device defaults stay unchanged.

Run three configurations in each of the four recorded settings:

| Configuration | Valuation/retry/powered execution | Live-claim stopping |
| --- | --- | --- |
| ordinary | Off | Not applicable |
| integrated | On for the evaluated seat | Existing behavior |
| no-stop | On for the evaluated seat | Disabled |

The settings, in order, are `known-rescue` (P1), `known-boundary` (P2),
`fresh-world0-p2-asteroids3` (P2), and `fresh-world1-p1-asteroids3` (P1).
Those historical names identify selected cases, **not fresh strength samples**.
Copy the seeds and commands from the recorded recovery comparison. Retain v13
against v10, three-second asteroids, the common shared routes host, graph/query
allowance 4/384, 4 Hz surveys, 15/4-second combat breaks, armed native controls,
all original observers and native complete matches (600-second limit). Optional
laser, acquisition defense and clearance remain disabled.

Build one Rust 1.89.0 release binary with `sensor-profile` after committing the
runtime switch, tests, runner and this plan. Freeze its hash and source commit,
the input documents and runtime sources, exact command arrays and original
source summary. Run **12 complete games**, at most two simultaneously, in the
fixed order above. In each group run ordinary and integrated first, then
no-stop. A failed run or audit stops later work and retains evidence. Do not
retry a game, alter a setting, or tune a coefficient in response to an outcome.
An explicit audit resume may reuse verified original outputs without rerunning
completed games; it cannot alter runtime, cases or this plan.

New main includes terrain changes absent from the historical runtime. Rebuild
and rerun both endpoints on this same new binary: do not require or imply exact
historical action/outcome parity, and do not combine old and new outcomes as
one strength estimate. The historical archive remains unchanged.

## Evidence and decision

Reuse the existing physical-visit, route, flight/fuel, continuation, planning
allocation, prediction, retry, disabled-combat and shared-recovery audits.
Check the actual per-seat stopping option in the report and every mission and
capture-ground trace. Retain both actors' actions, observations, claims,
departures, abandoned visits, losses, native round results and recovery.

Compare all three pairs per setting (**12 contrasts**). Keep full first-action
witnesses, equal-physical-state checks, native claim/support context, and the
chronology of both actors. Use common-horizon departure accounting and retain
raw progress denominators so earlier death cannot masquerade as efficiency.
The primary causal contrast is integrated versus no-stop; comparisons with
ordinary test whether the selected losses and gains persist on this runtime.

Report separately whether the loss signatures reproduce, whether disabling the
rule changes them, and whether the gains survive. If they do not reproduce on
main, say so and limit attribution accordingly. A favorable timing change in
four selected games does not establish a shipping fix. This step ends with a
mechanism diagnosis and supported next hypothesis, not promotion, broad seed
search, selectable-policy changes or deployment.

```sh
cargo +1.89.0 build --release --locked -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/diagnose-live-claim.py plan \
  --binary target/release/examples/surface_mission_soak \
  --out target/live-claim-ablation/v1
python3 tools/diagnose-live-claim.py run \
  --plan target/live-claim-ablation/v1/plan.json
```
