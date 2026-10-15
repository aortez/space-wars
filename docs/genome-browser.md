# Guidance genome browser and practice trials

The launcher’s **Genomes** view now connects saved guidance components to
repeatable practice trials and new training runs. It includes the retained
stationary finalist and three mixed-arrival finalists, plus genomes exported
by local training. The current view is for components; complete and hybrid
brain browsing remains part of the [broader design](design/evolutionary-training.md).

Choose a genome with the arrows. Its details show the supported role, parameter
count, origin generation, recorded training arrivals when available, and stable
genome ID. Recorded training scores describe the original recipe and build.
They are not a ranking across different curricula or evidence about the current
application build. **Refresh** reads newly saved candidates without changing
the selected ID when that entry remains available.

Choose **Stationary target** or **Moving target**, then **Try**. These are the
fixed `stationary_right` and `moving_right` arrival cases from the existing
[material arrival fixture](ship-arrival-diagnostic.md). The trial pins the exact
genome into the named practice brain, with ordinary flight controls and a
separate physical world. It reports outcome, elapsed ticks, final distance,
and relative speed. The static trajectory preview shows the ship path in blue,
the target path in gold, and the final ship position in white. It is not a live
view of training or an interactive match assignment.

Back leaves the browser while a trial finishes. Reopening preserves its selected
genome and most recent result. Changing genome or practice clears the displayed
result. A trial does not change the live match, saved controller settings,
training population, or random sequence. One bounded practice worker may run
alongside the single training worker; Pause remains available in Training if
foreground work needs the CPU. The open browser and a pending trial suppress
idle auto-start.

**Use seed** opens Training setup with that exact genome selected. Starting
creates a new job: one unchanged selected genome occupies the first slot and
the remaining slots are random. Every candidate is evaluated afresh. For the
mixed curriculum this selection replaces its usual bundled parent. It does
not resume the original experiment or import old fitness. The **Initial genomes**
row shows the selected ID and random count; either arrow restores the recipe’s
default population. Selection is disabled while a training job is active.

## Storage and identity

The job writer exports each completed generation’s selected genome into
`training/genomes/<genome-id>.json`, beside the existing job checkpoint and best
export. The library artifact includes the full model, version, identity, first
source, generation, recorded training counts, and original runtime. A checksum
covers the metadata and model. Saving the same genome again preserves the first
artifact; each later job checkpoint retains its own score history. Files are
created atomically without replacing existing entries. Corrupt or mismatched
artifacts are reported and excluded from selection.

Refresh also migrates valid `run-*.best.json` exports from earlier Training
versions into immutable entries, leaving the exports untouched. Their previous
generation and fitness are shown as unknown because a bare model does not
establish those facts. This scan is bounded to 256 training-directory entries.

Practice summaries are saved under
`training/trials/<genome-id>/<receipt-sha256>.json`. They retain the case, exact
practice-brain definition and identity, runtime, outcome, measurements, and
fitness. Repeating the same trial on the same build produces the same receipt.
The detailed trace stays only in memory for the current preview, bounded by the
fixture’s 1,200-tick limit. This avoids accumulating another collection of large
raw trace files.

The browser reads at most 256 saved directory entries per refresh, reports
truncation, and always includes the four bundled fixtures. It does not delete
older models. Current practice supports the declared Guidance v1 contract;
training additionally requires the frozen 9→8→3 architecture. Other valid
component artifacts can be inspected but unavailable actions stay disabled.
Model compatibility is independent of exact job-resume compatibility: a new
build can trial a compatible genome even when it cannot resume its old job.

## Control interface and verification

The screen ID is `launcher.genomes`. Controls are `genomes.previous`,
`genomes.next`, `genomes.practice.previous`, `genomes.practice.next`,
`genomes.try`, `genomes.seed`, `genomes.refresh`, and `genomes.back`.
Read-only controls expose `genomes.id`, `genomes.status`, `genomes.receipt`, and
`genomes.warning`.

```sh
spacewars-cli ui activate launcher.genomes --expect-screen launcher.main
spacewars-cli ui activate genomes.try --expect-screen launcher.genomes
spacewars-cli ui state --json
```

Commands are asynchronous; wait for `genomes.status` to become `ready` and check
the screen’s error and warning fields. Tests verify immutable provenance, safe
migration of earlier exports, corruption rejection,
repeatable trials, exact selected-genome seeding, and checkpoint continuation.
The real Slint/worker workflow covers both practice cases, leaving/reopening,
seed selection, job completion, and footer touch actions at 800×480, 1024×768,
and 480×800 at 1×. `SPACEWARS_GENOME_ARTIFACTS` retains these rendered screens.
No device rollout or physical-controller playtest is claimed by these tests.

Local Rust 1.89.0 validation on 2026-10-14 passed 692 client tests (four existing
ignores), 82 agent tests, 38 CLI tests, and 38 control tests. The client and CLI
build successfully with the locked, offline `ci` profile. Strict scoped Clippy
passes for agent/CLI/control; client linting retains the existing warnings
outside this work. Logs and the 1× browser screenshots are under
`target/training-ui/`.

Still planned: an evaluation-history UI, searching/filtering larger libraries,
renaming/pinning, multiple selected seeds, complete/hybrid brain entries,
animated replay with stepping, and assignment into ordinary gameplay scenarios.
