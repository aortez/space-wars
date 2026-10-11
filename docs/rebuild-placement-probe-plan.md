# Isolate placement inputs in unchanged replay states

Reproduce four retained paths from the radial experiment with diagnostic queries
only. Preserve every one of their 34 archived files byte for byte. The probe
must not change actions, physics, task state, clocks, flags, limits or outcomes.

| Path | Accepted preview | Probe tick | Purpose |
| --- | ---: | ---: | --- |
| `control_handoff` | 26370, bearing 344 | 27113 | Successful control immediately before its build |
| `control_recheck_walk` | 25560, bearing 343 | 25829 | Original direction disagreement |
| `control_recheck_handoff` | 25560, bearing 343 | 26370 | Original direction disagreement with handoff |
| `radial_handoff` | 24300, bearing 324 | 25373 | Coarse arrival and drift failure |

At each preview tick, capture the actual measured node, effective query direction,
and actor position that centered its replacement ground map. Recompute that
preview and require it to reproduce the retained accepted report. Freeze requests
from the preceding portable bundle before replaying. The successful control is
probed one tick before construction, while native eligibility still holds.

At each probe tick, keep one native world fixed and measure twelve combinations:
the preview versus actual contact point; the captured preview direction versus
current contact normal versus the radial direction at the tested point; and a
fresh map centered on the actual contact versus the original preview map extent.
Every map uses current geometry. The two fixed supplied directions isolate point
changes; the radial direction intentionally follows its point. Refreshing both
map extents separates local map bounds from point and direction effects.

Run the ordinary native query at the actual point as a baseline. Forced directions
use a diagnostic clone with radial override disabled. Keep all native placement
guards and offsets. Check physics, contact and recovery state before and after.
These hypothetical placements neither move the actor nor authorize construction.
If the native failure is fresh, record whether its report matches the baseline.
Differences are evidence to investigate, not grounds to hide a run.

Freeze source, requests, commands and binary before four prefixes and eight
continuations. Retain all failures and recorded controls; no simulation retries.
Auditor-only repairs must reuse verified raw output and be disclosed. Validate
native, bot, harness and Python tests, formatting, locked Rust 1.89 builds, bot
Clippy and the existing native Clippy diagnostic baseline.

Use these results to select a focused fix. If the preview point remains valid
but the actual point fails under the same direction and map, test precise foot
arrival for coarse sites separately from further holding changes. If orientation
or current geometry explains the failure instead, address that evidence first.
Keep radial placement disabled outside its retained diagnostic case. Preserve the
successful handoff and require rebuilding, settling and boarding before promotion
or expansion into fresh games. Work stays local.
