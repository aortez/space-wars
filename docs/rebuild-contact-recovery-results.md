# Contact-normal search reaches useful sites but native construction still fails

The existing refinement search discovers bearing **343**, reaches it precisely,
and later reaches a second accepted site at **388**. It needs no additional
sampling or larger search budget. However, native placement rejects both sites
after the construction timer completes. A fourth relocation is invalidated by
changed terrain. The candidate stops at **28809**, alive at health **100**, with
the original one ship loss, **zero builds and zero boardings**. Full recovery
remains **not qualified**.

This executes the [frozen post-veto plan](rebuild-contact-recovery-plan.md).
Only the replay harness changes the coarse live path's direction flag, once,
after its original negative forecast and exhausted offset search. Both retained
controls and every recorded-action continuation reproduce their prior outcomes.

| Live path | End tick | Result | Builds / boardings | Relocations | Health |
| --- | ---: | --- | ---: | ---: | ---: |
| Handoff control | 27261 | Complete | 1 / 1 | 3 | 60.666924 |
| Radial coarse control | 25739 | No reachable standing site | 0 / 0 | 1 | 100 |
| Coarse, contact-normal after veto | 28809 | Four relocations exhausted | 0 / 0 | 4 | 100 |

## The earlier failure and travel path are preserved

The original coarse forecast begins at **25373**, rejects offset -8 at **25413**,
and exhausts its remaining 25 offsets at **25438**. The harness switches to
contact-normal queries at **25439**, using those completed native events as its
trigger. No pending forecast is interrupted. Native observation, contact,
task telemetry and search request remain unchanged at the switch itself.

All **1,672 live rows and contact samples** from 23767 through 25438 match the
radial control. Selector events through exhaustion match outside explicit
wall-clock timing fields. The first differences then occur at:

| Evidence | First differing tick |
| --- | ---: |
| Full row, containing the new placement queries | 25440 |
| Pilot observation excluding the placement report | 25468 |
| Task telemetry | 25470 |
| Applied actions | 25666 |

The pilot-observation difference includes native recovery status; it does not
mean physical motion changed before the applied actions. Opponent actions
remain the original recorded stream throughout.

## Discovery succeeds within the existing search

The original radial relocation to bearing 324 at 24300 is unchanged. After the
switch, the ordinary sparse/refined search selects these further destinations:

| Relocation | Selection / bearing | Precise arrival | Later result |
| --- | --- | --- | --- |
| 2 | 25650 / 343, offset -11 | 25922, foot error 0.119738 | Native placement rejected at 26333 |
| 3 | 26340 / 388, offset -11 | 27415, foot error 0.119205 | Native placement rejected at 27856 |
| 4 | 27870 / 343, offset -11 | No arrival | Site invalidated by changed terrain; final native rejection at 28809 |

Bearing 343 is one of the three footings exposed by the preceding
[same-scene diagnostic](rebuild-query-directions-results.md). Its selection
uses eight coarse candidates, two refined candidates and **84 offset checks**.
The measured outbound route is **7.223 units**, with two jumps and no jetpack
flight. Bearing 388's later route totals **23.955 units**, including two jetpack
flights. Both arrivals satisfy the existing native 0.12-unit foot-distance test.

Across the entire candidate continuation, 12 surveys never exceed **240 offset
checks** or **two staging-route checks** in one survey. No staging move, footing
hold or explicit footing recheck activates. The original task start, ground
budget, search history and four-relocation limit remain in force.

## Failure moves from discovery to native placement

Both arrived sites retain terrain revision 22 through their first later native
placement rejection. Yet the actual support point has moved relative to the
accepted preview:

| Bearing | Native check | Ticks after arrival | Native point distance from preview | All 26 offset results |
| --- | ---: | ---: | ---: | --- |
| 343 | 26333 | 411 | 0.668367 | 18 no-ground, 8 no-hatch-route |
| 388 | 27856 | 441 | 0.299125 | 16 no-ground, 6 no-hatch-footing, 4 no-hatch-route |

The full native construction interval remains unchanged. Some progress accrues
while approaching: the timer is already about 14.4% and 8.1% complete at these
arrivals. The elapsed ticks above are therefore the remaining wait after
arrival, not a shortened construction interval.

At bearing 343, the actual native contact normal differs by **42.03°** from the
measured normal in the retained revision-22 map. At 388 those normals agree to
about **0.000005°**, but point displacement and hatch access still differ.
These normal references come from the earlier paired diagnostics at 25710 and
27114; they are not new same-scene ablations at the rejection ticks. Point,
normal and epoch differences must not be collapsed into a single proven cause.
Earlier [frozen placement probes](rebuild-placement-probe-results.md) separately
demonstrated that a preview/contact normal disagreement can reject a valid pose.

The fourth selected site belongs to revision 22. By **28330**, revision 25
invalidates it and clears the relocation; the pilot has not reached that site.
Native placement later runs at its current location and rejects 21 offsets for
ground and five for hatch footing. The task then enforces its four-relocation
limit at 28809. This is a separate terrain invalidation, not another verified
arrival followed by drift.

There are **no new forecast jobs after the switch**: none of the actual native
placement attempts passes geometry. The only candidate forecast is the retained
negative radial job, with 40 warmup and 120 ship steps, at most four per update.
Thus the forecast selector is not vetoing the new preview sites; those sites
fail before reaching its forecast stage. No replacement is constructed or
scuttled.

## Validation and next step

Runtime source **f9a950b**, **1,029 input hashes**, four changed inputs, commands,
trigger and bounds were frozen before execution. Only replay-harness runtime
code changed. Three prefixes and six continuations complete with **zero
simulation retries and zero audit resumes**.

Both controls retain every prior gameplay raw file outside known selector
timings. Their archives preserve **44 handoff / 48 coarse** prior files other
than the two timed logs, including historical diagnostic reports and audits.
Those restored audits retain their old timing provenance. Candidate prefix and
recorded gameplay files also match; its live search, route, contact, placement,
direction and forecast audits are newly evaluated. All recorded forks still
end at 29421 with their prior outcomes.

Passed: **66 Rust harness tests**, **1,026 Python tests**, bot Clippy, formatting,
locked Rust 1.89 release build and default-feature library check. Native runtime,
engine and controller implementations are unchanged; their test suites and
native Clippy were not rerun or counted as fresh validation.

The next focused change is a measured-preview-normal handoff to native
placement: retain the selected surface direction in planet coordinates, require
the same terrain revision, and query at the actual supported standing point
with fresh geometry, occupancy, hatch access and forecast validation. Preserve
the successful control and existing limits. The drift and later terrain
invalidation remain separate constraints to verify. This result supplies no
reason to expand sampling or relax a placement guard.

The [manifest](data/rebuild-contact-recovery-v1.json) and
[portable evidence bundle](data/rebuild-contact-recovery-v1.json.gz) include the
switch boundary, paired differences, executed search and arrival witnesses,
native rejections, reference-normal comparisons, validation, frozen source,
exporter and archive receipts. Full archives remain under
`target/rebuild-contact-recovery/v1/archives/`. The experiment remains local on
`bot-rebuild-contact-recovery`; it establishes no fresh-game, reactive-opponent
or frontier qualification.
