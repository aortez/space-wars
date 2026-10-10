# Current-frame placement improves the retained recovery

The native coordinate correction is verified, and the map-handoff candidate
completes its isolated recovery **371 ticks (6.18 seconds) earlier**. It does not
prevent the earlier fall or impact damage. The walking-only case still fails.
Keep the correction experimental pending broader qualification; keep the
[failed holding candidate](rebuild-footing-hold-results.md) disabled.

| Live mode | Result | Native build | Native boarding | Task end | Health | Relocations |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Continuous staging control | Blocked | — | — | 27300 | 100 | 4 |
| Map-handoff control | Complete | 27114 | 27603 | 27604 | 60.67 | 3 |
| Continuous staging + contact correction | Blocked | — | — | 27300 | 100 | 4 |
| Map handoff + contact correction | Complete | 27114 | 27232 | 27233 | 60.67 | 3 |

Both failures retain `no accessible rebuild after four measured relocations`.
All four live pilots retain the original one ship loss and remain alive.

## What was corrected

Native rebuilding used the solver's world contact point and normal, which
precede integration of the planet body. Placement previews and collision rays
use the completed world. The opt-in correction transforms the selected support's
local point and normal through the current Rapier planet frame, as claim
anchoring already does. Support, balance, relative speed, ownership, build time,
offset search, landing and hatch checks remain unchanged.

Separate diagnostic streams capture the selected contact in both frames. At the
live build tick 27114, the control's reported standing point is **0.345110 units**
from the current local contact. The candidate's error is **0.000055 units**.
Across all measured native attempts in either live candidate, the largest error
is below **0.000057 units**. The diagnostic also verifies that each placement
report agrees with its actual native candidate point. This directly establishes
the coordinate mismatch; the preceding holding experiment only inferred it.

The walking-only comparison changes placement reports but retains every physical
pilot observation, task state, applied action, landing diagnostic and world audit.
It therefore retains exactly the same failure.

For handoff, physical observations first differ when the ship is built at 27114.
Both choose offset -10, but the corrected anchor changes the replacement's pose
and hatch approach. Task state first differs at 27115 and controls at 27181.
Both replacements become physically settled on two feet at **27188**; the
candidate remains settled through boarding at 27232 and task completion at 27233.
Its gain is earlier boarding, not earlier construction or settling. Both still
take **39.33 impact damage at 25613** before reaching the final build site.

The recorded-controls forks provide separate native evidence. Both corrected
forks rebuild at **27459 instead of 27819**, settle at **27557 instead of 28227**,
and remain settled through 28695. Neither boards under the fixed original tape.
Their end at 29421 is not counted as task success; health remains 91.27 with one
ship lost. Applied recorded controls are unchanged even when generated diagnostic
task controls differ.

## Frozen comparison and validation

The [plan](rebuild-contact-frame-plan.md) froze source **3b881de**, **979 input
hashes**, seven changed inputs, four commands and one release binary. Four
prefixes and eight continuations completed with **no simulation retries or
audit repairs**. Each control retains all **26 preceding files** byte for byte,
despite the additional read-only contact probes.

All prefixes retain 23,768 rows, 47,536 native pilot observations, 6,948 task steps
and 396 world audits. Original task birth, crossing, activation, search cadence,
relocation budget, ground allowance and native build timer remain intact. All
continuations pass the opponent-control, world and native recovery audits.

Passed: **175 Rust tests** (24 native rebuild, 41 ground navigation, 47 surface
recovery, 63 harness), **964 Python tests**, bot library/tests/example Clippy with
warnings denied, formatting, locked Rust 1.89 release build and default-feature
library/dependency check. Broad native Clippy still **fails with the same 16
message/file diagnostic identities** as the preceding baseline.

The [manifest](data/rebuild-contact-frame-v1.json) and
[portable bundle](data/rebuild-contact-frame-v1.json.gz) retain all contact probes,
native attempt reports, event witnesses, timelines, pairwise differences,
validation logs, frozen sources and provenance. Full traces remain in verified
lossless archives:

| Archive under `target/rebuild-contact-frame/v1/archives/` | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control_walk.tar.gz` | 311,767,370 | 48,372,886 |
| `control_handoff.tar.gz` | 316,305,359 | 49,213,624 |
| `contact_walk.tar.gz` | 308,058,690 | 47,881,084 |
| `contact_handoff.tar.gz` | 305,942,505 | 47,615,870 |

The remaining problem is footing control through terrain revalidation. A next
holding experiment should use the corrected native anchor and distinguish fresh
placement rejection from a latched pre-arrival failure. It must retain the
successful handoff control and account for every revalidated relocation.
These two trajectories do not establish fresh-game or full-match qualification.
No defaults or frontier bot changed; all work remains local.
