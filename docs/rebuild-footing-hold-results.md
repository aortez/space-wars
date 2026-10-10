# Holding limits drift but regresses the retained recovery

The opt-in holding candidate is **not qualified**. It fails both live comparisons,
including the previously successful map-handoff case. Preserve the preceding
successful candidate with holding disabled.

| Live mode | Result | Terminal tick | Rebuilds / boardings | Health | Relocations |
| --- | --- | ---: | --- | ---: | ---: |
| Continuous staging control | Blocked | 27300 | 0 / 0 | 100 | 4 |
| Map-handoff control | Complete | 27604 | 1 / 1 | 60.67 | 3 |
| Continuous staging + holding | Blocked | 25856 | 0 / 0 | 100 | 4 |
| Map handoff + holding | Blocked | 26569 | 0 / 0 | 68.85 | 4 |

Every failure ends with `no accessible rebuild after four measured relocations`.
All pilots retain the original one ship loss. Reduced drift or less damage does
not compensate for losing recovery completion.

## What the comparison establishes

The [plan](rebuild-footing-hold-plan.md) froze source **ee22850**, 975 input hashes,
four commands and one release binary. The 11 changed inputs include the two
already documented auditor repairs from the preceding experiment; this iteration
introduced nine files/changes. Four prefixes and eight continuations completed
with **no simulation retries or audit repairs**.

Each control retains all **24 preceding files** byte for byte. All prefixes retain
23,768 rows, 47,536 native pilot observations, 6,948 task/control steps and 396
world audits. Original task birth, high crossing, staging arrival, task/ground
budgets, native build interval and relocation/search limits remain intact.

Both recorded-controls candidates are byte-identical to their respective
controls across all 5,655 rows. They repeat the native build at 27819, settling
at 28227 and no boarding. Holding never activates on those tape-driven paths.

The first live control changes occur only after actual build-site arrival:
25421 for continuous staging and 25422 for handoff. Native pilot observations
first differ one tick later. Holding retains the original ground task and its
clock, applies small proportional corrections and waits for real support.
Its maximum observed active foot error is about **0.1222 units**. That can exceed
the 0.12 arrival radius between ticks; it triggers correction, not an arrival.

## Why the candidate fails

The original pilot begins drifting before the terrain revision changes. Holding
does arrest that drift while active. However, it releases on the revision change
at 25537/25536, waits for a new preview, and counts another relocation at 25590.
That preview selects bearing 343 again. A further return to the same preview
uses the fourth relocation at 25830/26520. Both candidates exhaust the budget.

More fundamentally, keeping the reconstructed foot near the preview does not put
the native build query at the same location:

| Candidate / native attempt | Foot distance from preview | Native build point distance from preview |
| --- | ---: | ---: |
| Continuous holding, 25829 | 0.1202 | 0.4045 |
| Handoff holding, 26464 | 0.1207 | 0.4142 |

These distances come from the same rows and planet frame. Fresh previews at
bearing 343 accept offset -11, but the actual native attempts reject every
negative offset as `no_ground`. The holding candidate therefore corrects an
observable drift without resolving the placement disagreement.

The last holding release in each candidate also reacts to an older native failure:
at 25856 it uses the report from 25829, before arrival at 25855; at 26569 it uses
the report from 26554, before arrival at 26568. A future holding revision must
distinguish a fresh failure at the held position from this latched retry status.

Source inspection identifies a more direct issue to test first. Native
`rebuild_candidate` uses `support.position` and `support.normal` from the solver,
while placement rays use the current world. `PhysicsWorld::surface_contacts`
also exposes `local_surface`, specifically because the solver world point predates
body integration. The claim code already uses that local contact to avoid a stale
point on a moving planet. Rebuild placement still uses the old world point.
The observed 0.4-unit disagreement is consistent with this mismatch; this
experiment did not directly capture the pilot's local contact to prove its full
contribution. The next comparison should correct the native contact frame while
keeping holding disabled and retaining both original controls.

## Validation and evidence

Passed: **157 Rust tests** (6 native placement, 41 ground navigation, 47 surface
recovery, 63 harness), **961 Python tests**, bot library/tests/example Clippy with
warnings denied, formatting, locked Rust 1.89 release build and default-feature
AI library/native dependency check. Broad native Clippy still **fails with the
same 16 message/file diagnostic identities** as the preceding baseline.

The [manifest](data/rebuild-footing-hold-v1.json) and
[portable bundle](data/rebuild-footing-hold-v1.json.gz) retain reports/audits,
holding and drift timelines, native placement disagreements, stale failure
witnesses, recorded comparisons, validation logs, frozen source and provenance.
Full traces remain in verified lossless archives:

| Archive under `target/rebuild-footing-hold/v1/archives/` | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control_walk.tar.gz` | 305,650,989 | 47,161,745 |
| `control_handoff.tar.gz` | 310,176,367 | 48,001,682 |
| `hold_walk.tar.gz` | 279,678,149 | 42,795,228 |
| `hold_handoff.tar.gz` | 294,083,311 | 45,164,152 |

This failed opt-in experiment changes no defaults and includes no push, PR,
merge or deployment.
