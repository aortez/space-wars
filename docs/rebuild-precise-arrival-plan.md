# Require precise arrival at coarse rebuild sites

The frozen placement probes show that bearing 324 still accepts a build at its
previewed point, but rejects all tested directions at the pilot's actual point
1.08876 units away. Test the existing precise foot approach for every selected
rebuild site, independent of whether sparse or refined search found it.

Add a default-off replay flag. When enabled, a coarse selected rebuild site uses
the existing 0.01 route endpoint range and 0.12 foot arrival threshold, with the
same proportional walking and posture handling as refined sites. Keep the site
identity and its `precise` search-origin marker unchanged. Do not expand holding
eligibility, preserve ground-task clocks and relocation counts, and do not change
native physics, placement directions, query work, construction or boarding.

Freeze four modes before observing results:

| Control | Candidate | Retained native direction policy |
| --- | --- | --- |
| `control_handoff` | `precise_handoff` | Existing successful handoff, radial off |
| `control_coarse` | `precise_coarse` | Failed coarse-site path, experimental radial on |

The radial policy remains confined to the retained coarse experiment. Its
original-point failure was independently isolated by the probes; this experiment
does not qualify that policy or address the separate bearing-343 normal mismatch.

Remove the read-only probe requests from all four commands. Each pair differs
only in the precise-arrival flag and output path. Controls must retain the 34
non-probe files from the preceding paths byte for byte. Preserve native contact
traces under recorded actions. Keep all original task, search, holding, ground,
relocation and fixed-end bounds. Freeze sources, commands and a release binary
before four original prefixes and eight continuations. No simulation retries;
auditor repairs, if necessary, must reuse verified original outputs.

Audit actual foot arrival, subsequent contact displacement, terrain revisions,
native placements, builds, settling, boarding, health and terminal reasons.
If arrival improves but subsequent drift still prevents construction, record
that result before changing holding. Full-chain success still requires an actual
native build and settled boarding while alive with no additional ship loss.

Run native, bot, harness and Python tests, formatting, locked Rust 1.89 release
and default checks, bot Clippy, and compare native Clippy to its retained baseline.
Export frozen source, validation, witnesses and verified archives. Keep defaults
unchanged and work local; these retained paths are not fresh-game qualification.
