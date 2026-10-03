# Bounded escape after an actual-hatch abort

## Frozen question and policy

The [actual-hatch recovery experiment](actual-route-recovery.md) ended the
retained stalled landing, but the mission immediately started an incoming-fire
pursuit and lost its damaged ship. Test whether a bounded physical departure
gains separation while preserving defensive fire and higher-priority safety.

The `actual_capture_escape_v1` profile is opt-in through
`--capture-escape-seats none|0|1|both`, default `none`. It requires ValuePlanner
and actual-route recovery. Arm once when the mission consumes a native
actual-hatch failure on the tick immediately after the neutral abort. The
current actor, landed pose and flag objective must still match. The ship must
be available, aboard, armed and ready, with a visible, unoccluded live opponent
ship inside 350 units. Delayed, stale, moved or unthreatened aborts do not arm it.

Use the existing boundary-aware escape direction predictor and flight routing.
Keep the axis fixed; use radial lift while supported, then ground clearance,
obstacle routing and boundary braking. The original abort tick fixes the
deadline at **720 ticks / 12 seconds**. Hits, solar avoidance, unavailable
controls and target changes cannot restart that clock. Recovery takes priority.
No escape action presses interaction or authorizes hatch exit, jetpack, landing,
claim, boarding or a crossing. No planner budget or publication gate changes.

During escape, take only the existing combat controller's weapon actions;
discard its pursuit flight controls. This retains readiness, visibility,
occlusion, range, aim, ground-clearance and combat-break checks. Weapons remain
subject to those checks; permitting them does not guarantee a shot or hit.

Finish early only after 60 ticks of observed separation: no supported feet,
source-planet radial clearance above 70, boundary stopping clearance above 20,
and the original opponent either terrain-occluded or at least 350 units away
with nonnegative opening speed. Missing or replaced targets do not prove cover.
Early separation permits destination choice; new pursuit remains deferred only
until the original deadline. Otherwise timeout returns to ordinary mission
choice. These thresholds reuse the existing disengagement policy rather than
being fitted to the retained match. No game or bot default changes.

## Comparison plan, fixed before candidate outcomes

Baseline: `target/actual-recovery/v1/summary.json`, SHA-256
`e1810fbe8fbd22d9d488381bee62ed8f5e62ba38dc93988d5fa78065b378e186`.
Keep all **17** existing cases, commands, seeds, opponents, physics and budgets.
First replay all with the new option disabled and require exact report, seven
stream, sensor and allocation parity. Then run all 17 full comparisons: enable
only the evaluated seat in the 15 previously enabled cases; retain the two
cover-off controls. The three health-enabled regressions remain separate from
the eight primary armed matches. Use at most two simulations concurrently.

Freeze implementation, tests and this plan before inspecting candidate results.
Use a hashed copy of the profiled release binary. Do not tune the policy from
these outcomes. `tools/validate-capture-escape.py` retains the earlier native
route, physical transfer, flight, initial-cover, handoff and actual-abort audits.
Every arm, active tick and completion has the exact consumed observation, flight
and weapon actions, source receipt, clock and counters. Audit immutable clocks,
source binding, pursuit deferral, current separation and defensive weapon gates.
The first changed control must follow a recorded arm. Cases without an arm must
remain exact after removing only the new option's telemetry, preserving all
earlier failure/abort evidence.

Measure the actual liftoff, source clearance, opponent range/occlusion, damage,
loss or sustained separation, deadline/end reason, later captures/departures and
final result. Report all eight primary armed matches and the health cases,
including regressions. Suppressed pursuit is not evidence of successful escape.
These correlated development cases do not establish independent playing
strength or Raspberry Pi performance. Work remains local.

## Audit correction

The initial Python audit swapped the interaction and brake byte positions and
rejected a valid braking packet at tick 8040. Native `SurfaceSortieAction`
encodes turn, thrust, interaction, brake and seat in that order. The correction
uses named decoded controls and adds a regression with the actual native packet,
checking that braking passes and interaction fails. The bot implementation and
frozen binary from `24929ed` are unchanged. Re-audit all 34 saved matches, hashing
the gameplay inputs before and after; preserve the failed summary and original
checker alongside the corrected audit rather than rerunning or tuning physics.

## Results

The bounded escape improves immediate ship survival but regresses both changed
full matches. Keep it opt-in. Both copies of the stalled visit leave the surface,
reach almost 399 units from the opponent and survive the full escape window.
Neither establishes the required uninterrupted 60 ticks of separation before
the original deadline. Both later lose without another completed capture or
departure. No threshold or default was changed after observing these outcomes.

### Physical escape and subsequent mission

The primary and health-enabled copies share the initial escape:

| Tick | Observed event |
| ---: | --- |
| 7656 | Native actual-hatch failure aborts capture at 31.862 hull. |
| 7657 | Mission consumes the failure and arms the escape. |
| 7660 | Ship is flying with no supported feet; the old response lifted at 7662. |
| 7900 | Actual radial clearance from the source planet exceeds 70. |
| 8198 | First hull reduction during escape. |
| 8222 | Nearest planet frame changes from planet 0 to planet 2. |
| 8302 | Opponent range reaches 350 and separation first qualifies. |
| 8338 | Boundary stopping margin resets the separation hold and engages braking. |
| 8339 | Separation qualifies again, with too little original time left for 60 ticks. |
| 8376 | Original deadline expires; mission selects planet 2 and starts transfer at 31.174 hull. |

The last controlled escape observation is tick 8375: range **398.981**, opening
speed **20.909**, source radial clearance **736.263**, and boundary stopping
clearance **63.223**. These are measured world distances, not the capped landing
altitude sensor. The boundary guard intervenes once and holds for 38 ticks.
Each attempt has 718 controlled ticks: 2 lift, 240 climb, 438 escape and 38
boundary ticks. It emits no laser or cannon shots; the preserved weapon gates
never permit one on this trajectory. The arming and timeout observations make
720 retained escape witnesses per copy.

The primary mission abandons transfer at **8671** for a discretionary pursuit
of a nearby opponent after securing ground. Cannon fire destroys the ship at
**9254**, compared with laser loss at **8071** in the baseline. It then loses
the match to a missile while in a pod at **10031**, compared with **22016**.
Longer survival of this ship does not translate into more completed objectives.

The health variant defers that discretionary chase and continues traveling,
but starts an incoming-fire pursuit at **9152**. Cannon fire destroys its ship at **9270**
and its pilot dies to a missile at **10349**, compared with a baseline match
ending at **21354**. The health check deliberately preserves incoming-fire
responses, so it does not prohibit this later chase. These are correlated
copies of the same initial landing, with different later trajectories.

### Complete retained outcomes

All **17 disabled replays** are exact. Of the **17 comparisons**, 15 never arm
an escape and retain exact behavior after removing only the new option's
telemetry: seven streams, physical outcomes, ordinary sensor work, non-timing
native telemetry and allocation ledgers. Each changed case arms once at 7657,
has its first changed control at **7658**, and times out once at 8376. There is
no early separation, repeated arming or renewed deadline.

The eight primary armed cases retain **2 wins**, while claims fall from
**28 to 26** and completed departures from **27 to 25**.

| Primary case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 4 → 4 | 4 → 4 | 21907 → 21907 |
| World 0 P1 powered | win → win | 5 → 5 | 5 → 5 | 36000 → 36000 |
| World 0 P2 walking | loss → loss | 4 → 4 | 4 → 4 | 26591 → 26591 |
| World 0 P2 powered | loss → loss | 4 → 4 | 3 → 3 | 30536 → 30536 |
| World 1 P1 walking | loss → loss | 1 → 1 | 1 → 1 | 36000 → 36000 |
| World 1 P1 powered | loss → loss | 3 → 1 | 3 → 1 | 22016 → 10031 |
| World 1 P2 walking | win → win | 4 → 4 | 4 → 4 | 36000 → 36000 |
| World 1 P2 powered | loss → loss | 3 → 3 | 3 → 3 | 36000 → 36000 |

The world 0 P2 powered successful-exit control remains exact: positive actual
route at 18288, hatch exit at 18289, and the same later unresolved return.

The health regressions remain separate:

| Health case | Result before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 3 → 3 | 16190 → 16190 |
| World 0 P1 powered | loss → loss | 3 → 3 | 15014 → 15014 |
| World 1 P1 powered | loss → loss | 3 → 1 | 21354 → 10349 |

### Verification and evidence

Implementation, tests and the policy plan were frozen in **`24929ed`**. The
checker correction is **`22f0354`**, with no Rust or simulation changes.
All **1,054 Rust tests** and **710 Python tests** pass, including native abort
binding, stale-source rejection, fixed deadlines during safety overrides,
boundary braking, defensive weapon gates, clone/reset and native control-byte
decoding. Formatting, strict AI Clippy with `--no-deps`, and profiled/ordinary
release builds pass. Scenario Clippy retains its same seven pre-existing findings.

All 34 saved-run audits pass. The correction verifies all **374 gameplay input
files** before and after re-analysis, preserving the initial failed summary and
checker. Native dispatch remains capped at **4 graph operations / 384 queries**,
with maximum publication age **120 ticks**. No live budget, physical permission
or default changes.

The frozen profiled binary is
`target/capture-escape/surface_mission_soak-24929ed`, SHA-256
`3ca268a81967e700424c7ab76befdc7903507d6a6f2182341bcc9e942f0a744d`.
The complete summary at `target/capture-escape/v1/summary.json` has SHA-256
`b9caf00030385f1cce0602bdaff4f7412d9699ada60f5dda7862d661398e700f`.

[The manifest](data/capture-escape-v1.json) records all outcomes and checks.
[The archive](data/capture-escape-v1.json.gz) retains the frozen plan, summaries,
all consumed escape observations, prior native receipt/abort and physical/route
witnesses, first changed controls, complete mission transitions and damage
evidence, validation logs, correction/reproduction scripts and raw-file hashes.
All **150 embedded documents**, **619 raw files**, retained source inputs and
both frozen binaries pass hash verification. Work is local, with no default
promotion or deployment.

### Next investigation

Investigate the handoff from escape into destination travel, including which
threats should cancel that travel and which should permit defensive fire while
continuing it. The health variant already defers the discretionary chase, but
incoming fire later cancels its transfer too. Test any bounded commitment rule
against the complete retained suite; extending this escape deadline to fit the
known near-success would not establish a general improvement.
