# Read-only placement coverage on the retained ledge

Probe the corrected replay at tick **24330**, where the original search accepted
a misaligned site and the corrected search rejected it. Use the already frozen
source tape and the original task history from `rebuild-replay/v2`.

Run one executable with the same verified prefix and both existing
continuations. Add one diagnostic call on the live continuation at that tick.
Require every original raw output and generated audit to be byte-identical to
the preceding corrected replay. Require the diagnostic's native relocation
survey to equal the live task's actual survey. No controls, task state, timing,
queries used by the task, production placement offsets or native rules change.

The diagnostic measures the same local ground patch and uses the same measured
walking/jumping/jetpack graph for outbound reachability. Retain the 24-unit
route limit. Inspect all measured nodes within 24 units of the current foot,
including nodes omitted by the original two-unit spacing or minimum distance.
The patch bounds this to at most 113 nodes; it does not include caves or a
planet-wide search. Save the entire map, rejected nodes and route results.

For each reachable node, call the shared native placement evaluator with:

1. The unchanged four offsets `[-8, -14, +8, +14]`.
2. Those same offsets first, followed by half-unit offsets strictly between
   8 and 14 on both sides, for 26 offsets total.

Require the first four detailed results to match. Preserve all actual ground,
foot-normal, radial alignment, hull, other-seat, hatch-footing and hatch-route
checks. No new offset extends beyond the original envelope. Check native
physics snapshots before and after the probe. All additional measurements are
diagnostic and cannot select a site or authorize movement/building for the bot.

Freeze source, binary, commands, bounds and hashes before executing. Preserve
all candidates and rejections, not just a best candidate. Separate any valid
previews already available with four offsets from those requiring new offsets,
and identify whether the standing point belongs to the original sparse set.

A positive result establishes a measured placement proposal with outbound and
hatch routes, not an executed relocation, settled ship or boarding. It can
justify a subsequent bounded live experiment. A negative result covers only
these discrete samples in this scene. Neither result qualifies recovery,
changes bot defaults or establishes a full-match improvement.

There is one retained replay, one probe, no fresh seeds, no adaptive parameter
search and no simulation retries. Audit-only repairs may reuse hash-verified
raw output and must be recorded. Archive all raw traces and export the complete
coverage report, validation logs, hashes and diagnostic source for review.
