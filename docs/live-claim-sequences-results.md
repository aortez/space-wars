# Post-capture diagnosis of the two opposite live-claim outcomes

All four observational replays reproduce the original games exactly. The
disputed captures succeed in both configurations. Their later trajectories
change combat exposure and ship losses; both losing evaluated pilots die in
world-boundary impacts after projectile/debris contacts during pod recovery.
The surviving no-stop pilot wins on owned planets while its ground recovery
is blocked. That recovery contains a concrete false crossing completion.

This completes the [frozen sequence plan](live-claim-sequences-plan.md).
It does not change the [broader evaluation's](live-claim-evaluation-results.md)
rejection of no-stop promotion. These are four selected known cases, not new
strength evidence. Runtime, bot defaults and deployment are unchanged.

## Capture completion and later outcomes

Ticks are native simulation ticks at 60 Hz. Claim/departure columns refer to
the first capture affected by the stopping-rule difference.

| Case | Claim | Departure | Later ship loss | Evaluated pilot / round end |
| --- | ---: | ---: | --- | --- |
| World 1/P2, integrated | 7,343 | 7,571 | Laser, 13,032 | Boundary death / loss, 13,337 |
| World 1/P2, no-stop | 7,457 | 7,685 | Asteroid, 16,820 | Alive / time-limit win, 36,000 |
| World 3/P1, integrated | 7,659 | 8,077 | None | Alive / win, 21,833 |
| World 3/P1, no-stop | 7,685 | 8,100 | Cannon, 10,946 | Boundary death / loss, 11,509 |

In World 1, the evaluated P2 has only **1.153% hull** in both variants before
the stopping-rule split, following the earlier common fight. The integrated pilot completes
its next departure at 11,753 and pursues a nearby opponent at 11,754. No-stop
departs at 11,859 and instead selects another transfer at 11,860. It abandons
that approach at 14,544 and later starts a vulnerable-opponent pursuit at
14,894. Both versions have three claims and departures by the integrated
pilot's death. No-stop survives an asteroid ship loss, lands its pod at
18,276, exits at 18,278, and eventually wins with two owned planets to zero.
It never rebuilds its ship or completes recovery.

In World 3, no-stop claims the next planet earlier, at 9,955 versus 10,045,
but takes **686 ticks from boarding to departure**, versus 334 integrated
ticks. Its hull falls from **99.609% to 61.303%** during departure. The native
stream records laser damage and a cannon receipt at 10,328; the latter tick
reduces hull from 98.619% to 65.886%. No-stop begins its post-capture pursuit
at 10,644 and loses the remaining ship at 10,946. Integrated transfers to
another planet, survives with 97.924% hull, and wins when the opposing pilot
hits the sun at 21,833. Both evaluated variants already have four claims and
departures by tick 11,509.

These sequences establish changed exposure after successful captures. They
do not isolate a universal benefit of arriving earlier or later, or prove
that declining either pursuit would have survived. The earlier
[pursuit-health gate](pursuit-health-gate.md) already failed its comparison;
these observations do not qualify that rejected rule for use.

## Pod contacts and braking

Both losing pods request full braking and no thrust on every observed tick
after their initial recovery-transition tick: **304/304** World 1 ticks and
**562/562** World 3 ticks. The native observer confirms those controls were
not overridden. Turning remains controlled by the bot.

The measurements below use native **center-of-mass** motion. Distance is the
ray from the center of mass to the circular world boundary along the current
velocity; braking distance is `speed² / (2 × observed brake acceleration)`.
These are straight-line diagnostics that omit body extent, gravity, future
contacts and steering, not proofs that a death was unavoidable.

| Evaluated pod / tick | Native contact label that tick | COM speed | Distance along velocity to wall | Ideal full-brake distance |
| --- | --- | ---: | ---: | ---: |
| World 1 integrated, 13,200 | Before the contact sequence | 15.57 | 479.36 | 3.03 |
| World 1 integrated, 13,201 | Cannon | 90.06 | 427.11 | 101.39 |
| World 1 integrated, 13,202 | Fragment | 202.42 | 422.23 | 512.18 |
| World 1 integrated, 13,204 | Fragment | 227.95 | 417.95 | 649.54 |
| World 3 no-stop, 11,066 | Cannon | 145.34 | 347.62 | 264.04 |
| World 3 no-stop, 11,104 | Fragment | 59.91 | 1,587.39 | 44.87 |
| World 3 no-stop, 11,304 | Cannon | 187.66 | 389.99 | 440.21 |

In World 1, the cannon contact at 13,201 precedes three successive fragment
contacts at 13,202–13,204. The largest speed increase occurs before spawn
protection ends at 13,212: protection from pilot damage does not prevent
these measured motion changes. Forty-three later laser-damage events leave
98.176 pilot health before the fatal boundary impact at 13,337. The final
pre-step COM speed is 141.86, with only 2.72 units along its velocity to the
wall. Native contact closing speed at death is 137.77.

In World 3, the first cannon contact at 11,066 raises COM speed from 16.91
to 145.34. A fragment contact at 11,104 changes direction and reduces speed;
the pod subsequently slows to 18.72 before another cannon contact at
11,304 raises speed to 187.66. Thus the earlier hit alone does not explain
the ending. The pilot receives 83 laser-damage events and one missile-damage
event (the pilot-vitals name for the cannon projectile), followed by the
fatal boundary impact. It has 53.845 health before that impact. At the last
pre-step sample, COM speed is 53.48, with 1.55 units along velocity to the
wall; native contact closing speed at death is 59.65.

Contact labels are associated with the measured tick; the last-contact
field is not an exhaustive decomposition of every force during that step.
The initial ship-to-pod conversion is retained separately from same-form
motion changes. Vehicle-origin speed would be misleading: at 11,066, its
velocity differs from COM velocity by **83.23 units/s** because the pod is
spinning rapidly. No counterfactual controls were tested here. The earlier
[pod-boundary experiment](pod-boundary-impact.md) remains separate evidence.

## The surviving run has a false ground-gap completion

World 1/P2 no-stop starts recovery ground travel at 18,278. Its telemetry
records four completed gap flights:

| Ground-gap nodes | Completion tick | Foot distance to takeoff | Foot distance to destination |
| --- | ---: | ---: | ---: |
| 55 → 62 | 18,562 | 3.038 | 0.072 |
| 85 → 90 | 19,180 | 4.442 | 0.264 |
| 116 → 120 | 19,762 | 3.387 | 0.018 |
| 121 → 124 | 19,963 | **0.108** | **3.353** |

Foot position is reconstructed from the consumed native actor position and
up vector using the runtime's 0.45-unit half-height, then transformed into
the planet frame. The raw observations and derived positions are included
in the review bundle.

The fourth flight switches from `Cross` to `Descend` after one tick. At its
reported completion, the pilot is supported and balanced near the original
takeoff endpoint. The crossing controller's arrival test uses destination
error projected onto the pilot's current right vector, which is only
**−0.017** there. It does not require proximity to the destination footing.
See [the completion condition](../crates/spacewars-ai/src/jetpack_crossing.rs)
and [the route handoff](../crates/spacewars-ai/src/ground_task/jetpack.rs).

The parent clears its route and counts a completion. Two ticks later, at
19,965, it selects **the same 121 → 124 gap in the same direction**, with a
route containing only node 121. After recharge, it enters `Approach` at
20,147 and never launches again. At 21,466 it waits for a terrain survey;
at 21,495 the same corridor is revalidated to revision 21. It remains in
`Approach` until the overall ground deadline blocks recovery at 23,679,
**58.87 seconds after approach began**. The planet-revision change therefore
does not explain the original retry or clear it. The ground wrapper updates
`last_progress_tick` while following this crossing, even without a phase
advance; that timestamp alone is not proof of physical progress.

This is a reported completion, not a successful crossing of that gap. The
win still includes 12,321 final blocked mission ticks and no rebuild. The
earlier report's claim that the fresh games had no jetpack crossings has
been corrected: its route audit inspected capture tasks, while these flights
belong to `mission.recovery.ground`. All four selected capture-task counters
remain zero. This diagnosis does not count recovery flights across the other
44 fresh games.

## Next testable hypothesis

Test whether a ground-gap crossing must reach **the destination footing in
the planet frame** before completing and releasing its route. The existing
one-dimensional arrival condition accepts a source-side landing here.

A focused test should reject this exact false completion while accepting
the three preceding genuine arrivals. Then an explicitly planned candidate
replay should check useful forward travel, claim/rebuild/boarding progress,
bounded retries, fuel and survival. Merely suppressing the completion counter
or timing out earlier would not count as a recovery improvement. No fix or
new candidate replay is part of this checkpoint. Combat exposure and pod
survivability remain separate work; so does the World 3/P2 no-asteroid
integration regression identified by the broader evaluation.

## Verification and retained evidence

- Qualified executable remains from `e7683bf`, SHA-256
  `7c5e6a65a612f0866648ad61252d89ee02b19ddcf60e7d9041a2cea9f055acd6`.
  No Rust code changed or runtime rebuild was performed.
- Every deterministic original stream matches byte-for-byte; non-timing
  reports, sensor records, physical outcomes and planning results match too.
  The harness/observer seat remains P1; evaluated-seat analysis correctly
  uses P2 for World 1 and P1 for World 3.
- All **165,358** dense actor rows join to unchanged observer controls,
  actions and native state. Four final observer records reconcile the last
  physics step. Both actors' losses, phases and records are retained.
- **924 Python tests pass**, including ten tests for this step. The unchanged
  runtime reuses its previous 616 passing Rust tests, formatting and scoped
  Clippy qualification; those checks are not claimed as newly executed.
- The initial audit stopped at the opponent's voluntary on-foot scuttle
  at 11,892 in World 1 no-stop. Commit `ba64aad` distinguishes its held
  three-second chord, completed native progress and loss transition from a
  damage receipt. The frozen commands, runtime, plan and 924 other inputs
  stayed unchanged; only the new runner and tests changed among 926 inputs.
  Explicit `--reaudit` reused the first two recordings and ran the remaining
  two games once. No completed or partial game was retried. The failed
  summary/log and initial verified archive are retained.
- Four current archives preserve **72 files**, **3,760,683,901 raw bytes**,
  compressed to **499,329,670 bytes** under
  `target/live-claim-sequences/v1/archives/`. Their hashes and every member's
  size/hash are bound in the manifest; the earlier archive is retained too.

The committed [manifest](data/live-claim-sequences-v1.json) links the
[compressed portable review bundle](data/live-claim-sequences-v1.json.gz).
It includes the frozen plan, qualified source summary, original and replay
reports, full derived sequence diagnostics, selected recovery observations,
commands, source text, validation logs, audit interruption/resume receipts,
and reproducible derivation/export scripts. Full raw streams remain in their
lossless local archives. Branch: `bot-live-claim-sequences`.
