# Coarse arrival explains the retained delay; fresh ordinary-offset screen

The existing ordinary-offset candidate passes the complete predeclared fresh screen: 14 of 16 comparisons preserve every input and native match result, one gains a completed recovery and pilot survival, and one shifts recovery by seven ticks while preserving the selected bot's outcomes. There are no regression flags in the tracked metrics. Keep this profile and precise coarse arrival; this investigation does not justify another policy change.

The Angular P1 delay is caused by requiring the measured rebuild footing. Disabling only coarse-site arrival precision reproduces the standard controller's entire remaining input and physical checksum streams and its native match result. It removes 315 blocked ticks, but does not add a completed recovery. This supports keeping precise arrival: its measured point is reached correctly, and earlier [coarse-placement evidence](rebuild-precise-arrival-results.md) showed that ordinary stopping distance can prevent construction.

This follow-up changes diagnostic code only. The selected profile remains `terrain_recovery_supported_hatch_ordinary_offsets_v1`, with ordinary native offsets, enhanced sensing and supported hatch arrival. Default behavior is unchanged.

## Same-world diagnosis

The two continuations clone the candidate immediately before tick 32,115, including physics, mission memory and the reactive opponent. Only the existing recovery task's `rebuild_precise_arrival` flag differs. A restoration check compares full controller debug state. No task reset, input override, physical edit or deadline extension occurs. Refined-site precision remains independent.

| Selected P1 measure | Precise coarse arrival | Ordinary coarse arrival |
| --- | ---: | ---: |
| First arrival tick | 32,241 | 32,163 |
| Foot distance from measured target | 0.118675 | 1.296230 |
| Block starts | 33,566 | 33,566 |
| Native rebuild observed | 35,624 | 35,309 |
| Blocked ticks | 2,058 | 1,743 |
| Relocations | 2 | 3 |
| Additional completed recoveries after the fork | 0 | 0 |
| Final recovery / ground goal | Board / wait for ship | Board / wait for ship |

Both complete the same earlier recovery at 16,297, survive to the 36,000-tick limit and own two planets. The first changed input is 32,159; the first changed physical checksum is 32,160. Walking, later combat and terrain changes then diverge. The candidate reaches its target within the existing 0.12-unit tolerance. Native support is later lost and construction restarts. The 315-tick difference is the later native rebuild releasing the already-blocked task, not a 315-tick hatch wait.

All 32,115 prefix decisions and 3,885 unchanged-continuation decisions reproduce the candidate reference. Its final observation bytes also match. The ablated continuation matches every standard input and native physical checksum; its native result equals standard exactly. Both continuations pass conservation and work-limit audits (130 sampled states total). This isolates the setting responsible for this case; it does not prove that ordinary arrival is a better general recovery policy.

## Fresh seed screen

One predeclared seed, `3659225967059093636`, supplies both Weak dirt shapes, collapse off/on, quiet/Mixed8, with a standard control and the candidate independently in P1 and P2: 24 matches and 16 comparisons across eight settings. Both seats use `material_mission_v13`; only the selected recovery profile differs. Candidate selection starts at tick zero. These are repeated settings of one seed, not 24 independent worlds.

| Setting | Seat | Recoveries, control → candidate | Blocked ticks, control → candidate | Pilot death tick, control → candidate | Flags |
| --- | --- | --- | --- | --- | --- |
| round-collapse-0-asteroids-0 | P1 | 0 → 1 | 11039 → 6535 | 34361 → None | none |
| round-collapse-0-asteroids-0 | P2 | 0 → 0 | 0 → 0 | None → None | none |
| round-collapse-0-asteroids-8 | P1 | 1 → 1 | 16708 → 16708 | 35561 → 35561 | none |
| round-collapse-0-asteroids-8 | P2 | 0 → 0 | 0 → 0 | None → None | none |
| round-collapse-1-asteroids-0 | P1 | 0 → 0 | 7044 → 7044 | None → None | none |
| round-collapse-1-asteroids-0 | P2 | 0 → 0 | 0 → 0 | None → None | none |
| round-collapse-1-asteroids-8 | P1 | 2 → 2 | 2428 → 2428 | None → None | none |
| round-collapse-1-asteroids-8 | P2 | 0 → 0 | 0 → 0 | None → None | none |
| angular-collapse-0-asteroids-0 | P1 | 0 → 0 | 0 → 0 | None → None | none |
| angular-collapse-0-asteroids-0 | P2 | 1 → 1 | 0 → 0 | 21109 → 21109 | none |
| angular-collapse-0-asteroids-8 | P1 | 1 → 1 | 0 → 0 | 16230 → 16230 | none |
| angular-collapse-0-asteroids-8 | P2 | 0 → 0 | 0 → 0 | None → None | none |
| angular-collapse-1-asteroids-0 | P1 | 0 → 0 | 0 → 0 | None → None | none |
| angular-collapse-1-asteroids-0 | P2 | 1 → 1 | 0 → 0 | 17987 → 17987 | none |
| angular-collapse-1-asteroids-8 | P1 | 0 → 0 | 0 → 0 | None → None | none |
| angular-collapse-1-asteroids-8 | P2 | 1 → 1 | 0 → 0 | None → None | none |

All 24 matches finish. 14 of 16 comparisons preserve every game input; 1 comparisons add a completed recovery and 0 have automatic regression flags. The compact receipt retains every comparison, including ownership, sorties and outcomes. Flags are descriptive; coupled match outcomes need inspection before attributing a specific policy defect.

### The two changed fresh comparisons

- Round, collapse off, quiet, candidate P1: the control dies at 34,361 with no rebuild or recovery. The candidate completes native recovery at 32,792 with two supported ship feet and 0.25 seconds settled, and launches at 32,793. It survives to 36,000, owns one planet instead of zero and has 6,535 blocked ticks instead of 11,039. It still loses the match. This is one new-seed recovery opportunity, not a demonstrated general strength advantage.
- Angular, collapse on, Mixed8, candidate P2: both select the same coarse rebuild point at 17,040. At the first changed input, 17,109, standard horizontal input is zero and the precise candidate uses 0.104968. Arrival occurs at 17,125 versus 17,124, native rebuild is observed at 17,608 versus 17,607, and recovery completes at 17,745 versus 17,752. Both remain unblocked, complete one recovery and two sorties, survive, own two planets and win. Their subsequent combat differs: the opponent ends with health 100 versus 12.08994. The seven-tick boarding change does not increase recovery count or change the winner.

The changed-case inspection and exact unchanged-pair checks are retained in
`fresh-inspection.json`. No extra simulations were needed for this review.

## Evidence and validation

- 456 AI tests pass, with 22 explicit diagnostics ignored in the default run. The targeted two-arm diagnostic separately passes. Strict AI Clippy (`--no-deps`), default-feature compilation, formatting and diff checks pass.
- The fresh audit checks 701383 decision ticks, 11694 conservation samples and 662 case artifacts. Native physical refinement remains false on every step. Completion receipts, matched initial worlds, exact profile differences, bounded surveys and shared planning budgets are checked.
- 22 native completions are retained; 22 show later mission activity before their match ends. Maximum shared work is 4 graph units and 138 physics queries per tick, within 4/384.
- The runtime source is `155cd5c4996f561f514e7a26cf129d5378db68b6`; binary SHA-256 `40631211f11ceb3fe01ddba90a2dca39cb36f306cacb25d5071243700a5541a3`. All 2472 source inputs are verified against that commit. Source, script, command, reference and artifact hashes are retained.
- An initial diagnostic compilation failed on a JSON macro expression; it was corrected before any simulation. The failed build inputs and logs are retained. The diagnostic and fresh simulations have no outcome-based reruns.

The [machine-readable receipt](terrain-recovery-p1-followup-results-v1.json) links the full evidence. Raw files live at `/home/data/workspace/spacewars-handoffs/recovery-p1-followup-20261011`; the sibling `terrain-recovery-p1-followup-20261011.tar.gz` package has an external SHA-256 verification receipt. The incremental source bundle requires main `f717a7f5c528c514de30dec08fc1b2a991c4b464`.

The prior time cap was explicitly removed by the user. This run completes the declared matrix; the one-seed screen is limited qualification evidence and does not establish frontier strength or warrant a default change.

The retained-delay question and the declared fresh screen are complete. The next step is review of the existing opt-in ordinary-offset profile and its integration path. Keep the named candidate stable during that review; broader evaluation is still needed before changing the default bot.

## Live candidate integration

The launcher now offers **recovery v1** for either or both players. Its HUD and
scoreboard identity is **Recovery bot v1**. This is the qualified Value v13
policy plus `terrain_recovery_supported_hatch_ordinary_offsets_v1`, selected
before the first decision. Existing controller settings and automatic-fill
defaults retain their meanings.

The client calls the same paired observation/decision driver as the retained
qualification. Enhanced sensing is enabled only for an active recovery task;
ordinary physical rebuild offsets are restored before the simulation steps.
The usual shared planning budgets and flag surveys still run after decisions.
Sensor and policy timing remain separate. Diagnostics expose
`mission_recovery_profile_p1` and `mission_recovery_profile_p2` alongside the
base mission policy identity.

A `terrain-recovery` build feature exposes this production path without the
`sensor-profile` instrumentation or query diagnostics. `sensor-profile`
includes the new feature, preserving the historical experiment entry points.

Local verification on Rust 1.89:

- 440 AI unit tests pass with only `terrain-recovery` enabled.
- 456 AI unit tests, 45 ground-navigation tests and 51 surface-recovery tests
  pass with `sensor-profile` enabled. Strict AI Clippy passes with either feature.
- 687 client unit tests pass, including controller ownership, pause/reset,
  persisted settings and all 36 human/bot scoreboard pairings.
- The production client reproduces the fresh Round/collapse-off/quiet/P1 and
  Angular/collapse-on/Mixed8/P2 cases: all 72,000 retained world checksums,
  final world snapshots, mission telemetry and native match results match.
  Each candidate completes recovery; physical refinement is false after every
  decision. Recorded controls are never applied to the live host.
- The first replay checks reached the correct final worlds but compared an
  in-memory JSON float against a parsed artifact. Applying the same JSON
  serialization/readback to both sides fixes that test-only mismatch; both
  full replays then pass without controller or physics changes.
- The launcher functional test now covers the candidate in either and both
  seats, verifies the exact profile through runtime diagnostics, and repeats
  that check after restart. Its execution belongs to the display-backed CI gate.

Reproduce either full host comparison using the corresponding `output`
directory from the frozen fresh-screen package:

```sh
RUST_MIN_STACK=16777216 SPACEWARS_LIVE_RECOVERY_REFERENCE=/path/to/case/output \
  cargo +1.89.0 test --locked --profile ci -p engine-client --bin engine-client \
  live_recovery_matches_retained_qualification -- --ignored --nocapture
```

Local logs are under `target/live-recovery-candidate/`. The local Pi GPU check
requires the missing host `libseat` development package; CI and the Yocto target
build provide the kiosk dependencies. Live integration is a candidate rollout,
not evidence for changing the default bot or a general playing-strength claim.
