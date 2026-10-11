# Ordinary native offsets recovery candidate

The from-zero replay confirms the targeted Angular P2 fix: recovery completes at tick 25,282, one sortie follows and the pilot survives the 36,000-tick match. Its full input and physical-state checksum streams and native match result equal the standard control. The previous candidate died at tick 33,563 without recovering. The earlier extra Round recovery and the other three retained recovery counts are preserved.

The Angular P1 case still accumulates 315 more blocked ticks than standard (2,058 versus 1,743), matching the previous candidate. This remains a separate issue. The old Round 20202 ownership tradeoff also remains: two recoveries instead of one, but zero final planets versus one for standard. No default change or deployment is warranted.

The opt-in `terrain_recovery_supported_hatch_ordinary_offsets_v1` profile keeps enhanced recovery sensing, bounded recovery and supported hatch arrival while using ordinary physical rebuild offsets. Existing profile meanings and defaults remain unchanged.

Runtime source: `d9752f7a3d0ed8b75e0e4a0c34656386924e0f67`; binary SHA-256: `0df30c62135d3373401235d275cd3496d92c17a7ac9c208bb3bca5a2e741562b`. The experiment used the unchanged `material_mission_v13` mission policy. No controller reset, input override, forced damage or physics edit was used.

## Retained comparisons

All five historical candidate replays reproduced the frozen action, site, decision and scenario checksum streams, initial/final observations and match outcomes. The only permitted initial-controller text difference was the new configuration field, checked explicitly for both seats. Each new candidate started from tick zero in the identical physical world.

For the table, values are standard control / previous candidate / ordinary-offset candidate. The long seed is `13656578344987333250`.

| Case and selected seat | Recoveries | Blocked ticks | New completion ticks | Flags versus previous candidate |
| --- | --- | --- | --- | --- |
| 10101-angular-collapse-0-p1 | 1 / 1 / 1 | 0 / 0 / 0 | 11337 | none |
| 20202-round-collapse-0-p2 | 1 / 2 / 2 | 14819 / 5047 / 5047 | 7519, 24911 | none |
| 13656578344987333250-round-collapse-1-asteroids-8-p1 | 2 / 2 / 2 | 104 / 0 / 0 | 10542, 24336 | none |
| 13656578344987333250-angular-collapse-1-asteroids-0-p1 | 1 / 1 / 1 | 1743 / 2058 / 2058 | 16297 | none |
| 13656578344987333250-angular-collapse-1-asteroids-0-p2 | 1 / 0 / 1 | 17796 / 25287 / 17796 | 25282 | more_longest_block |

The Angular P2 longest-block flag is retained: its longest continuous block is 17,796 ticks versus 16,228 previously, while total blocked time falls from 25,287 to 17,796 and recovery and survival improve. The new values equal the standard control; this metric alone is not a loss of recoverability.

Detailed paired inputs, outcomes, ownership, deaths and metrics are in [the machine-readable receipt](terrain-recovery-ordinary-offsets-results-v1.json). Comparison flags are reported without treating a single match as proof of playing strength.

## Validation and limits

- 456 AI tests passed; 21 explicit diagnostics remained ignored by the default test run. Strict AI Clippy (`--no-deps`), default-feature compilation, formatting and diff checks passed.
- The first dependency-inclusive Clippy run stopped at the existing `collapsible_else_if` warning in `crates/engine-rapier/src/spaceling.rs:395`. That source was unchanged. Both lint commands and outputs are retained.
- 10 native matches, 315063 decision ticks and 5252 conservation samples were audited. New-profile physical refinement was false on every step; enhanced sensing remained enabled. All 21 native completion receipts showed later mission activity; per-tick planning/survey bounds were checked (maximum four graphs and 142 physics queries).
- 2468 source inputs were verified against the runtime commit, together with the binary digest. 443 retained case/reference artifacts have checksums.
- The original comparison reader stopped because older preflight summaries omit `native_recovery`. Reader v2 obtains those historical counters from the retained final native pilot observations. Completed simulations were reused without rerunning; both reader versions, freezes and the original exception are retained.
- The 24 planned fresh-seed matches (seed `3659225967059093636`) were not launched within the 15-minute cap. This is a retained-case compatibility check, not fresh-seed qualification or a default-promotion result.

Evidence: `/home/data/workspace/spacewars-handoffs/recovery-ordinary-offsets-20261011`. The verified package is `terrain-recovery-ordinary-offsets-20261011.tar.gz` beside that directory; its external checksum receipt records the package digest. Runtime commands are in each case `command.json`; use a fresh output directory to reproduce. The source bundle is based on main `f717a7f5c528c514de30dec08fc1b2a991c4b464`.

Time box: 2026-10-11 07:14:47–07:29:47 UTC, with no new simulation launched after 07:27:30 UTC.

Next bounded step: inspect the retained Angular P1 delay and determine whether it is required supported-hatch waiting or avoidable controller delay, using retained traces first. Then run the declared fresh-seed comparison before considering broader qualification.
