# Recovery changes are separated and the combined tree is verified

See the [publication follow-up](terrain-recovery-publication.md) for the prepared
PRs and current integration gates. This report records the earlier local
separation pass and retains its original source identities.

The recovery work can now advance #178 without copying #174's unpublished
terrain implementation into its branch. The local
`bot-recovery-integration-prep` branch extends #178's `cada1eb` checkpoint with
the supplied diagnostic, the bot-owned changes through `eb41941`, and a CI job
that exercises `sensor-profile`. It remains a preparation branch: its retained
diagnostic tests depend on #174's terrain configuration APIs.

The local `bot-recovery-assembled` branch rehearses integration with the frozen
terrain snapshot and current main. Before the main/CI additions, its complete
Git tree is identical to the qualified `eb41941` tree, including source, tests,
dependencies and evidence. The later main merge changes only README.md. The
new CI job and this assembly receipt add no runtime changes.

## Source and ownership

| Source | Revision | Purpose |
| --- | --- | --- |
| Published draft #178 | `cada1eb12a2d8a394f285e0dbc47666c4f8bffca` | Bot branch base |
| Supplied #174 snapshot | `f5c368bff32d1604a2bc0c3c5c30d438b5562741` | Local terrain dependency, not published by this work |
| Original combined base | `6083226304e31d74ef141b7eb8e421bc181e755f` | Start of the post-handoff bot delta |
| Qualified recovery source | `eb419415c43245bf4e9bebab4c152fc27b51f20e` | Exact assembly reference |
| Current main | `6611a0b0eef4a13d5f5c061541a95d79cb82165e` | README-only update |

The preparation commits are:

- `e3303d8`: retain only the supplied test-only recovery diagnostic and module
  declaration. This is the foundation used by the subsequent comparisons.
- `f427c8e`: retain the 112-path bot delta, including the complete recorded
  experiments, tests, results and auditors. Of those paths, 111 match the
  qualified file objects exactly. The remaining file, Rapier's world module,
  adds only the four connectivity diagnostic declaration/export lines to the
  original bot source; terrain CCD and dependency declarations stay with #174.
- `08ab58f`: add the focused experimental recovery CI job.

This branch has no frozen-terrain ancestor. Neither the terrain runtime,
vendored physics changes nor terrain evidence is imported into its delta.
The original #178 checkout stays at `cada1eb`, and the original qualified
worktree stays at `eb41941`.

The rollup preserves the existing experimental options so that retained
comparisons remain reproducible. It does not enable the unsuccessful standing
rejoin, corridor or direction experiments. The named
`terrain_recovery_supported_hatch_v1` profile and default behavior keep their
[qualified meanings](terrain-recovery-profile-results.md).

## Integration rehearsal

Merging the frozen terrain source into the separated bot branch encounters two
conflicts. Both have concrete, retained resolutions:

1. `crates/engine-rapier/src/world.rs`: retain the bot's ball-cast,
   forecast-body and connectivity modules alongside terrain's CCD module and
   clearance dependency exports.
2. `crates/spacewars-ai/src/combat_pilot/recovery_diagnostic.rs`: retain the
   bot's extended diagnostic. The supplied original diagnostic is already its
   foundation; the overlapping differences are its header and two submodules.

Two previously recorded line wraps in `scenarios/spacewars/src/physics.rs`
and `scenarios/spacewars/src/terrain/tests/loose.rs` are applied only after the
terrain merge. They add no tokens. The resulting merge `731d69a` has tree
`f4d1291db979109bcb3388c89fc2abfb16f43bd6`, exactly the tree of `eb41941`.
Merging current main produces `1727849`; its only difference is README.md.
Applying the CI change produces validation source `8097da7`.

The full-file/object comparison covers all 2,439 files in the qualified tree.
No match replay or expected trace is regenerated. The preceding full-match
qualification remains applicable to this identical runtime; this rehearsal
does not count as additional recovery or bot-strength evidence.

## Validation and CI

The existing CI workflow never selects `sensor-profile`. The new
`spacewars-recovery` job explicitly runs the recovery-profile library tests,
native ground/boarding integration tests, supported-corridor observation tests
and strict AI Clippy. The long retained-world diagnostics stay ignored and
continue to require their frozen packages. No launcher or default selection
changes.

The actual commands from the new YAML job were run locally using Rust 1.89.0,
the locked dependencies, the CI optimization profile and a 16 MiB test stack:

| Check on assembled source | Result |
| --- | --- |
| AI library, `sensor-profile` | 454 passed; 17 ignored diagnostics |
| Ground navigation | 45 passed |
| Native surface recovery | 51 passed |
| Ground posture observations | 3 passed |
| Strict AI library/test Clippy | Passed |
| AI library/test compilation, `sensor-profile` | Passed |
| Workspace formatting | Passed |

There are **553 passing tests** in the new CI job. The YAML parses and its
three run commands succeed; this is local validation of the job, not a GitHub
Actions run. Whole-workspace/platform CI and device timing are still final
integration gates.

On the separated branch alone, default-feature library compilation passes.
Compiling the retained diagnostic tests with `sensor-profile` fails with five
errors referring to three absent terrain APIs: `engine_common::DirtMode`,
`surface_sortie::dirt_config` and `SurfaceSortieState::enable_dirt_mode`.
The assembled source supplies all three and compiles successfully. The supplied
diagnostic is a test module even without `sensor-profile`, so the separated
branch is not a standalone CI-ready PR update. Do not add substitute terrain
APIs or rewrite historical fixtures merely to bypass this dependency.

A whitespace check on the terrain merge also reports three pre-existing blank
lines at EOF in supplied terrain log files. Those evidence files are preserved
byte-for-byte. The bot delta and new workflow have no whitespace errors.

## Final integration sequence

1. The #174 owner publishes the tested terrain work. Recheck its source against
   `f5c368b`; the published `52f9e18` checkpoint is insufficient. Preserve the
   raw package and historical controls. If runtime source changed, assess it
   and repeat the affected controlled qualification on that exact physics.
2. After the terrain source is available in the agreed target branch, advance
   #178 with `bot-recovery-integration-prep` and merge that published terrain
   source. Use the rehearsal to resolve the two known conflicts; examine any
   new conflict separately. Do not substitute this local snapshot for the
   terrain owner's published work.
3. Compare the assembled source with the qualified tree. With today's main,
   only README.md, `.github/workflows/ci.yml` and these two assembly documents
   are expected differences. Review any additional difference before relying
   on the old replay results.
4. Run combined CI, then device timing and playtests. Update #178's description
   to reflect the final implementation and evidence before promotion.

Repeated native placement cost, the sealed Angular pocket, the collapse-on
replacement-distance gate and the later opponent rebuild stall retain their
[documented limits](terrain-recovery-profile-results.md). No default/frontier
promotion or claim that every recovery succeeds follows from this assembly.

## Retained artifacts

The [compact receipt](data/terrain-recovery-assembly-v1.json) records source
objects, validation commands, results and raw artifact hashes. Working files
and logs are in `spacewars-handoffs/recovery-integration-prep-20261010`.
The portable `terrain-recovery-assembly-20261010.tar.gz` contains:

- a bot-only source bundle requiring `cada1eb`;
- a rehearsal bundle requiring the bot branch, `f5c368b` and current main;
- the exact extraction and formatting patches, assembly audit, compile
  dependency failure, successful CI logs and commands;
- source comparison and integration instructions.

The preceding terrain and recovery packages retain the prerequisites and full
match evidence. Import the bot bundle before verifying the rehearsal bundle.
Neither the bundle nor its preparation constitutes a PR update or deployment.

At the final publication check, #174 remains open at `52f9e18`, draft #178
remains at `cada1eb`, and main remains at `6611a0b`. No GitHub write was made.
