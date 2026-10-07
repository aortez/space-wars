# Bounded refinement builds on the tape but still blocks in live recovery

The opt-in refinement is implemented and tested. It keeps the original four
placement offsets first, adds bounded nearby-footing and intermediate-offset
search, and requires precise supported arrival at refined destinations.
The default control preserves all **20 files** from the preceding corrected
replay byte for byte.

**The original autonomous recovery remains unqualified.** The live task still
ends at tick **24548**, with no relocation, rebuild or boarding. Its capped
neighbor searches repeat earlier candidates. Read-only analysis also finds
that the exact route to the known placement-valid footing costs **24.082
units**, just beyond the retained 24-unit route limit.

There is partial native evidence in the recorded-controls continuation: an
intermediate offset produces a real ship at **27819**, and it reaches two-foot
settling at **28227**. That continuation applies the original P2 tape; its
generated relocation and boarding controls are diagnostic. It never boards.

## Frozen experiment and retained behavior

The [plan](rebuild-refinement-plan.md) freezes source `576e34e`, 962 input
hashes, one copied release executable and two commands. Each command runs the
original prefix and forks into recorded-controls and live-task continuations.
Refinement is enabled for P2 only after the handoff at 23767. P1 always follows
the original tape. Both prefixes match all 23,768 tick rows, 47,536 native pilot
observations, 6,948 task/control steps and 396 world audits.

The recovery starts at **16820** and retains the high crossing completed at
**23226**, the original claim, and its 5,400-tick ground allowance. The
four-relocation limit, five-second missing-site allowance, traversal/stall
limits, native build timer and boarding gates are unchanged. There are two
planned prefixes, four continuations, no fresh games, no simulation retries
and no audit repairs. This experiment has no scored-match or default-promotion
claim.

The ordinary survey and accepted original offsets retain precedence. Only
after ordinary placement fails does refinement try up to eight nearby nodes,
within two units of promising coarse footings. Each survey permits at most
eight coarse nodes, eight refined nodes and **240 offset checks**. Native build
attempts permit at most 26 offsets on the existing retry cadence. The new
offsets are half-unit steps strictly between 8 and 14 on each side.

Refined routes target the actual measured node within 0.01 units. Arrival
requires the supported foot within 0.12 units. Synthetic task tests cover
propagation, clone/reset behavior, unsupported arrival rejection and unchanged
deadlines. **The live native continuation never selects a relocation, so it
does not exercise this new precise-arrival behavior.**

## Live search still makes no progress

| Continuation | End tick | Proposed relocations | Native builds | Boardings |
| --- | ---: | ---: | ---: | ---: |
| Default, live task | 24548 | 0 | 0 | 0 |
| Refined, live task | 24548 | 0 | 0 | 0 |
| Default, recorded controls | 29421 | 0 | 0 | 0 |
| Refined, recorded controls | 29421 | 1 | 1 | 0 |

Both live tasks stop with `no reachable standing site with hatch access`.
Across all **782 live rows**, task telemetry, generated controls, applied
controls, landing diagnostics and native pilot observations match after
excluding placement/search diagnostics. The first diagnostic difference is
24247. The portable receipt lists the exact excluded fields; it does not mask
task status, motion, support, health, native counters or selected sites.

The refined live arm performs ten surveys and respects the 240-offset cap.
At 24330, its eight refined bearings are:

```text
330, 341, 329, 340, 331, 342, 332, 339
```

The same set appears at 24420 and 24510. Nearby survey phases also repeat
their own initial neighborhoods. The implementation restarts its distance
queues each survey; it has no persistent cursor through untested neighbors.
The previously identified footing **343 is never sampled** before the task's
original missing-site deadline. Full candidate lists and work counts for all
ten surveys are retained.

## Exact routing adds a second constraint

The [preceding coverage probe](rebuild-coverage-results.md) found placement
previews at footing 343, offsets -11 and -12, on planet zero revision 21.
Its ordinary route stopped at nearby footing 342, within the 0.8-unit target
range. To resolve the endpoint question, a separate native utility queried the
saved measured graph after the frozen experiment. It constructs no world and
performs no physics steps. The entire graph round-trips unchanged, and all
**99 original routes** reproduce exactly before precise routes are compared.

| Route to footing 343 | Endpoint | Length |
| --- | ---: | ---: |
| Original 0.8-unit range | 342 | 23.651978 |
| Precise 0.01-unit range | 343 | 24.082449 |
| Retained admission limit | — | 24.000000 |

Consequently, sampling 343 at this saved state would still reject its exact
route under the frozen policy. None of the prior probe's placement-valid
points also has a precise route within the limit. This conclusion concerns
that measured local patch, snapshot and 26-offset coverage. It does not rule
out a usable pose after movement, at another time, or with other offsets.

The next iteration should investigate search progress retained across surveys
and a short staging move that can bring a precise final approach within the
per-leg route bound. Both need bounded work and the original recovery deadline.
Their usefulness is still a hypothesis; neither is implemented or validated
by this experiment.

## Recorded controls produce a real build and settling

The refined recorded-controls task proposes a relocation at **25560**. Its
generated movement is not applied. The original tape later reaches a different
actual supported position and completes a native rebuild at **27819**:

| Measurement | Result |
| --- | --- |
| Planet / revision | 0 / 22 |
| Actual local standing position | `(23.625763, -18.674660)` |
| Selected offset | -12 |
| Predicted resting angle | 5.5042 degrees |
| Preview hatch route | 3.5808 units; one jump |
| First two-foot, 0.25-second settled tick | 28227 |
| Actual angle at first settling | 5.9703 degrees |
| Continuous settled interval | 28227–28598 inclusive; 372 rows |
| Native boardings | 0 |

The four original offsets all fail at this actual standing position. The
expanded offsets supply the accepted native placement. This establishes a
real build and a period of settling for the later revision-22 pose. Arrival
at the original revision-21 preview remains unverified.

At first settling, the task generates movement toward boarding, while the
applied tape retains zero horizontal input. The diagnostic task eventually
blocks with `spaceling stopped making progress on ground route`. At the fixed
end, 29421, the ship has one supported foot, zero settled time and a 10.2134
degree angle. P2 remains on foot, alive at 91.2675 health, with one ship lost
and one rebuild. The bundle includes every post-build landing sample and
dense witnesses around building, settling and loss of support. Boarding under
the task's actual controls remains untested in this arm.

## Validation and review evidence

Passed: **142 Rust tests** (4 native placement, 41 ground navigation, 34 surface
recovery and 63 replay harness), **954 Python tests**, bot library/tests/example
Clippy with warnings denied, formatting, the locked Rust 1.89 release build and
the default-feature native library check.

Broad native-crate Clippy still **fails with 16 diagnostics**. Their messages
and source-file identities match the preceding coverage baseline; line numbers
can move with added fields. No diagnostic points to the new refinement or
coverage modules. The failed log and baseline comparison are retained. This
remains an outstanding repository lint limitation.

The [manifest](data/rebuild-refinement-v1.json) records the commands, hashes,
work counts, direct control comparisons, route and landing receipts, validation
outcomes and archive inventories. The
[portable bundle](data/rebuild-refinement-v1.json.gz) includes all reports and
audits, the 13 frozen changed sources, validation logs, complete precise-route
output and utility sources, landing timeline, dense witnesses and reproducible
exporter. Its provenance links the prior coverage and original replay bundles.

The complete raw traces remain in losslessly checked local archives:

| Archive under `target/rebuild-refinement/v1/archives/` | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control.tar.gz` | 249,622,777 | 38,719,432 |
| `refined.tar.gz` | 254,524,512 | 38,487,638 |

Work remains local on `bot-rebuild-refinement`. Refinement is opt-in; bot
defaults and deployment are unchanged.
