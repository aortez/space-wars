# Select the qualified recovery candidate by name

Make the tested mission recovery combination an explicit, versioned profile
for #178 integration hosts. `standard` preserves ordinary recovery.
`terrain_recovery_v1` preserves the earlier terrain comparison, including its
original hatch arrival. `terrain_recovery_supported_hatch_v1` selects that
terrain profile plus the separately qualified supported hatch arrival. Keep
the existing individual switches for retained ablations; do not silently
change their meaning.

Selection occurs before the first mission decision and uses the existing paired
observation/intent driver. Preserve clone/reset, later ship-loss task creation,
cached decisions, native refinement cleanup, host planning budgets and ordinary
mission priorities. No other experimental controller, failed ground-rejoin
option, saved-normal bridge or settling forecast joins this profile. Launcher
and default bot selections remain unchanged.

First verify configuration equivalence and lifecycle behavior. Then freeze the
source and executable before two full-match integration replays: seed 20202,
Round, collapse off, selected P2; and seed 10101, Angular, collapse off, selected
P1. These cover the newly recovered failure and an existing successful recovery,
both seats and both terrain shapes. The previous sixteen-pair hatch comparison
remains the behavioral qualification; this step checks profile selection and
host wiring, not a new strength comparison or another policy experiment.

Require initial world and complete controller equality to the previous explicit
selection, every original action/decision/scenario checksum, native completion
and final state equality. Reuse the reactive opponent and original host queues,
cadence, deadlines and work budgets. Keep all first-run outputs, including any
failure; do not change the policy to make an integration replay pass.

Run the AI library, strict AI lint, default-feature build and formatting. The
120 physical gameplay tests from the hatch-arrival commit remain relevant:
this step adds profile selection without changing physical/controller rules.
Retain source provenance, independent action/final-state auditing and a portable
handoff with the source delta.

The current published #174 head remains `52f9e18`, which omits its supplied
local terrain work. Comparing all 2,138 frozen paths with the adjacent #174
checkout finds only one changed documentation file and two new handoff documents;
runtime source is unchanged. Current main `6611a0b` adds only README changes to
the integration base. Retain that receipt, continue on the frozen terrain, and
require exact published-source verification before final combined qualification.
Do not put an unpublished terrain snapshot into #178's remote branch.
