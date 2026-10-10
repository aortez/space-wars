# The retained Angular recovery now runs without a controller reset

The opt-in ordinary RulePilotV4 driver recovers the immediate Angular case,
preserving the original task clock, three landing rejections, landing retry
count, relocation budget, native rebuild history and surrounding combat state.
It rebuilds, settles, boards at tick **21,849**, and returns to combat. It later
loses the ship at **23,824**. This is a completed recovery, not uninterrupted
survival or qualification of a new default bot.

All work remains local on `bot-terrain-recovery-ablation`, in
`/home/data/workspace/spacewars-handoffs/integration-20261009/combined`.
The original #178 checkout remains at `cada1eb`. Neither GitHub PR, gameplay
default nor deployed instance changed. The terrain source is still the frozen
#174 handoff; integration against its eventual published source remains due.

## What the component comparison established

The [component plan](terrain-recovery-components-plan.md) was frozen at
`430eb8a` before execution. All four arms share the same physical fork at 8,065,
identically initialized fresh tasks, reactive opponent and 18,000-tick window.

| Enabled configuration | Recovery completed | Subsequent subject ship loss |
| --- | ---: | ---: |
| Existing | No | None |
| Search and arrival | 21,849 | 23,824 |
| Placement and forecasting | No | None |
| Full previous candidate | 21,917 | None |

The existing and full controls retain all 12 checked action, sample, event,
initial-decision and state files exactly. Search/arrival alone is sufficient
for this recovery. Placement/forecast alone leaves the same blocked behavior.
These are feature groups: refinement expands both search sites and native
offsets, so individual necessary components are not yet isolated. No saved
preview normal activates, no staging route is checked, and no terrain-flight
forecast is supplied in the successful group comparisons.

The smaller configuration reconstructs at 21,621 through ordinary native
placement guards. The full configuration starts a settling forecast at 21,621
and reconstructs at 21,661. Both settle and board. The later survival difference
does not by itself show why one subsequent combat continuation loses its ship.

## Bounded reconsideration through ordinary recovery

The [policy plan](terrain-recovery-reconsideration-plan.md) and runtime were
frozen at `7134dbe`. The default-off policy checks a timed-out pod approach
for a current alternative: a different, unrejected measured site closer than
the current measurement of the old site, or an alternative after that old site
disappears. Terrain revision changes alone do not grant more attempts.

There are at most 15 review surveys over 15 seconds, at most one per second.
One accepted opportunity grants a fixed 90-second control window. Disarmed
ticks consume that window; toggling the option cannot renew it. Original
recovery history and the four-relocation limit remain intact. Invalid identity
or backwards observations stay terminal. On-foot stalls and exhausted landing
retries are excluded.

The explicit `sensor-profile` driver uses ordinary task creation and stepping,
with one enhanced recovery observation per decision. It selects the tested
search/arrival group; native contact-frame and settling-forecast options are
not enabled by it. The original blocked controller is never replaced.

| Angular milestone | Tick |
| --- | ---: |
| Original task starts | 863 |
| Original flight budget exhausted | 8,064 |
| One current alternative accepted | 8,065 |
| Pod lands / pilot exits | 9,668 / 9,670 |
| Four measured relocations exhausted | 12,255 |
| Fixed extra control deadline | 13,465 |
| Native ship reconstructed | 21,621 |
| First retained sample with settled landing | 21,720 |
| Settled ship boarded; recovery completes | 21,849 |
| Combat resumed | 22,523 |
| Subsequent ship loss after combat return | 23,824 |

The alternative at 8,065 is bearing 56, about 50.5 units from the pod, versus
86.3 units for the active bearing-33 approach. One survey grants the opportunity.
The original three rejected landings remain recorded. The native rebuild later
releases the already-existing one-time boarding exception, with deadline
27,021. Completion has two supported feet, native `landed` status, 0.25 seconds
settled and full ship health.

The long blocked wait after 12,255 remains. This change does not make recovery
finish inside the extra 90 seconds; it restores a bounded active attempt that
eventually puts the pilot where native construction becomes possible.

Selecting the driver at tick zero produces identical controls and physics
through the Angular fork and identical controller memory at the fork. Its
entire 18,000-tick continuation has the same encoded actions and site requests
as the fresh search/arrival arm. Controller telemetry intentionally differs
because it retains history. Some late scenario checksums differ after the next
ship loss, when the ordinary driver also configures the new recovery task.

The later Angular fork does not reopen its expired review window. Both Round
forks keep their original on-foot block. All three produce the same encoded
actions as their unchanged blocked controls. Failed attempts still do not prove
those physical states unrecoverable.

## Checks, evidence and remaining work

Passed on `7134dbe`: **433 AI library tests**, **117 recovery/navigation/gameplay
tests**, strict bot Clippy on Rust 1.89, formatting, locked release build and
default-feature library check. The 117 include 20 physical mission tests and
the combat recovery test. The four-arm comparison and four automatic/control
comparisons pass their terrain conservation and physical-speed audits.

The [component audit](data/terrain-recovery-components-v1.json) verifies 2,352
frozen inputs, 12 retained control files and 87 evidence files. The
[automatic audit](data/terrain-recovery-reconsideration-v1.json) verifies 2,358
frozen inputs, all four retained control streams and 120 evidence files.
Maximum observed refinement work remains eight coarse/eight refined sites and
236 offset checks, below the 240-check cap. No simulation outcomes were rerun
or tuned after freezing. The auditor initially compared two JSON float formats;
it now checks their identical source f32 bits. No runtime or evidence changed.

Raw outputs, commands, executable hashes, manifests and validation logs live
outside Cargo's target under `spacewars-handoffs/recovery-components-20261009`
and `spacewars-handoffs/recovery-automatic-20261009`. The follow-up portable
package `terrain-recovery-reconsideration-20261009.tar.gz` includes both phases
and a Git bundle based on `ea069ce`; restore the
previous integration package first if that prerequisite is missing.

Next, keep Round separate: its blocked control first has balanced, supported
footing in the retained once-per-second sample at 16,500. Reconstruct that
state and test a measured claim route before introducing any on-foot retry.
Broader fresh-match and device-cost qualification, and integration with final
#174 source, remain before changing defaults or declaring #178 ready.
