# Named recovery candidate reproduces the qualified matches

**`terrain_recovery_supported_hatch_v1` now selects the tested recovery
combination through one public mission profile.** Two full matches reproduce
the preceding explicit-switch candidates exactly: all 52,611 action records,
decision/scenario checksums, initial controller states and final snapshot bytes
match. The Round pilot still completes recovery at 24911; the successful Angular
recovery and the later Round opponent rebuild stall are also preserved.

This is profile integration for #178, not a new controller policy or a strength
comparison. The [sixteen-pair hatch qualification](terrain-recovery-hatch-arrival-results.md)
remains the behavioral evidence. The profile is available to experimental hosts
behind `sensor-profile`; launcher and default bot selections are unchanged.

## Host selection

| Profile ID | Rust variant | Selected recovery behavior |
| --- | --- | --- |
| `standard` | `Standard` | Ordinary mission recovery, without experimental recovery options. |
| `terrain_recovery_v1` | `TerrainV1` | The earlier bounded pod reconsideration, rebuild search/refinement, continuous staging, precise arrival and terrain-flight sensing. Original hatch arrival is retained. |
| `terrain_recovery_supported_hatch_v1` | `SupportedHatchV1` | TerrainV1 plus continued measured-route steering while near the hatch without support. |

Select the complete profile before the first decision, then use the existing
paired observation/intent driver:

```rust
use spacewars_ai::mission_policy::{MissionBot, MissionPolicy, MissionRecoveryProfile};

let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context, breaks)
    .with_recovery_profile(MissionRecoveryProfile::SupportedHatchV1);

let (observation, intent) = bot.decide_with_terrain_recovery(
    &mut state,
    cadence,
    &evaluation,
);
```

The host continues its existing post-intent destination/flag planning and then
applies the intent. The paired driver supplies the profile's recovery sensors
and synchronizes native refinement before the physical step, including cleanup
when recovery finishes. Selecting the profile and calling an unrelated host
observation path does not establish equivalent behavior.

The named selection preserves clone/reset and later ship-loss task creation.
It rejects configuration changes after the first decision. Choosing `Standard`
before starting removes both recovery extensions. The historical
`with_terrain_recovery(bool)` and independent hatch setter remain available for
the existing diagnostic comparisons; their meanings have not changed.

Saved-preview normal, native settling-forecast selection, standing-ground
rejoin, continued corridor sensing and target-directed rejoin remain outside
this profile. The broader experimental frontier configuration is also separate.

## Full-match integration checks

Both cases start from tick zero with the named profile on the selected seat and
the ordinary Value bot on the other. Physics, opponents, host queues, cadence,
asteroid pressure and shared work limits follow the
[frozen plan](terrain-recovery-profile-plan.md).

| Seed / shape / collapse | Selected seat | Finish tick | Completed recovery ticks P1 / P2 | Blocked recovery ticks P1 / P2 |
| --- | --- | ---: | --- | --- |
| 20202 / Round / off | P2 | 36000 | 12301 / 7519, 24911 | 4653 / 5047 |
| 10101 / Angular / off | P1 | 16611 | 11337 / none | 0 / 0 |

Every encoded input and complete action-record field matches the prior
explicit-switch run. Both initial and final snapshots are byte-identical;
controller debug state matches before the first decision. Native recovery
completion receipts show the pilot aboard an available full ship, with the
task and native refinement cleared. Both matches retain their P1 winner.

The Round result includes P2's earlier blocked period and P1's later
rebuild-route stall; naming the profile resolves neither. These are the same
two retained worlds, not two new independent recovery discoveries. Collapse-on
coverage remains the preceding sixteen-pair comparison.

## Verifier correction and validation

The first two diagnostic runs stop before any game decision. The verifier
compares an in-memory `serde_json::Value` with a parsed saved snapshot. Default
JSON parsing changes four Round and six Angular floating-point values in their
last binary digit. The raw saved reference numbers equal the in-memory numbers,
and every native `f32` representation agrees. Complete initial controller
comparison already passes in both failed runs.

The correction compares exact saved snapshot bytes at both endpoints. It adds
no tolerance and changes no reference, profile, controller rule or physical
state. The original source, binary, failed runs and difference analysis remain
in `recovery-profile-20261010`; the corrected runs are in
`recovery-profile-20261010-v2`. The independent auditor checks that the only
Rust change between those runtimes is the diagnostic comparison file.

**454 AI tests pass**, with 17 ignored diagnostics. Strict AI Clippy,
default-feature compilation and formatting pass. The 120 physical gameplay
tests from `9c8285d` are reused, not reported as newly executed: this change
selects already tested behavior and adds no physical/controller rule.

The independent audit passes 2,437 source inputs, ten reference files, 66 output
artifacts, 52,611 decisions and 877 material samples. Initial/final byte equality,
native completion, refinement cleanup, material conservation and speed bounds
pass. The corrected match runs require no further change or rerun.

Profile implementation: `3c8fb050e2cea0b41166f5f8a058044af35acd5a`.
Corrected diagnostic runtime: `6b943b44244eb29548ae86afa72eb93486248d2e`.
Frozen binary SHA-256:
`9d61538aadbc6f3fea4e11c8dfb766a207cc4ff9732ab6de4ddd2f33ece31b25`.

The [compact receipt](data/terrain-recovery-profile-v1.json) includes both cases,
validation, the retained failures and terrain-source provenance. Audit without
simulating:

```sh
python3 tools/audit-terrain-recovery-profile.py /path/to/recovery-profile-20261010-v2 --output /tmp/recovery-profile-audit.json
```

Keep the original and v2 raw directories as siblings. Each successful case's
`command.json` records the exact frozen test invocation and environment; use a
new output directory to reproduce it. The portable package is
`spacewars-handoffs/terrain-recovery-profile-20261010.tar.gz`. Its incremental
source bundle requires `43feacd` from the hatch-arrival package and includes
both diagnostic versions.

## #174 / #178 integration boundary

The publication check finds #174 still open at `52f9e18` and draft #178 at
`cada1eb`. The adjacent #174 checkout's runtime source matches the supplied
snapshot `f5c368b`: among 2,138 frozen paths only
`docs/late-match-dirt-review.md` differs, and its two additions are handoff
documents. Current main `6611a0b` differs from the integration base only in
`README.md`. The exact per-file hashes and GitHub head receipts are retained.

The next combined-source gate is therefore publication of #174's tested local
terrain work, followed by comparison against that frozen runtime. If its runtime
source is identical, preserve these results and verify the final merged tree.
If physics changes, establish new controls and compare the named candidate on
that physics; do not silently replace historical replay expectations.

Keep recovery policy and its integration with #178. Do not push the unpublished
terrain snapshot into that PR. After the final source is assembled, run combined
CI and device timing/playtests. The repeated native placement cost, Angular
enclosure, collapse-on replacement-distance gate and opponent rebuild stall
remain separate limits. Default/frontier promotion still needs its own evidence.

This work is local on `bot-recovery-candidate-profile`. The original #178
checkout stays clean at `cada1eb`; no PR, deployment or bot default changed.
