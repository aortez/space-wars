# The sampled Round openings do not produce a successful recovery

Fresh ordinary, search/arrival and full candidate recovery tasks all fail at
both retained openings. At **25,500**, they request standing repeatedly and
eventually stand, but never walk out or find reachable rebuild space. At
**25,800**, they produce the same game inputs as the unchanged blocked control.
Neither fork demonstrates a usable opportunity for an on-foot retry policy.

This is a negative result for these controllers and these two forks. It does
not prove that every possible walk, rotation, jetpack or debris-pushing sequence
would fail. No on-foot reconsideration, bot default or deployed behavior changed.

## Same-world comparisons

The [plan](terrain-recovery-round-opening-plan.md), source and executable were
frozen at `a6a0aed` before the single comparison run. The unchanged seed-42 Round
control is reconstructed through 25,800, with separate native clones at 25,500
and 25,800. Each fork has an unchanged blocked arm and three diagnostic resets
of only the recovery task. Private combat memory, opponent, physical world and
RNG begin identically; opponents remain reactive. Candidate groups are selected
explicitly. Each arm continues for 18,000 ticks.

| Fork | Original physical condition | Fresh ordinary, search/arrival and full candidate result |
| --- | --- | --- |
| 25,500 | Knocked down, stable contact on retained planet; standing obstructed | All block at 26,910: no reachable standing site with hatch access |
| 25,800 | Balanced, stable contact on a loose body; no retained-planet support; standing obstructed | All block at 26,821 with the same reason |

There is **no native ship reconstruction or completed recovery** by 43,500 or
43,800 respectively. No arm has another subject ship loss. The three fresh
configurations produce identical encoded game inputs to one another at each
fork, despite their different native placement diagnostics.

At the first fork, each fresh task issues **68 primary-input ticks**, including
**65 native standing attempts** in the first 900 ticks, and no horizontal or
interaction input. The first standing attempt is blocked at 25,501; a later one
starts at 25,636 and succeeds at 25,637. The unchanged control also becomes
balanced at 25,638 without an explicit standing request. The fresh actor's maximum
once-per-second sampled displacement in planet-local space is about **1.37 units**.
It does not execute a walk/jump/jetpack route out of the pocket. The terrain
component is already closed again in the sample at 25,620.

Standing changes the physical continuation from 25,636. With a reactive opponent,
17,571 ticks of combined game input differ from the blocked arm; that figure
must not be confused with the subject's 68 primary-input ticks. The fresh
candidate groups still match the fresh ordinary controller's encoded inputs.

At the second fork, all subject movement, primary and interaction inputs remain
zero. The fresh ordinary continuation also retains every blocked scenario
checksum. Candidate placement options change some diagnostics from 26,520, but
still produce identical encoded game inputs. The pocket is closed in the sample
at 25,860. Standing on a loose body is insufficient for the ordinary bot's
retained-floor claim and crawl permissions.

Search/arrival and full each receive **531** rebuild surveys at the first fork
and **526** at the second. Maximum coarse work is eight sites; no refinement,
offset or staging work is reached. No terrain-flight forecast, saved-normal
capture or settling-forecast job activates. The placement features cannot supply
a route from the pilot's current position.

## Native clearance explains why empty cells were insufficient

The new `sensor-profile` diagnostic reads the real actor collider without
advancing physics or supplying information to the controller. It records both
raw and query-separated pose fits, eight straight sweeps at fixed orientation,
and ungated versions of the ordinary crawl queries. Every call asserts the
native physics snapshot bytes remain unchanged.

At 25,500, neither the actual pose nor its 0.025-unit contact-separated query
fits without overlap. The longest of the eight six-unit sweep queries reaches
only **0.26103 units**, in the right contact-tangent direction. With the ordinary
0.04 margin that leaves **0.22103 units**, with retained floor at all three
probe points. This is shorter than the ordinary unbalanced crawl's required
0.6-unit step. Left clearance is zero. The ordinary crawl sensor initially skips
these measurements because no blocked standing attempt has yet been observed.

At 25,800, the actual support collider is a loose body, not the retained planet.
Both crawl-direction clearances are zero, with no retained-floor ray hit at the
probed contact. The longest directional sweep is only **0.004495 units**. Ordinary
crawl readings are absent because retained support is missing, even though the
body is balanced. Both pose-fit checks fail here as well.

These queries include bodies absent from the retained material grid. They
measure the current collider at fixed orientation, including a query-only
separation allowance. They do not identify a particular blocking body, prove
that a rotating or pushing actor cannot escape, or establish a physics defect.
The native standing attempts and full continuations provide the separate test
of what these recovery controllers actually do.

## Verification and scope

The [audit receipt](data/terrain-recovery-round-opening-v1.json) records:

- **25,800** prefix action/decision/scenario rows and **430** samples matching
  the earlier control exactly.
- Both fork states and native material dumps matching the earlier retained
  states; 9,000 and 8,700 old blocked continuation ticks retained respectively.
- **17,700** overlapping ticks of the two new blocked arms matching exactly.
- **2,374** frozen source inputs and **205** raw evidence files verified.
- Terrain conservation and speed checks throughout, 900 consecutive posture
  decisions per arm, and 30 native clearance/material measurements per arm.

Passed: locked Rust 1.89 release build, the frozen comparison, **433 AI library
tests**, **2 native posture tests**, strict AI Clippy, default-feature library
check and formatting. No simulation was rerun or tuned after seeing results.
The added native diagnostic is excluded from default builds and is called only
by the developer harness.

Strict Clippy for the wider native scenario crate reports **seven pre-existing
warnings** in four files unchanged from `85bdc13`: collapsible conditionals,
field reassignment after `Default`, and an existing eight-argument function.
The failed strict check, successful ordinary Clippy run and byte-equality proof
for those files are retained. This is not a clean whole-workspace Clippy claim.
The previous broad gameplay suite is not rerun for this diagnostic-only change.

Raw evidence and validation logs live outside Cargo's target in
`spacewars-handoffs/recovery-round-opening-20261010`. The portable handoff is
`spacewars-handoffs/terrain-recovery-round-opening-20261010.tar.gz`; its source
bundle requires the preceding `85bdc13` results commit. Work remains local on
`bot-round-opening-recovery`; the original #178 checkout is unchanged. No PR
updates were made.

The recovery-policy conclusion is to keep on-foot retries out of this change.
For #174, this provides native support and clearance evidence on the frozen
terrain source, without a demonstrated terrain/support defect to fix. For #178,
retain Round as a documented limitation while completing final terrain-source
integration and ordinary-match qualification of the Angular improvement.
If escaping under loose debris becomes required scope, investigate executable
locomotion separately; additional controller resets alone have not established
that capability. Reconsideration should follow a demonstrated executable route.

The subsequent [fresh combat screen](terrain-recovery-fresh-matches-results.md)
retains ordinary match behavior in both seats on the same terrain source.
