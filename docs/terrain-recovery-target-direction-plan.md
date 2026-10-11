# Target-directed choice during bounded ground rejoin

Continued corridor sensing supplies valid steps after upright clearance returns,
but both retained attempts still cycle and fail. The late candidate chooses the
farther endpoint on 73 decisions when both fresh directions are open. Test the
direction preference alone, without assuming the nearer endpoint is reachable
standing ground or that moving away is always wrong.

Prespecify the same seed-20202 Angular/collapse-off P2 forks, 8025 and 8325.
Reconstruct and verify the original mission prefix; clone the native world,
both controllers and all host queues. Both arms select the existing bounded
rejoin and continued corridor sensing. Only the candidate selects the new
direction policy on its current recovery task. No controller resets. Verify
the complete control continuation against the corresponding retained candidate
trace from `7644bfc` (results `80289dc`). Retain every outcome and continue both
reactive arms to native match end, at most tick 36000. No later fork selection
or parameter tuning in response to the results.

The independently selectable policy is off by default. While the one bounded
rejoin is active, choose the fresh valid corridor whose actor-center endpoint
is closest to the retained standing target. On an exact distance tie, preserve
the previous direction. A sole fresh corridor remains eligible even if it points
away from the target. With no fresh corridor, preserve the existing retained
step fallback, including its less-than-15-tick, revision and spatial bounds.
The policy adds no deadband, new sensing, movement permission or progress reset.
Its purpose is to isolate the previous-direction preference, not to prove that
endpoint distance constitutes a route.

Keep native capsule/floor queries, stable support, strict 3-unit graph attachment,
eight-second/six-unit attempt bounds, overall task deadlines and retry counts.
The selection applies only to an active bounded rebuild rejoin; normal hatch,
claim and rebuild navigation keep their policies. Clone/reset preserve selection
without carrying an active attempt into a reset. Cached duplicate ticks retain
their earlier action. No new attempt, jump, flight or interact permission.

Freeze this plan, source and executable before the native comparison. Retain
inputs, decision/scenario checksums, all attempt observations, endpoint native
diagnostics, material audits and host work receipts. Separate direction changes,
short motion, standing-route attachment, native rebuild and aboard completion.
Only complete native recovery establishes recoverability. Check whether reversals
decline and actual attachment distance improves, without equating either with
successful recovery.

Run direction-selection, lifecycle, retained-step and observation-gate tests,
the AI library and recovery/navigation/gameplay suite, native observation tests,
Clippy, default-feature checks and formatting. Retain the negative results too.
If both attempts still fail, inspect the physical trajectory and retained scene
before adding another heuristic, retry or budget extension. This remains local
experimental work for #178; final #174 source integration and wider qualification
remain necessary before promotion.
