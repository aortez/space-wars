# Acquisition-defense counterexample: native impact diagnosis

**The survival regression reproduces exactly.** Defense changes P2's motion
beside the arena boundary; a cannon contact at tick 15,498 coincides with ship
destruction, 82 ticks before the baseline. The enabled pod subsequently takes
two cannon hits and dies against the world boundary at 15,971. The baseline
survives until laser fire kills its pilot at 18,397: **2,426 ticks / 40.43 seconds
later**. Braking remains requested throughout both armed pod continuations.

This is observation of the selected counterexample from the
[failed acquisition-defense screen](acquisition-defense-results.md), not a new
strength sample or a policy fix. The runtime, thresholds and defaults are
unchanged. The evidence supports testing whether an escape with negative
forecast clearance should be rejected before interrupting native acquisition.

## Replay identity

The [observation plan](acquisition-defense-loss-plan.md), runner and eight tests
were committed at `091fdd6` before either replay. Both games use the original
`c59fac6` binary, SHA-256
`fb3e2feb2c7bed2ffb68841942f98291af3789ff41de6e4fd96f06f31e11375c`.
The case is `fresh-world1-v10-asteroids3-p2-{off,on}`, seed
14699744800433948105: integrated v13 in P2, retained v10 in P1, three-second
asteroid interval, climb laser enabled for P2 in both arms. Only the output path
and existing native impact observer are added to each original command.

Each replay preserves all **nine original non-timing streams byte for byte**,
including the full dense trace. Non-timing reports, sensors and charged planning
ledgers also agree. Physical visits, routes, budgets, allocation, continuation,
prediction, retry, defense and laser audits retain the original results. All
**68,736 native impact rows** join to their exact actor, clock, actions and
motion, with **zero control overrides** and matching native final-round receipts.
All 62 frozen input hashes and the original binary remain unchanged.

## The handoff starts beside the wall with a weakened ship

Both arms retain identical completed visits, with departures at **3,155 and
5,545**. Earlier cannon hits at 4,433 and 6,799 remove 17.22 and 42.67 hull
percentage points. Before the trigger, cumulative recorded damage is 59.88
points labeled cannon, 14.43 laser and 0.33 asteroid, leaving **25.35% hull**.
These are the native aggregate damage labels, not a split of every contact.

The current acquisition starts at 12,563. At the qualifying laser-hit observation
**15,455**, hull is 25.31%; the native acquisition reports `scan_deferred`, with
no selected landing site or objective route. Defense changes native capture's
small turn and thrust into full turn (`-1`), brake and no thrust. P2's motion
first differs at 15,456; P1's controls respond at 15,456 and its motion differs
at 15,457. Tiny laser-damage differences begin at 15,458.

All **43 defensive updates, 15,455–15,497**, use boundary guidance, the same
full-turn/brake/no-thrust controls, open wings and neither weapon. The original
720-tick deadline is 16,175; ship loss preempts it at 15,498.

At the trigger, radial clearance from the vehicle origin to the world circle
is 3.29 units; center-of-mass clearance is 4.42. The guard subtracts its existing
20-unit margin and stopping allowance, yielding **−16.71** stopping clearance.
Source-planet radial clearance is 494.30 units. Boundary guidance
therefore dominates this episode; it is not a near-ground climb response.
During defense, center-of-mass clearance falls as low as 3.55 and stopping
clearance as low as −20.11. These center/origin distances are not hull-contact
separations.

The chosen six-second escape forecast has minimum clearance **−16.47**.
The selector prefers nonnegative candidates; when all candidates have negative
clearance, it returns the least negative one. Acquisition defense currently
accepts that result. This coarse forecast neither models every collision nor
proves that every physical escape is impossible. Braking also does not
universally prohibit thrust: the shared guidance can request both when aligned.
The no-thrust observation here is specific to these 43 updates.

## The earlier ship loss

At **15,498**, each arm records one new cannon contact with spawn tick **15,463**,
an age of 35 ticks. P1 is the only actor whose native cannon request at that
spawn clock is followed by an actual shell-counter increment. This binds each
contact to a corresponding P1 firing event; the record does not provide a
cross-replay projectile identity or establish identical trajectories after
the control divergence.

| Observation | Defense off | Defense on |
| --- | ---: | ---: |
| Hull before the contact | 24.98396% | 24.98391% |
| Heading before the contact | 1.23139 rad | 1.56491 rad |
| Recorded hull reduction at 15,498 | 15.45268 points | 24.98391 points |
| Hull after the contact | 9.53128% | ship lost |
| Ship-loss tick | 15,580 | 15,498 |
| Ejection protection ends | 15,760 | 15,678 |

The baseline later loses its remaining 6.14% hull at 15,580, with a current
cannon contact spawned at 15,555. Native firing counters again verify P1's shot.

The collision implementation makes ship/debris damage depend on contact impulse,
effective mass and the projectile's damage scalar; it is not a fixed cannon
damage amount. The changed pose and motion precede different damage outcomes.
However, the observer lacks full-ship contact manifolds and per-contact hull
damage. Its source label prioritizes a current weapon hit, and destruction
records only the remaining hull. **24.98391 is a capped aggregate loss, not the
unclipped damage of that shell.** The trace cannot isolate how much each
simultaneous contact contributed, or attribute the outcome to the turn or brake
bit alone. Full-ship contact instrumentation would be needed for that narrower
physics claim.

## The pod deaths have different causes

Both pods have one initially unarmed ejection update. Thereafter all **472
enabled** and **2,816 baseline** armed updates request braking and no thrust.
Earlier ejection also makes enabled protection expire 82 ticks sooner; the first
pilot-health difference appears at 15,708.

| Arm / tick | Current native evidence | Consequence |
| --- | --- | --- |
| Off / 15,689 | Cannon spawned 15,675; ejection protection active | Physical kick without pilot damage |
| On / 15,805 | P1 cannon spawned 15,795; 40 pilot damage | COM speed 2.61 → 70.59; spin −0.024 → −9.79 rad/s |
| Off / 15,808 | P1 cannon spawned 15,795; 40 pilot damage | COM speed 2.61 → 141.38; motion initially inward |
| On / 15,928 | P1 cannon spawned 15,915; another 40 pilot damage | COM speed 7.85 → 121.06; spin 1.39 → 146.44 rad/s |
| On / 15,971 | Native world contact, closing speed 298.89 | Remaining 10.29 pilot health removed; match ends |
| Off / 16,029 | Native world contact, closing speed 12.67 | Survives 2.69 pilot damage |
| Off / 18,397 | Current laser damage | Remaining 0.08181 pilot health removed; match ends |

Actual P1 firing increments corroborate all listed cannon spawn clocks. The
pilot-damage API calls these contacts `missile`; the debris provenance identifies
the cannon source. After the enabled second hit, center-of-mass radial clearance
is 6.23 units. At its last live update, 15,970, speed is still 96.59, spin 142.24
rad/s and radial clearance 1.82, with braking requested. The final world receipt
reports contact closing speed separately from center-of-mass speed; spinning
vehicle-origin motion must not be substituted for either quantity.

The baseline also takes a substantial projectile impulse and a later wall hit,
but survives those contacts and is eventually worn down by laser fire. Its
recovery becomes blocked at 16,990; neither arm completes another capture or
rebuild. This case supplies no new evidence about the separate stale-recovery
defect. Neither a missing brake request nor a new defense timeout explains the
enabled pilot's death.

## Next bounded experiment

Test a handoff admission rule: when the best existing escape forecast has
negative clearance, retain the native acquisition update and state instead of
starting defense. Preserve a receipt explaining the rejected proposal. Freeze
the rule before games and retain this failed experiment unchanged.

This would reject the observed proposal. The previously rescued qualification
has positive forecast clearance (**69.30**), so its initial proposal would pass;
that does not establish the full policy's outcome. Later qualifying hits could
produce different proposals. A new experiment must verify the complete known
rescue and this counterexample, disabled parity, the other retained results and
completed visits before evaluating a separately frozen fresh screen. These
selected cases are now diagnosis/qualification evidence, not fresh strength
evidence. No counterfactual control policy has been run in this diagnosis.

## Validation and artifacts

**864 Python tests pass**, including eight new tests for selected replay identity,
preserved options, current contact clocks, spawn ordering, actual firing versus
requests, ambiguous shooters and bounded last-contact provenance. There is no
runtime rebuild or new Rust-test claim. Both games complete on their first run;
no auditor correction or gameplay rerun is required.

The [manifest](data/acquisition-defense-loss-v1.json) binds the original evidence,
commands, binary, frozen inputs, comparisons and raw archive members. The
[review bundle](data/acquisition-defense-loss-v1.json.gz) contains exact plans,
auditors, relevant runtime sources, physical/defense audits, joined contact
evidence, selected traces, pod motion, reviews and logs. New raw and derived
evidence is archived under `target/acquisition-defense/loss-v1/archives`:
**32 files, 1,678,262,725 raw bytes in 191,315,981 compressed bytes**, with
every member verified before removing generated raw copies and source
extractions. Original archives remain unchanged. Work is committed locally.
