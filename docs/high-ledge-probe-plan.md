# Bounded native test of the rejected recovery ledge

This experiment tests the physical question left by the
[route diagnosis](recovery-route-diagnosis.md). It does not widen any
production survey or change a bot policy. Freeze this plan, sources and
executable before running the selected replay; do not tune the candidate
after seeing its result.

Replay only `new-world1-p2-asteroids3-no-stop` from the frozen arrival
experiment, with its original seed, both policies, asteroids, planning
budgets, control inputs and native ending. Change only the executable,
output directory and the opt-in diagnostic argument
`--probe-high-ledge 1:22575,22965:276:282`. There are zero fresh games.
The prior executable and archives remain intact.

At P2 ticks 22,575 and 22,965, retain complete native-world clones and the
consumed recovery observation. Preserve the already-running ground task's
absolute deadline, 23,678, for every fork. Record both players' actual encoded
inputs through that deadline. After the original game finishes, run:

1. A control fork from each source, replaying all recorded inputs. Require
   exact equality of both pilots' native state and the round at every tick,
   including the deadline. Also require the main replay to retain all original
   deterministic streams and non-timing physical/planning results exactly.
2. A clearance query for each of the original eight endpoint margins and
   three cruise heights at the 276 → 282 ledge. Use the ordinary capsule,
   exclusions and sampled corridor query. The diagnostic alone permits at
   most 20 units above the lower footing. Report both the unchanged ordinary
   ten-unit rejection and the actual clearance result.
3. If any higher candidate is clear, select the first in margin/height order
   and try it once in another native clone. If none is clear, preserve that
   rejection and do not attempt an unmeasured flight.

The execution fork uses the existing local flag navigator, initialized at
the real pilot pose and equipment charge. It receives the one additional
physically measured corridor, remeasured at every completed ground survey.
It uses ordinary controls, with no pose, velocity, charge, health, terrain
or ownership writes. Preserve every other encoded input from the tape.
The navigator is fresh, but the external cutoff remains the source task's
original absolute deadline. This isolates local flight feasibility; it is
not a replay of the entire mission-controller history.

Keep the existing 98% launch gate. Stop at the original ground deadline,
720 ticks after the first actual powered launch input, charge below the
5% landing reserve, round ending, lost equipment, controller block, or
verified crossing completion. Never replace an existing measured corridor
to make space in the eight-plan observation. A changed or missing fresh
corridor must go through normal flight revalidation.

Accept a local flight only with native support and balance on the correct
planet, speed below one, feet within the existing one-unit destination
window, charge at least 5% throughout, and completion strictly before both
cutoffs. Audit dense fork actions against the tape: only the selected pilot's
movement action may differ. Other inputs are recorded rather than adaptive
in a fork, so passing this test is not a survival or recovery qualification.

If clearance or execution fails, retain the exact rejection without trying
new endpoints, heights or timers. If it passes, the next runtime experiment
must supply a bounded terrain-flight forecast and demonstrate native claim,
rebuild, boarding and survival in full games before promotion. Default and
frontier bot selection stay unchanged in this diagnostic experiment.

Validation covers native-query noninterference, the harness, probe evidence
and candidate selection, formatting and Clippy. Retain the frozen commands,
all raw files in a hash-verified archive, the source observations, candidate
measurements, control parity, any physical execution traces, and validation
logs. Audit repairs may reuse completed recordings explicitly; they may
not rerun the game or change the frozen runtime.
