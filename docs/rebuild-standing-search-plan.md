# Read-only standing-site coverage and conditional settling

The forecast selector rejects the unstable coarse build, then the unchanged
recovery task exhausts its five-second search for another footing. Inspect that
search without changing controls, placement rules, deadlines, default settings
or the forecast selector. This is a diagnostic, not a live search policy.

Freeze two retained replays from `rebuild-forecast-selection/v2`: `selection_handoff`
and `selection_coarse`, preserving their prefixes and both continuations. Add
read-only diagnostics on the live forks at these predetermined snapshots:

| Replay | Ticks | Reason |
| --- | --- | --- |
| Handoff | 27114 | Known positive placement, before construction |
| Coarse | 25373 | Known negative placement, before the forecast veto |
| Coarse | 25440 | First relocation survey after all current offsets are rejected |
| Coarse | 25560 | First relocation survey at terrain revision 22 |
| Coarse | 25710 | Last relocation survey before the fixed missing-site timeout |

Measure every existing local ground-map node within 24 units of the pilot foot,
including those omitted by the native two-unit spacing, minimum distance and
promising-seed filter. Retain the original local patch: at most 113 nodes, plus
one separate current native support point when eligible. Inspect all 26 existing
placement offsets in their original order. Do not add terrain samples, wider
offsets, longer routes or new physics. Keep the native radial-query flag for each
replay.

Retain both outbound routes: the coarse 0.8-unit endpoint range and the precise
0.01-unit range. Also call the existing staging-leg evaluator for the precise
route: a walk-only first leg of 2–4 units and a second leg of at most 24, combined
route at most 28. Record any proposed staging leg as a read-only proposal. Mark
coarse-only reachability explicitly; it does not establish precise arrival.

Check placement when the node has a permitted coarse, precise or staged route.
The current native support point needs no outbound relocation. Preserve all
native ground, hull, alignment, other-seat, hatch-footing and hatch-route checks.
Retain every rejected offset and the full measured maps, including rejected
edges. Repeat each accepted offset individually and require its detailed result
to equal the corresponding all-offset result before forecasting it.

For each geometrically accepted offset, run the existing two-body model from
this snapshot's epoch, with zero launch delay and the unchanged 120-step
horizon. This assumes a hypothetical replacement is inserted now at the queried
footing. It does **not** predict walking time, the eight-second build interval,
arrival eligibility, future terrain, opponents, damage or hatch execution.
A positive is conditional landing potential, not proof of a recoverable future
site. Whole selected-planet geometry remains privileged input.

The diagnostic examines the current support point first, then nodes ordered by
distance and bearing. Cap forecasts at 64 per snapshot; retain every excess
accepted placement as explicitly untested. Each forecast call advances at most
four steps, but the diagnostic finishes jobs synchronously offline. At most
7,680 projected physics steps and 3,028 placement-offset evaluations per snapshot
(114 × 26 plus 64 repeated accepted-offset checks). Do not interpret those work
counts as a gameplay frame budget.

Require physics, pilot and native recovery to remain unchanged. Require all
pre-existing raw replay files to match the preceding run byte for byte, except
wall-clock timing fields in the selector JSONL diagnostics; compare every other
field there exactly. Retain new timing files and the previous timing provenance.
Reuse old audits only after preserving every prior archive file except the two
timed selector logs, whose structural equivalence is checked separately.

Relate every snapshot to its actual native survey, search history, terrain
revision and task clock. Count sampled and omitted viable footings separately,
and distinguish geometry acceptance, route precision, staged access, forecast
positive/negative/inconclusive and cap-unexamined cases. Compare the current
support point at the two initial ticks to the already archived conditional
landing references; preserve any differences. Do not turn a negative sample
into a claim that the planet has no possible recovery site.

Freeze committed source, binary, commands, five snapshot ticks and bounds before
execution. Run one prefix and two continuations for each path, no fresh games,
seed search, runtime tuning, simulation retries or default promotion. Auditor
repairs may reuse verified raw files and must be disclosed. Test native
read-only behavior and work limits, audit classification, omitted measurements,
precise-route distinction, replay preservation and immutable frozen inputs.
Export the complete diagnostic reports, maps, forecasts, observed search,
validation logs and archive receipts in a portable review bundle.
