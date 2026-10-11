# Continued short corridor sensing during bounded rejoin

The retained standing-rejoin comparison moved both trapped pilots but completed
neither recovery. The later attempt regained upright clearance at tick 8530,
outside the strict 3-unit standing-graph attachment distance. Ordinary posture
sensing then stopped supplying short corridors. Test that sensor handoff alone.

Prespecify the same seed-20202 Angular/collapse-off P2 forks, 8025 and 8325.
Reconstruct and verify the original mission prefix, then clone physics, both
existing controllers and all host queues. Both arms enable the previous bounded
rejoin. Only the candidate additionally requests continued short corridor probes
when stable, balanced, supported posture already has upright clearance. Verify
the entire control continuation against the corresponding retained rejoin trace
from source `8f4f554` (results `eddda37`). Do not reset a controller or choose a
different fork after observing outcomes. Continue all arms to native match end,
at most tick 36000, with reactive opponents.

The host request is independently selectable and off by default. It exists only
while the original rejoin is active and within its eight-second/six-unit limits,
the task is running within its overall budget, controls and queries are ready,
the actor owns and is supported by the retained planet, and no ship is available.
Rejoin success, block, cancellation or expiry stops the request. Task reset keeps
selection but clears the attempt; selection grants neither a new attempt nor a
renewed deadline. Native sensing reuses the exact capsule, fixed-pose sweep,
query-only separation, clearance margins, minimum step and retained-floor tests.
No graph-attachment threshold, collision geometry, jump, flight, ownership or
native rebuild rule changes. Ordinary sensors and normal mission defaults keep
their existing behavior.

Freeze source, plan, test harness and executable before comparisons. Retain every
attempt decision, native audit samples, host work, full controls and checksums,
and read-only endpoint diagnostics. Check the new sensor fields without treating
diagnostic sweeps as controller permission. Record the first changed observation,
control and physical state. Separate short motion, route attachment, native
rebuilding and boarding; only complete native recovery establishes recoverability.
The early attempt may remain unchanged because it never had stable upright
clearance. Additional probes may still fail physically or expose oscillation.

Run lifecycle and native read-only sensor tests, the AI library, recovery and
gameplay checks, Clippy, default-feature checking and formatting. Keep all raw
outcomes, validation receipts and a checksummed incremental handoff. This local
experiment does not update PR #178, deploy a bot, change defaults or settle the
Round executor, Angular/on distance gate, placement cost or final #174 integration.
