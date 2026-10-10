# Test native settling on a bounded clone before changing placement

Straight round-foot sweeps still reject the valid recorded handoff: its actual
descent drifts sideways into a supportive corner before rotating onto both feet.
Test whether a short native rollout reproduces this behavior and measure its
cost. This experiment adds diagnostics only, not a placement gate or bot sensor.

On each fresh accepted native build in the retained controls, clone the complete
current `SurfaceSortieState`. Run at most **120 native steps**, each 16,666,667 ns,
using the ordinary scenario step, gravity, landing assist, collision response and
landing telemetry. Send explicit neutral movement, wings, weapons, mining and
impact controls for every seat on every step. Empty action lists are insufficient
because this simulation retains held controls. Existing hazards, pressure,
terrain updates, timers and other native systems continue on the clone. Do not
read future replay actions, reset clocks, alter geometry or relax settling rules.

Record the initial sample and each projected tick, including the pilot/ship,
planet frame and revision, landing telemetry and actual foot contacts. Continue
through the fixed horizon even after first settling to expose subsequent changes.
Stop early if the replacement becomes unavailable or the match ends. A positive
prediction requires the existing native full-ship settled gate. A negative means
only "did not settle within this horizon"; early termination without settling is
inconclusive. This full-world diagnostic includes information unavailable to a
normal bot and is not eligible for direct production use.

Measure clone time, native stepping, diagnostic sampling and live-state
verification separately, plus total API time and snapshot/body/collider sizes.
One measurement per retained build is a cost sample, not a hardware benchmark or
a claim about Raspberry Pi performance. Keep timing out of controller decisions.
Unit tests must verify the bound, repeatability excluding timing, unchanged live
physics/observations, fresh-build guards, and agreement with an independently
advanced native fixture under the same neutral controls.

Freeze committed input hashes, commands and a copied Rust 1.89 release binary
before running the two original controls: two prefixes, four continuations,
three native builds. Retain the existing round-foot probes and support gate off.
Require every preceding raw file and all 41 handoff / 40 coarse archive files to
match byte for byte before explicitly reusing the existing audits. No simulation
retry, seed search, horizon adjustment or retuning is allowed. Auditor-only
repairs may reuse hash-verified raw output and must be disclosed.

Compare forecast versus actual first contact, two-foot support and settling;
record first differences and pose errors across their common windows, including
all misses and inconclusive outcomes. Report the fixed-horizon classification
separately from the existing full recovery outcome. Agreement on these three
known builds only licenses a later candidate-placement experiment, with explicit
cost and information limits. It does not qualify complete coarse-site recovery,
bot defaults or frontier selection.

Run native rebuild, bot recovery/ground, harness and Python tests, formatting,
locked release/default checks, bot Clippy and the retained native Clippy baseline
comparison. Export the frozen source, controls, forecasts, comparisons, cost
samples, validation and raw archive receipts in a verified portable bundle.
