# Compare a shared radial placement direction

The completed-frame contact correction removes the stale build point, but the
failed footing rechecks still preview bearing 343 with a ground-map normal about
42 degrees from the actual pilot contact normal on the same terrain revision.
Both paths rotate placement rays using their supplied normal. Test a common
radial query direction derived from the current standing point and planet center.

The experiment changes the shared offset-query function, covering coarse and
refined previews and actual native placement. It does not copy preview geometry
into construction. Both landing feet are raycast again; their measured positions
and normals determine the proposed ship pose. Keep all existing surface-slope,
hull-clearance, radial-landing, hatch-footing and hatch-route gates and thresholds.
Support, balance, speed, ownership, eight-second construction, settling and
boarding remain authoritative. Record the actual query direction in local
coordinates only when the flag is enabled; legacy reports remain unchanged.

Freeze these six modes before observing outcomes:

| Control | Matching radial candidate | Holding / explicit recheck |
| --- | --- | --- |
| `control_handoff` | `radial_handoff` | Off / off |
| `control_recheck_walk` | `radial_recheck_walk` | On / on |
| `control_recheck_handoff` | `radial_recheck_handoff` | On / on |

The first control is the successful contact-frame handoff. The other two are the
failed explicit rechecks, preserving both staging variants. Every control must
retain all 32 files from the preceding experiment, including its final repaired
audits. Each matched command differs only in the orientation flag and output
directory. Retain recorded-controls forks and all negative results. Do not add
candidates or tune thresholds after seeing results.

Use the same tape, original task birth 16820, crossing 23226, activation 23767,
ground allowance 5400, four-relocation cap, bounded search and holding timers, and
fixed end 29421. The flag changes only after the original prefix. P1 remains on
the tape. Freeze sources, commands and a release binary before six prefixes and
twelve continuations. No simulation retries. Auditor-only repairs may resume
only against hash-verified original raw output and must be disclosed.

Audit query directions against each placement's own radial standing frame, native
contacts, preview acceptance, fresh/ignored failures, revalidation, actual arrival,
damage, native construction, settling, boarding and terminal reasons. Review
differences from each matched control, including recorded-controls physics; the
native query change can affect construction even under recorded actions.

Require focused native, bot, harness and Python tests, formatting, locked Rust
1.89 release and default builds, and bot Clippy with warnings denied. Compare
broad native Clippy failures against the retained diagnostic baseline. Preserve
raw archives and export reports, witnesses, validation and source provenance.

Full-chain success requires native rebuilding and settled boarding while alive
with no additional ship loss. Distinguish preserving an existing success from
rescuing either failed recheck. These retained paths do not qualify fresh games
or full matches. Keep defaults unchanged and work local.
