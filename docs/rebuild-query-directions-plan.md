# Same-scene placement query directions

Compare radial and measured-normal queries at all five snapshots retained by
[standing-site coverage](rebuild-standing-search-results.md): handoff 27114 and
coarse 25373, 25440, 25560, 25710. The handoff is a positive control; the coarse
snapshots cover the initial veto, the first later search, a terrain revision and
the final search before timeout. Do not choose new ticks after seeing results.

For each snapshot, run the existing complete standing diagnostic under its
native direction flag. Clone the unchanged scene, flip only that flag and run
the diagnostic again. Contact normal means native support up for the current
support point and the measured ground-node normal for a hypothetical footing.
Radial up means the direction from the planet center through that same point.
Neither query chooses ship orientation directly: native measured landing feet
still determine the proposed pose, which must pass every existing guard.

Require identical terrain and replacement maps, pilot foot, node order, point
and normal inputs, sparse membership, coarse and precise routes, staging legs
and placement eligibility. Preserve all 26 offsets in their existing order and
all rejected attempts. Retain the local patch, 24-unit route bound and existing
2–4-unit walk staging with a second leg at most 24 and combined length at most
28. Do not change physics, geometry, hull clearance, landing alignment, other
seat checks, hatch footing or hatch access.

Use the unchanged snapshot-epoch, zero-delay conditional forecast for every
accepted pose, up to 64 per direction in current-support/distance/bearing order.
Each direction has its own cap; neither can consume the other's budget. Retain
excess accepted poses as untested. At most 128 forecasts, 15,360 physics steps
and 6,056 placement-offset evaluations per paired snapshot. Each forecast still
has 120 steps and advances at most four per call; jobs finish synchronously
offline. This is not a live frame budget or a forecast of future arrival.

Compare the native-direction report to the previous standing report in every
field except explicit forecast setup/step timing and per-chunk elapsed time.
Require live physics, native direction flag, actor, recovery and gameplay to
remain unchanged. Rerun the same two retained prefixes and four continuations;
do not enable either counterfactual direction in a playing controller. Require
all gameplay raw files to match byte for byte, with only known wall-clock
fields excluded in the two selector logs. Preserve old standing reports and
audits byte for byte as historical evidence, including their old timings.

Audit every footing/offset pair: geometry rejection, positive, negative,
inconclusive or budget-unexamined under each direction. Measure query angles,
proposed center differences and forecast outcomes; retain all underlying
reports and samples. Separate precise/staged access from coarse-only access
and current support. A positive tests conditional settling now, not walking,
future construction eligibility, the eight-second build interval, future
terrain, opponents, damage or hatch execution. Whole selected-planet geometry
remains privileged. Do not claim that sampled negatives rule out all landings.

Freeze committed source, binary, input hashes, commands, ticks and caps before
execution. No fresh games, seed search, runtime tuning, simulation retries or
default promotion. Auditor repairs may reuse verified raw output and must be
disclosed. Test read-only behavior, input equality, preserved baselines, frame
metadata, pair classifications, cap accounting and immutable frozen inputs.
Export a portable bundle with full paired reports, prior references, observed
search rows, validation, frozen source and complete archive receipts. Keep this
diagnostic local; a later policy decision needs its own evidence.
