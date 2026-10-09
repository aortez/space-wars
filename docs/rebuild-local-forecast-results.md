# Two-body forecasts preserve settling from all three unbuilt candidates

The reduced model distinguishes the failed coarse placement from both successful
handoffs, including the corner landing rejected by the earlier geometric checks.
Both positive cases earn native settling on the original ticks. Setup plus
physics costs **1.45–2.33 ms per forecast**, versus **62.77–66.05 ms** for the
archived full-world references. The largest four-step chunk, including diagnostic
sampling, costs **0.141 ms** in these workstation measurements.

This is still a diagnostic. It captures the selected candidate before the real
replacement exists, then advances independently over 30 live frames. Construction
continues immediately under the existing rules; the forecast does not yet select
another offset, delay a build or authorize it.

## What the smaller model retains

The [frozen plan](rebuild-local-forecast-plan.md) creates a hypothetical ship with
the shared native spawn helper, actual hull and round feet, and the origin/COM
velocity correction. Each initial ship and planet motion matches the real build
exactly. Every capture records that the live replacement was unavailable.

Each forecast contains **two physical bodies and 11 colliders**, compared with
46–51 bodies and 205–210 colliders in the corresponding live worlds. It shares
immutable geometry from the selected planet and copies physical ephemeris/gravity
inputs for three planets plus the sun. It does not serialize or clone the whole
simulation. The engine rejects oversized or unsupported geometry instead of
truncating it; limits are 128 planet colliders, 16,384 shape parts/vertices and
32 planetary gravity sources.

The model uses the existing gravity solver, prescribed planetary motion, landing
assist, contact response and two-foot settling gate. It copies no other actors,
debris, claims, construction timers, RNG, damage, hazards, terrain evolution or
contact/solver history. The pilot's landing frame stays on the selected planet;
exceeding the local travel bound or approaching another planet invalidates the
result. Scope failure is inconclusive even after an earlier predicted landing.

The entire selected planet's geometry remains privileged input. This reduction
does not establish what a normal bot is allowed to observe, nor predict what
happens if an asteroid changes the site or another actor interferes.

## Forecast versus the reference and actual motion

The archived full-world forecasts already match the retained actual windows.
The new audit compares against both: 363 full-world rows and 362 actual rows.

| Candidate | First contact, reduced / reference | Two feet, reduced / reference | Settled, reduced / reference | Maximum position difference |
| --- | --- | --- | --- | ---: |
| Live handoff, 27114 | 27173 / 27173 | 27174 / 27174 | 27188 / 27188 | 0.002612 |
| Recorded handoff, 27459 | 27510 / 27510 | 27543 / 27543 | 27557 / 27557 | 0 |
| Live coarse, 25373 | 25422 / 25421 | Neither | Neither | 0.009124 |

Position differences are in world units. The recorded handoff matches every
projected motion, landing, planet and contact field at native precision across
all 121 samples. The live handoff first differs at 27180, after touchdown, but
still settles at 27188. The coarse case first differs in motion at 25375 and
contacts one tick later; it never earns two supported feet or settling within
the 120-step horizon. All differences are retained, rather than treating these
classifications as an exact reproduction of the full simulation.

All three jobs finish the full horizon without a scope failure. A negative only
means not settled within this two-second conditional model. These are known
positive and negative cases, not held-out prediction accuracy or proof that an
alternative-placement search will recover the coarse failure.

## Work and elapsed-time measurements

`advance` performs at most four physics steps regardless of the requested budget.
Zero-budget calls and completed jobs do no work. The harness advances each job
once per live tick, producing 30 chunks and completion at ticks 27143, 27488 and
25402. The same model produces identical samples with one-step scheduling.

| Candidate | Setup | 120 physics steps | Setup + physics | Maximum four-step chunk |
| --- | ---: | ---: | ---: | ---: |
| Live handoff | 0.041 ms | 1.412 ms | 1.453 ms | 0.096 ms |
| Recorded handoff | 0.031 ms | 1.614 ms | 1.645 ms | 0.102 ms |
| Live coarse | 0.041 ms | 2.286 ms | 2.326 ms | 0.141 ms |

Sample generation totals another 0.604–0.631 ms per job. The maximum chunk
includes its sample generation; setup and final JSON export are separate. The
setup-plus-physics samples are approximately 28–43 times smaller than the prior
clone-plus-native-step samples. These are individual measurements from separate
runs on this workstation, not repeated benchmarks, Raspberry Pi measurements or
a guaranteed wall-time deadline. Four steps is a deterministic work bound.

## Preservation and validation

Every previous raw recovery file, excluding the old timed forecast diagnostics,
is byte-identical. The synchronous full-world forecast flag was disabled in the
new run. Its three files and audits were restored from verified archives as
references, not rerun or presented as new cost measurements. All **44 handoff /
42 coarse** preceding archive files are preserved exactly before reusing their
audits. New verified archives contain 47 and 44 files respectively.

The live handoff still completes recovery at 27233 with one ship loss. The live
coarse route still blocks at 27349 after timing out and scuttling its replacement,
with two losses. The recorded handoff settles but does not board before the fixed
end, and the recorded coarse route builds nothing. No bot default, frontier
choice, placement gate or gameplay outcome changes.

Runtime source **8d5ee80**, **1,015 input hashes**, 12 changed inputs and a copied
release executable were frozen before running two prefixes and four
continuations. Three candidates produced **360 projected physics steps**.
There were **zero simulation retries and zero audit resumes**.

Passed: **316 Rust tests** (126 engine, 36 native rebuild, 41 ground navigation,
50 recovery, 63 harness), **997 Python tests**, bot Clippy, formatting, locked
Rust 1.89 release build and default-feature check. Native Clippy retains its
16 prior diagnostic identities. An additional engine Clippy check retains one
existing `collapsible_else_if` diagnostic in `spaceling.rs`, confirmed by running
the same check on the previous commit, 9459cfb. Both baseline logs are retained.

Meaningful fixture tests verify capture before construction, unchanged source
physics, independent native settling, one/four-step equivalence, clamped quotas,
inconclusive scope failures and rejection of excess sources or unready geometry.
Engine tests cover query readiness without a physical step, copied-body
independence, compound limits and unsupported geometry. Auditor tests reject
post-build inputs, excess work and changed initial poses, and retain deliberate
motion/classification disagreements.

The [manifest](data/rebuild-local-forecast-v1.json) and
[portable bundle](data/rebuild-local-forecast-v1.json.gz) contain the frozen
sources, reference provenance, predictions, comparisons, costs, validation and
exporter. Full verified archives remain under
`target/rebuild-local-forecast/v1/archives/`.

The next step is to compare alternative offsets early enough for a forecast to
finish before construction, and revalidate the candidate against current terrain,
planet motion and pilot position before using its result. Preserve both known
landings and require the coarse case to find a replacement that actually settles
and can be boarded. Current-site holding and observation limits remain part of
that work; this experiment alone does not qualify a production gate.
