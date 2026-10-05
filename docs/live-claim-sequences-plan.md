# Diagnose the two opposite live-claim outcomes

The broader comparison retained no-stop as experimental. Diagnose the two
fresh integrated → no-stop outcome reversals, now selected known cases:

- `new-world1-p2-asteroids3`: integrated loses; no-stop wins.
- `new-world3-p1-asteroids3`: integrated wins; no-stop loses.

Both disputed captures succeed. Original report samples identify a laser ship
loss in the first integrated run, a cannon ship loss in the second no-stop run,
and fatal world-boundary impacts in both. The first no-stop run survives a later
asteroid ship loss but blocks during recovery. Dense center-of-mass motion is
needed to assess the pod trajectories; vehicle-origin velocity is insufficient
when pods spin rapidly.

## Four observational replays

Reuse the exact qualified binary from `e7683bf`, SHA-256
`7c5e6a65a612f0866648ad61252d89ee02b19ddcf60e7d9041a2cea9f055acd6`.
Commit this plan, runner and tests before freezing the four commands. Retain
every original game option, seed, policy, evaluated seat, shared planner,
asteroid setting, native controls and complete 600-second round limit.

Only add the existing impact observer: ticks `[0, 36001)`, pod control `bot`,
control-from tick zero. It records both actors. Its configured `--seat` remains
the original harness seat; analysis uses the item's evaluated seat. Run one
game at a time and archive it before starting the next.

Require every original deterministic stream hash and all non-timing report,
sensor and planning data to match. Retain both players' summaries and the
existing physical, configuration, stopping, planning and recovery audits.
The only extra raw stream may be `impact.jsonl`. Join every impact row to its
dense trace action, controls, goal, form, location and ship observation. Reject
any override, missing row, changed original stream, or unrecorded loss. Include
the final native physics step when reconciling losses and deaths.

An invalid run stops the diagnosis and preserves evidence. Do not retry games,
replace settings or change policies. An explicit audit resume may reuse original
verified raw data and change only this step's runner/tests; the plan, runtime,
binary, predecessor inputs and commands stay fixed.

## Questions and evidence

Record capture completion and departure clocks, subsequent mission/combat
transitions, ship-loss receipts, pod controls, damage/contact events, native
center-of-mass motion and final pilot damage. Inspect both actors so the shared
host and opponent's behavior remain visible.

For pods, report speed, spin, mass, point-to-boundary distance along current
velocity, and ideal full-brake distance using the observed brake acceleration.
That straight-line calculation excludes body extent, gravity, future contacts
and steering. It is a geometric diagnostic, not proof of unavoidable death or
evidence that an alternative policy would survive. Preserve raw measurements
and controls alongside any derived values. Report large motion changes as
observations; a last-contact label alone need not explain the whole impulse.

Distinguish a failed capture contract from later exposure, ship destruction,
pod impact or stalled ground recovery. Finish with the supported mechanism,
remaining uncertainty and one next testable hypothesis. No policy tuning,
counterfactual control override, default change, strength claim or promotion
belongs to these four selected replays. The separate World 3/P2 no-asteroid
regression remains unresolved by this scope.

Preserve all old archives, losslessly archive new complete streams, and bind
commands, hashes, observer parity, chronologies, original reports, diagnostics
and validation in a portable review bundle.

```sh
python3 tools/diagnose-live-claim-sequences.py plan \
  --out target/live-claim-sequences/v1
python3 tools/diagnose-live-claim-sequences.py run \
  --plan target/live-claim-sequences/v1/plan.json
```
