# Earlier footing revalidation still does not complete recovery

The holding candidates remain **not qualified**. Preserving the explicit query
hint finds the same footing one survey earlier, and the fresh-failure guard
correctly ignores pre-arrival failures. Neither change produces a native rebuild
in the live holding cases. Preserve the successful contact-frame handoff control
with holding disabled.

| Live mode | Result | End tick | Build / boarding | Health | Relocations |
| --- | --- | ---: | --- | ---: | ---: |
| Continuous staging control | Blocked | 27300 | 0 / 0 | 100 | 4 |
| Map-handoff control | Complete | 27233 | 1 / 1 | 60.67 | 3 |
| Holding, continuous staging | Blocked | 25859 | 0 / 0 | 100 | 4 |
| Holding, map handoff | Blocked | 26584 | 0 / 0 | 68.85 | 4 |
| Explicit recheck, continuous staging | Blocked | 25859 | 0 / 0 | 100 | 4 |
| Explicit recheck, map handoff | Blocked | 26460 | 0 / 0 | 66.40 | 4 |

All failures end with `no accessible rebuild after four measured relocations`.
All six live pilots retain the original one ship loss and remain alive. Reduced
damage relative to the successful control does not compensate for losing recovery.

## What the fixes establish

All modes use the completed-frame native placement correction. The holding pair
adds a guard requiring an actual failed placement since arrival, on the same
planet and terrain revision. The explicit-recheck pair additionally carries the
requested bearing through a native search-history reset. Old geometry and visited
nodes still expire; route and placement checks run again.

In both holding cases, the terrain revision releases the first hold at
25537/25536. Without the explicit recheck, the survey at 25560 misses bearing 343;
the next accepts it at 25590. With the explicit recheck, **343 is the first refined
candidate at 25560** and passes the fresh preview immediately. Both versions
count this as relocation three. No survey cadence or work limit changes.

The earlier selection starts corrective walking at **25575** instead of waiting
for the later selection. However, both handoff candidates still lose support at
**25585** and suffer an impact at **25608**. The explicit recheck takes 33.60 damage,
versus 31.15 with holding alone. This experiment rejects the hypothesis that the
extra survey delay alone caused the fall.

The stale-failure guard also operates as intended. For example, both walking
candidates ignore the earlier report during 25856–25858, then release on the
new native failure at 25859. The holding handoff ignores latched status during
26569–26583 and releases at 26584; the recheck handoff waits during 26440–26459 and
releases at 26460. The original twenty-second holding and five-second missing-site
deadlines remain authoritative.

All recorded-controls forks are byte-identical to their matched controls across
5,655 rows. They rebuild at 27459, settle at 27557 and do not board under the fixed
tape. Holding never activates on those paths. Only the live control completes:
native build 27114, settling 27188, boarding 27232, task completion 27233.

## Remaining preview versus native placement disagreement

Correcting the contact's position did not make the preview and actual placement
use the same surface normal. The latest ground map for bearing 343 has local
normal approximately **(0.9848, -0.1736)**. The pilot's actual supporting contact
has local normal approximately **(0.6153, -0.7883)**: a **42.03-degree difference**.
These measurements share the same planet and terrain revision.

| First native rejection after revalidation | Distance from accepted preview | Normal difference |
| --- | ---: | ---: |
| Holding, continuous: 25829 | 0.08585 | 42.03° |
| Holding, handoff: 26464 | 0.07467 | 42.03° |
| Explicit recheck, continuous: 25829 | 0.08662 | 42.03° |
| Explicit recheck, handoff: 26370 | 0.07714 | 42.03° |

The fresh preview accepts offset -11. Actual attempts reject every negative
offset as `no_ground`, while positive alternatives lack a usable hatch route or
ground. Source code uses the supplied normal to orient the offset rays, so these
queries examine differently oriented footprints. The points also differ slightly;
this experiment does not isolate the normal's causal contribution.

The next focused comparison should test a shared placement orientation for
preview and actual construction, retaining current native clearance, settling and
hatch checks. Further holding tuning should wait for that disagreement to be
resolved. Keep both successful and failing controls.

## Freeze, audit repairs and validation

The [plan](rebuild-footing-recheck-plan.md) froze runtime source **9679c68**, **982
input hashes**, nine changed inputs, six commands and one release binary. Six
prefixes and twelve continuations completed with **zero simulation retries**.
Each control retains all **30 preceding files** byte for byte. Prefixes retain
23,768 rows, 47,536 pilot observations, 6,948 task steps and 396 world audits each.
Original task, ground, search, relocation, build and boarding bounds remain intact.

Two auditor-only repairs required **two audit resumes**, using the same
hash-verified raw outputs and binary:

1. The new recheck auditor stopped tracking when an escape pod was available.
   It now waits for an available full ship, so it records the pending surveys and
   counted selection correctly (`ef0f178`).
2. That exposed an exact JSON float comparison: 29.005197525024414 versus
   29.005197525024418. Site comparison now requires equal native f32 bits and exact
   identifiers, preserving the runtime's precision (`228fe8f`).

The original archives, original validation, both earlier summaries and audit logs
are retained. Only the two new auditor/test source files differ from the runtime
freeze. Native traces, reports and outcomes are unchanged; only recheck audit
files differ in the final archives.

Passed: **178 Rust tests** (25 native rebuild, 41 ground navigation, 49 surface
recovery, 63 harness), **969 Python tests** after the two audit regression tests
(967 at freeze), bot Clippy with warnings denied, formatting, locked Rust 1.89
release build and default-feature library/dependency check. Broad native Clippy
still **fails with the same 16 message/file diagnostic identities** as the baseline.

The [manifest](data/rebuild-footing-recheck-v1.json) and
[portable bundle](data/rebuild-footing-recheck-v1.json.gz) include full contact
probes, fresh/ignored failures, revalidation witnesses, surface-normal comparisons,
timelines, validation, frozen and repaired source, and the complete audit history.
Final archives use the suffix `-reaudit-394879659e93.tar.gz` under
`target/rebuild-footing-recheck/v1/archives/`:

| Mode | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control_walk` | 308,058,930 | 47,881,708 |
| `control_handoff` | 305,942,745 | 47,616,056 |
| `hold_walk` | 281,722,301 | 43,394,418 |
| `hold_handoff` | 297,353,667 | 45,990,977 |
| `recheck_walk` | 281,483,890 | 43,368,317 |
| `recheck_handoff` | 295,021,739 | 45,630,866 |

No defaults or frontier bot changed. This retained-case experiment does not
establish fresh-game or full-match qualification. Work remains local.
