# Ordinary native offsets restore the retained Angular recovery

**The extra native rebuild placement is the trigger in this retained case.**
Restricting actual placement to ordinary offsets, while keeping the candidate's
sensing and recovery controller, restores recovery at tick 25,282. All 12,363
remaining inputs and the native match result then match the original standard
Value v13 control. Disabling supported hatch arrival does not repair the failure;
that branch dies sooner. A short movement pause preserves life but not recovery.

This completes the [15-minute diagnostic plan](terrain-recovery-angular-diagnosis-plan.md).
No production behavior, profile definition, device setting or GitHub state was
changed. These are component ablations of one retained failure, not fresh
qualification or grounds to promote a default bot.

## Controlled comparison

The frozen runtime is `3e4827c3e5d8902e0291e1dc03e4b9e8c7c56dd6`; binary SHA-256
is `0c9f04ad8ba598eeaa7463945417f36ccda7ef3fac6f0f8e396ccc77d99211e1`.
The first compile needed one diagnostic type correction: use the pilot's typed
vehicle identifier in the boarding assertion. Both build attempts are retained;
the correction precedes every simulation run.

The original held-out seed is 13656578344987333250, Weak Angular, collapse on,
quiet, Value v13 with `terrain_recovery_supported_hatch_v1` on P2. Replay verifies
the initial world/controller bytes and every decision, encoded input and scenario
checksum through tick 23,637. Four continuations clone the complete native world,
both controllers and host queues immediately before the early rebuild. Physical
fingerprints match; the hatch-only branch additionally verifies that restoring
its one option restores the complete controller debug state.

Each continuation runs to native pilot death or the existing 36,000-tick limit.
The unchanged branch verifies all remaining 9,926 decisions and exact final
observation bytes, reproducing all 33,563 decisions of the original failure.
No controller resets or physical-rule edits occur. Applied input overrides and
the actual refinement setting used for each physical step are explicitly logged.

| Continuation | P2 recovery completion | P2 death | Blocked ticks after fork | Result |
| --- | --- | --- | ---: | --- |
| Unchanged candidate | None | 33,563 | 9,060 | Original inaccessible return reproduced. |
| Ordinary physical-step offsets | 25,282 | None by 36,000 | 1,569 | Recovers, completes one sortie; matches the standard control's remaining inputs and match result. |
| Original hatch-arrival rule | None | 23,893 | 1 | Dies from terrain-fragment impact while returning to the rebuilt ship. |
| Pause movement for 900 decisions | None | None by 36,000 | 11,806 | Remains alive but cannot establish a measured return route. |

The ordinary-offset branch still uses the paired candidate observation driver
and all its controller options. It turns refinement off only after each decision,
for native placement in the ensuing physical step. This is a diagnostic
separation, not a new production host API or a silently redefined named profile.

The pause branch neutralizes horizontal, jump/thrust and brake input at ticks
23,638–24,537, while preserving interaction and other inputs. Controller clocks,
host work and the reactive opponent continue. Normal inputs then resume. None
of the paused decisions boards the ship; survival alone is not a recovery.

## Placement and return evidence

The three branches retaining physical refinement rebuild at offset −11.5 at
23,638. The placement report accepts a 1.63-unit static route to the predicted
settled hatch. The actual new ship is initially flying/unsettled, with its
currently exposed hatch about 19.66 units from the pilot. Source inspection
confirms that placement forecasts the resting pose while spawning the ship in
a raised pose; the forecast is not proof that the intervening settling and
pilot return will succeed.

In the unchanged branch, the pilot first has support on a different planet from
the ship at 24,198. The ship first reports landed at 24,504, when return navigation
blocks with `other_planet`. The earlier sparse transition evidence recorded the
block at 24,504; the new per-decision trace identifies the earlier separation.

Ordinary offsets reject that early opportunity and instead rebuild at −8 at
25,206. The current hatch is then about 1.99 units from the pilot. The ship
lands and native transfer becomes ready at 25,281; recovery completes at 25,282.
The final pilot is alive, with one recovery, one sortie, one ship loss and one
native rebuild. Its entire post-fork applied-input sequence and complete native
match observation equal the original standard control.

Original hatch arrival first changes input at 23,835 and dies at 23,893, before
the ship settles. Pausing movement avoids the cross-planet separation, but the
ship never reports landed in the retained decisions, and its final landing
state is assisted with zero supported feet. The pilot ends alive and blocked
with “no measured walk/jump/jetpack route to destination.” Neither intervention
demonstrates a usable return from the early refined placement.

The controlled offset restriction therefore identifies a successful conservative
alternative for this physical state. It does not establish that every extra
offset is bad, or fully separate unstable settling from route execution and
subsequent terrain contacts. Native support, clearance and boarding guards
remain authoritative in all four branches.

## Validation and next step

All four declared continuations finish in 36.86 wall seconds. The independent
audit verifies identical fork physics, all 900 input interventions, native
boarding, 587 material/speed samples and the unchanged reference reproduction.
Source provenance verifies 2,465 frozen files. **454 AI tests and strict AI
Clippy pass**; formatting and diff checks pass. No simulation is rerun and no
adverse result is discarded.

The next small implementation should make native placement refinement separately
selectable from recovery sensing, then qualify an explicitly named candidate
that uses ordinary native offsets while retaining the recovery improvements.
Preserve this failure, the earlier Round improvement and the Angular P1 delay
as checks, followed by fresh paired seeds. Keep supported hatch arrival enabled
for that comparison. Broader settling forecasts and placement changes can remain
a separate investigation if this conservative candidate is useful.

The [compact receipt](data/terrain-recovery-angular-diagnosis-v1.json) links the
runtime, audit, placement inspection and source verification. Raw evidence is
in `spacewars-handoffs/recovery-angular-diagnosis-20261011`; the portable archive
is `spacewars-handoffs/terrain-recovery-angular-diagnosis-20261011.tar.gz`.
From an extracted package, run `python3 audit-results.py` to inspect saved output
without simulating. Its source bundle requires merged main `f717a7f` and includes
the preceding held-out qualification commits.
