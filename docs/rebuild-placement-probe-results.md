# Frozen placement probes separate direction and position failures

The probes identify two distinct causes. Both original bearing-343 failures
accept placement at the actual pilot position when supplied the preview's surface
direction. Moving to the preview point without changing the contact direction
does not help. The coarse bearing-324 failure instead accepts every tested
direction at its preview point and rejects every direction at its actual point.

| Retained case | Point tested | Contact direction | Captured preview direction | Radial direction |
| --- | --- | --- | --- | --- |
| Successful handoff | Actual | -10 | -10 | Rejected |
| Successful handoff | Preview | -10.5 | -10.5 | Rejected |
| Original recheck, walking | Actual or preview | Rejected | -11 | Rejected |
| Original recheck, handoff | Actual or preview | Rejected | -11 | Rejected |
| Coarse radial failure | Actual | Rejected | Rejected | Rejected |
| Coarse radial failure | Preview | -8 | -8 | -8 |

Numbers are accepted placement offsets. Every row has the same result with the
current native map extent and with a fresh map over the original preview extent.
Map extent therefore does not explain these cases. These are read-only placement
results; no hypothetical actor movement or successful construction is claimed.

The original walking and handoff points differ from their previews by only
**0.08662** and **0.07714** units. Their supplied directions determine acceptance
at either point. Both accepted previews reproduce exactly when captured at 25560;
the same-world probes run at 25829 and 26370. This isolates the direction problem
that the earlier global radial experiment could not isolate.

The coarse point differs by **1.08876 units** at 25373. Its preview at 24300 still
passes on the current world when queried at the original point. This supports
testing precise arrival for coarse sites separately from holding. The successful
handoff's probe at 27113 also shows why the radial change must remain disabled:
it rejects valid placements even at the exact positions where the existing
directions succeed.

All three fresh native failure reports match the probes' ordinary native
baselines. The success probe is deliberately one tick before its actual build,
while native eligibility still holds. Every accepted hypothetical pose retains
the native landing, hull and hatch checks.

The [plan](rebuild-placement-probe-plan.md) froze source **f9772f5**, **990 input
hashes**, seven changed inputs, four requests, four commands and one release
binary. All four replays retain their preceding **34 files byte for byte**.
Four prefixes and eight continuations completed with no simulation retries or
audit resumes. Each of the four fixed states receives twelve comparisons.
Physics, native contacts and recovery observations are unchanged by probing.

Passed: **181 Rust tests** and **977 Python tests**, bot Clippy with warnings
denied, formatting, locked Rust 1.89 release build and default-feature check.
Native Clippy retains its 16 existing message/file diagnostic identities.

The [manifest](data/rebuild-placement-probe-v1.json) and
[portable bundle](data/rebuild-placement-probe-v1.json.gz) contain the matrices,
captured previews, native reports, selection and probe witnesses, frozen source,
validation and exporter. Verified raw archives remain in
`target/rebuild-placement-probe/v1/archives/`. The unchanged full paths and their
earlier evidence remain linked through the preceding bundle's hash.

No bot behavior or defaults changed in this diagnostic step. The next controlled
change is precise foot arrival for coarse sites. The independently demonstrated
surface-direction mismatch still needs a separate correction; arrival accuracy
alone cannot solve the original bearing-343 failures.
