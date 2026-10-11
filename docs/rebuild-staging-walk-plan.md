# Walking the measured staging leg within the original deadline

Freeze three modes before replay: retained staged search as control, continuous
walking only during staging, and continuous staging with the already measured
walking map handed to the ground task. All three retain opt-in placement
refinement and persistent search. Do not tune or repeat a mode after seeing its
result. These changes remain local and opt-in.

The existing continuous-walk controller uses full walking input only on supported,
balanced, forward walking interiors. Keep proportional final approach, posture,
jump and flight behavior, and the strict supported arrival radius below 0.12.
Keep the original missing-site deadline of 300 ticks, task birth at 16820,
ground allowance, four-relocation limit, one-stage limit and end at 29421.

For the handoff mode, request the native map only with a staging proposal. Reuse
that survey's actual tick, planet, terrain revision, nodes and walking/jumping
edges; omit its added jetpack edges. No extra physics survey, forecast or changed
cadence is allowed. The ground task applies its existing map validation and one
pure route query, requiring a complete 2–4 unit walking route to the precise
staging point. Invalid handoff data falls back to the ordinary survey path.
The task issues no movement on the proposal tick. The next actual step still
checks armed controls, posture, material queries, terrain revision and support.

After physical arrival, keep the fresh onward placement/route survey, native
eight-second rebuild timer, landing, two-foot settling and boarding checks.
Neither map reuse nor arrival resets the original search clock. Continuous
walking is disabled when a new ground task leaves the staging destination.

Each frozen command replays the original prefix to 23767, then runs both a
recorded-controls continuation and a live-task continuation. P1 follows its
original tape throughout. Require the control to reproduce all 22 preceding
staged files byte for byte. Record first differences, walking onset, measured
map handoff, actual arrival, fresh placement selection, subsequent route progress,
native builds, settling, boarding and terminal reasons for every mode.

Freeze source inputs, all new modules, commands and one release executable
before the three prefixes and six continuations. Use the existing work bounds,
native world audits and lossless trace archives. Preserve all negative results.
Audit-only repairs may reuse hash-verified raw data and must be disclosed;
partial simulations cannot be retried. No fresh games are included.

Success requires live native recovery completion with the pilot alive and no
additional ship loss. Passing the staging leg alone is intermediate evidence,
not full-chain validation or justification for default/frontier promotion.
Export the reports, dense movement witnesses, validation logs, frozen source,
provenance and diagnosis in the portable review bundle.
