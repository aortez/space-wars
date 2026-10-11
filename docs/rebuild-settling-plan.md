# Native rebuild settling experiment

The high-terrain candidate reaches the recovery flag and rebuilds twice, but
neither replacement settles for boarding. Both poses passed the old standing
support-normal test. The first ship has two supported feet but a native radial
landing angle of 24.77 degrees at tick 26000; the second has one supported foot
and 23.37 degrees at the final failure. Native settling requires an angle below
20 degrees. Landing assistance damps motion but does not rotate an empty ship.

## Frozen change

For each existing placement offset, use its measured two-foot floor and normal
to predict the resting ship origin. Reject the pose if its direction does not
meet the existing native radial landing-angle limit at that origin. Publish the
measured angle and `landing_misaligned` reason in the shared native report and
relocation preview. Retain all hull, hatch-footing, route and other-seat checks.

This is a shared native placement correction. Keep the landing threshold,
settling duration, rebuild timer, four candidate offsets, relocation bound,
ground and flight deadlines, physics, combat and pod control unchanged. The
high-terrain forecast remains opt-in and unchanged. There is no default bot
promotion, remote deployment, fresh seed search or native pose correction.

Regression tests retain both failed poses and check their angle independently
of world rotation/translation, preserve a valid native placement, and verify
that the placement query is read-only and rejects dirty geometry.

## Fixed comparisons

Freeze committed source/input hashes, both copied executable hashes, predecessor
summaries and exact commands before executing:

1. The existing 20 three-minute `run-ground-navigation-trials.py` cases, without
   jetpacks: 18 expected completions and two intentional bounded failures. This
   includes both seats, pod recovery and crater-driven rebuild relocation.
2. World 1 / P2 integrated retained full game.
3. World 3 / P1 integrated retained full game.
4. World 3 / P1 no-stop retained full game.
5. World 1 / P2 no-stop affected full game.

Preserve every full-game argument from the high-terrain experiment, including
its opt-in forecast flag. No retuning or game retries within this experiment.
Explicit audit-only repairs may reuse hash-verified raw evidence. Preserve
negative results and complete all comparisons even if qualification fails.

## Acceptance and evidence

Run existing physical, capture, stopping, observer, terrain reserve/deadline
and destination-footing audits. Audit every published native and preview
placement: accepted directions must be below 20 degrees, and alignment
rejections must meet or exceed it. Record actual native rebuilds, settling
transitions, boarding, recovery counters and bounded failures.

For each full game, record first observation, action and native-state changes
against the preceding high-terrain run. Physical comparison removes only the
placement diagnostic report, retaining all body motion, support, native status,
counters and ownership. Report full non-timing report parity separately.

Advancement requires all established trials to retain their expected result,
three controls to retain actions and native state, and the affected case to
complete the high crossing, claim, native rebuild and real boarding/completed
recovery. Preserve the original winning outcome, zero pilot deaths and at most
one ship loss. A rejected bad pose, preview or counter change alone does not
qualify. Keep all raw files in hash-verified lossless archives and export a
portable review bundle. Broader validation remains a separate step; the older
narrow-flight forecast discrepancy is outside this placement correction.
