# Fresh climb-laser lost win: recovery and exposed acquisition

The integrated candidate's world-2 lost win is reproduced exactly. The extra
laser damage brings forward destruction of the opponent's parked ship,
restarting its blocked recovery. The opponent returns to combat while the
candidate is still waiting for its first landing site. The candidate keeps
waiting under fire, loses its ship, and later dies after a projectile spins
its braking pod into the world boundary.

The next bounded experiment should address **incoming fire during airborne
first-site acquisition**. This diagnosis also identifies a separate stale
recovery task: a successful physical rebuild does not release a previously
blocked controller. Neither finding changes the completed comparison's retain
decision, the laser gate, or bot defaults.

## Scope and exact reproduction

The [frozen observation plan](pursuit-climb-laser-loss-plan.md) selects
`fresh-world2-v10-asteroids0-p1-candidate-{off,on}` from the
[192-game comparison](pursuit-climb-laser-results.md). Both use seed
11223442104665788832, integrated v13 in P1, v10 in P2, no asteroids and the same
shared execution-routes host. The only new runtime flags enable the existing
native impact observer for both actors throughout the game, with bot controls.

Both replays retain exact hashes for **nine original streams**, including all
dense observations, actions and capture evidence. Non-timing reports, sensor
streams and charged planning ledgers agree. Existing physical, configuration,
route/budget and laser audits pass; player results, allocation, continuation,
route summaries and retry results match the original games. Every one of
**136,536 impact rows** joins its current pilot/action observation. There are
zero control overrides, and final native round receipts match.

These are two observational replays of a selected counterexample, not new
strength samples. The related v16 result remains supporting evidence from the
same world/seat cluster; it receives no additional replay or full-trace identity
claim here. The shared host does not establish device or stock-host performance.

## How the paths separate

Ticks identify observed simulation state; controls at T affect the next step.
The first extra request is not the first damaging hit or flight change.

| Tick | Evidence |
| ---: | --- |
| 3,995 | First added laser request, with unchanged native flight and inputs. |
| 3,996 | First energy-supply difference; neither ship's motion changes. |
| 14,946 | First differing hit counter and opponent pilot health, while that pilot is on foot. |
| 21,948 | In both arms, P2 physically rebuilds a full-health ship while its recovery task remains blocked. |
| 26,526 | Both arms finish their fourth common capture-and-departure sequence. |
| 26,902 | First differing hull on P2's parked ship: 99.8679% off versus 99.7982% on. |
| 28,129 | A P1 cannon projectile spawned at 28,099 leaves the off ship at 0.0651% hull but destroys the on ship. The latter resets P2's blocked recovery task. This is the first differing vehicle motion. |
| 28,157 / 28,158 | First non-laser action differences for P2 / P1; P1's motion first differs at 28,159. |
| 28,861 | The off arm finally destroys P2's parked ship with another cannon projectile, 732 ticks later. |
| 30,718 / 30,793 | In the on arm, P2 rebuilds again and boards. Its recovery completes at 30,794. |
| 30,905 | P1 starts local capture acquisition, without a selected landing site. |
| 31,348 / 31,361 | P2 first requests fire from its recovered ship; P1 first observes laser damage. |
| 31,605 | A P2 cannon projectile spawned at 31,590 destroys P1's ship. |
| 31,848 | Another P2 cannon projectile, spawned at 31,835, sharply accelerates and spins P1's pod. |
| 32,268 | Boundary impact kills P1. The off arm instead wins at the 36,000-tick ownership limit. |

All four completed P1 visits have identical recorded milestones, including
departures at **2,585 / 6,940 / 18,575 / 26,526**. The loss is not a missing
completion among those visits. P1 owns two planets to P2's one at both endings;
pilot death overrides that lead in the enabled game.

Immediately before the cannon contact at 28,129, the parked opponent has
**42.1322% hull off versus 41.8686% on**. The small difference changes whether
that contact is fatal. Both source/spawn clocks agree, and P1 requests the
recorded cannon shot. Extra damage therefore changes recovery timing, rather
than directly damaging the candidate. This reconstructs the actual chain; it
does not isolate the necessity of every earlier added beam.

## Acquisition prevents a defensive response

From **30,905 through 31,604**, all 700 observations remain in capture `survey`:
no selected site, no admitted objective route, no supported feet, and the pilot
still aboard. There is one handoff observation, followed by 699 current
acquisition receipts: **676 scan deferrals and 23 candidate rejections**.
All 23 evaluated scans reject their directions for unavailable survey evidence.
Missing admitted evidence does not establish that no feasible site exists.

The mission coordinator's `pursuit_opportunity` returns early whenever a capture
task exists. Its later incoming-fire check therefore cannot interrupt this
airborne survey. This is broader than preserving a landed or on-foot capture.

P1 continues surveying for **244 control updates / 4.07 seconds** after first
observed damage. It requests neither weapon. Cannon contacts at 31,392 and
31,484 remove 35.0565 and 33.6768 hull points respectively; intervening laser
damage leaves 12.9398% hull before the fatal third contact at 31,605. Each
contact has a current native source/spawn receipt and a matching P2 request.

Only four of those 244 observations have non-marginal present-aim/readiness
windows for either weapon: 31,491–31,492 and 31,603–31,604. These are possible
requests, not demonstrated hits or a rescue. Copying the climb's laser-only
hook into acquisition has not been established as sufficient.

This differs from the earlier [exposed-site diagnosis](threatened-capture-approach.md):
here no site is selected. The experimental [30-second acquisition deadline](bot-bounded-acquisition.md)
is disabled in this configuration, and its deadline alone would fall after the
recorded loss at 11.67 seconds. Its different hold guidance has not been tested
on this trajectory. Previously retained cover/escape experiments remain gated.

## The pod is already braking

The first pod observation at 31,605 is unarmed. All **662 armed updates from
31,606 through 32,267** request brake with no thrust. Native center-of-mass
motion, rather than the spinning vehicle-origin velocity, shows:

| Tick | Event | COM speed | Spin, rad/s | Radial boundary clearance |
| ---: | --- | ---: | ---: | ---: |
| 31,605 | Pod appears | 35.97 | 1.98 | 427.81 |
| 31,847 | Before the later projectile contact | 16.76 | 1.04 | 412.28 |
| 31,848 | After that contact | 186.87 | -104.55 | 411.27 |
| 32,267 | Last observation before fatal impact | 27.40 | -62.66 | 1.51 |

Protection ends at 31,785. The later projectile's pilot-damage receipt records
40 health points and labels its cause `missile`; native debris provenance labels
the same contact `cannon`, with a 13-tick-old projectile and P2 firing at its
spawn tick. The ship-damage clock is still 31,605 and is **not** reused as a new
ship-loss receipt. Subsequent laser damage leaves 53.6891 pilot health, which
the final world contact removes at 32,268.

The final interval is not a missing-brake defect. No alternative escape,
steering policy or outcome is proven by these motion measurements.

## Separate recovery lifecycle defect

P2's recovery task blocks at **8,328** after four measured relocation attempts.
Physical rebuilding nevertheless completes at **21,948**, producing a full-health
ship. The task remains blocked for another **6,913 ticks off / 6,181 ticks on**,
until the next ship destruction resets it. In the off continuation P2 eventually
blocks again and never completes recovery; in the on continuation it completes
at 30,794 and resumes combat.

The source explains this behavior: `RecoverShipTask::step` short-circuits a
blocked task before checking the newly available ship, while the mission
coordinator starts a new recovery task on an increased ship-loss counter.
Destroying the ship clears stale task state that the successful rebuild did not.
This warrants a separate correctness fix with a regression for newly observed
physical progress. Any reactivation should be tied to a new physical state,
not an unconditional retry of the same exhausted search.

The baseline's opponent weakness is part of this retained result. It is not a
reason to tune the laser to preserve a damaged enemy ship or to declare the
enabled combination safe. Evaluate recovery changes explicitly, rather than
silently changing the opponents in a laser comparison.

## Next bounded work and evidence

Test an opt-in defensive handoff when fresh hostile damage is observed while
the bot is airborne and still lacks its first capture site. Preserve ground and
solar safety, landed/on-foot tasks, completed visit milestones and an immutable
episode deadline; repeated hits must not restart it. Compare the full return to
objectives and survival, retaining this loss, earlier successes and fresh worlds.
Choose the trigger and successor behavior before running that experiment. This
is a hypothesis, not an implemented rescue or a default recommendation.

The runner, nine new tests and observation plan were committed at `993314a`
before the two games. All **845 Python tests pass**. The existing qualified
`b729cd8` binary remains unchanged; no Rust rebuild or new Rust test result is
claimed. All **46 frozen tool/plan inputs** retain their hashes. There was no
game rerun, audit correction, policy modification or threshold tuning.

The [manifest](data/pursuit-climb-laser-loss-v1.json) binds the replay summary,
original evidence, binary, validation and archives. Its
[review archive](data/pursuit-climb-laser-loss-v1.json.gz) includes the exact
summary, audits, runner/tests/plan, post-review script, selected actor snapshots,
all recorded damage/contact transitions and the acquisition interval. Full
reports, raw streams and complete derived points remain in two lossless local archives
under `target/pursuit-climb-laser/lost-win-v1/archives`; every member is checked
before removing generated uncompressed copies. Original comparison archives
remain intact. Work is local, and defaults remain unchanged.
