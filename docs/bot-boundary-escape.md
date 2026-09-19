# Escape guidance inside the arena

This follows the [opponent-response investigation](bot-opponent-response-forecast.md)
at `dd0c1d2`. A new headless experiment keeps escape trajectories inside the
arena and reserves braking room before handing control to a transfer. It avoids
the three previously recorded wall-impact cases, but **is not promoted**: the
quiet P2 acceptance match changes from a win to a loss. Default policies and the
previous optional escape remain available with their original behavior.

The useful result is a reproducible separation between geometric safety and
mission value. A ship can avoid the wall, establish separation, and still make
a poor next choice. Destination cover and the full successor remain unmeasured.

## Retained experiment

Enable `--disengagement-boundary true` alongside `--disengagement-seats`. The
flag applies only to the selected escape seats and is recorded in the report.
There is no new launcher setting or Pi deployment.

The original seven escape directions are joined by three directions centered
on the arena's inward radial direction. The existing coarse six-second inertial
forecast includes stopping clearance at each of its twelve half-second steps.
This is at most 120 coarse steps per escape start, compared with 84 before;
there are no new physics queries, graph expansions, or world clones. It retains
the existing preference for nonnegative obstacle clearance before separation.
When every alternative fails that estimate, it still chooses the least-negative
clearance. That is a fallback, not permission to call the route safe.

A local guard is armed only after the first actual escape attempt. For escape,
launch and transfer guidance it reserves a 20-unit origin-to-wall margin, 0.6
seconds of outward travel, and stopping distance at half observed brake
acceleration after subtracting outward gravity. It begins braking with another
20 units of reserve and releases above 40 units with no outward radial velocity.
While active it opens the wings and requests an inward velocity with limited
tangential travel through ordinary ship controls. No pose or velocity is forced.
The margins accommodate turning, opening wings, and braking relative to a moving
planet; they are not a proof under arbitrary contacts or future gravity.

Successful escape also requires stopping room throughout the existing one-second
separation check. The original twelve-second escape deadline, surface/recovery
priorities and timeout back into combat remain. The guard does not take over
local landings, on-foot recovery, pods, or combat. It remains armed for later
eligible transfers until episode reset. The read-only successor forecast keeps
a clone of this guard so that it predicts the same transfer controls; it cannot
start or recursively probe another escape.

Both the boundary experiment and its runtime telemetry are separate from the
older escape option. Clone/repeated-tick semantics remain deterministic. Reset
retains configuration and clears accumulated guard/escape state.

## Physical comparisons

Generated matches pair one v11 seat against v10. The main seed is
`7725194555774358125`, quiet or with one asteroid every three seconds; seeds 2
and 7 supply additional asteroid cases. Every run finishes a match, with a
600-second limit and the existing ownership tiebreak. These are saved acceptance
cases, not an unbiased win-rate sample.

| Case / v11 seat | Prior escape | Boundary experiment | Completed v11 sorties, prior → new |
| --- | --- | --- | --- |
| Main quiet / P1 | P1 win, 201.10 s | P1 win, 311.22 s | 1 → 2 |
| Main quiet / P2 | P2 win, 203.28 s | **P1 win, 203.07 s** | 1 → 1 |
| Main asteroids / P1 | P1 win, 313.13 s | Same | 1 → 1 |
| Main asteroids / P2 | P1 win, 128.45 s | Same | 0 → 0 |
| Seed 2 asteroids / P2 | P2 ownership win, 600 s | P2 win, 131.80 s | 5 → 1 |
| Seed 7 asteroids / P2 | P2 win, 180.95 s | P2 win, 225.28 s | 2 → 2 |

The three wall cases now have minimum ship-origin clearance of **315.56**
(quiet P1), **376.92** (quiet P2), and **90.98** (seed 2 P2), versus previous
recorded minima of 6.41, 5.18, and 4.56. New traces are dense from escape start
through six seconds after its first end; none of those intervals records a
world contact or a near-wall velocity impulse. The older quiet P1 impact is
still a geometric inference; the other two have retained world contacts. This
is not a claim that an entire match or a subsequent pod recovery avoids walls.
The unchanged main asteroid P2 loss still ends with a pod/world impact.

In all six retained experiment runs, the changed initial direction prevents
the emergency guard from needing to activate. A separate physical fixture
isolates the guard: from the same outward-moving start, ordinary guarded
controls keep more than 20 units of wall clearance and reduce outward speed
below 5; the unguarded control reaches within 10 units of the wall. The ablations
below also exercise the guard in complete matches.

In the quiet P2 regression, separation completes at tick 11,153 with ship health
39.48 versus the opponent's 98.31. The bot selects planet 0, pauses that transfer
for the nearby opponent at 11,439, loses its ship at 11,999, then loses the pilot
to a ship collision at 12,184. Avoiding the wall did not establish a useful
escape destination. A faster win in seed 2, caused by the other pilot dying to
an asteroid, does not establish better capture performance either. Seed 7 keeps
the win but now loses and rebuilds a ship during the sequence.

## Ablations and stopping point

Three seven-match passes isolated the behavior before the final option was
added. Their patches, binaries and traces are retained:

- **v1, retained behind the new option:** change initial direction scoring and
  provide the guard and handoff room check. Eliminates the recorded wall cases,
  but loses the quiet P2 acceptance win.
- **v2, rejected:** retain the old initial direction and add only braking/room
  checks. The ship repeatedly returns to its fixed outward escape direction
  after braking. Quiet P1, quiet P2 and seed 2 time out their escape; quiet P2
  and seed 2 both change from wins to ownership losses at 600 seconds.
- **v3, rejected:** also reflect the escape direction inward once when the
  guard activates. This reduces quiet P1 and seed 2 interventions from three
  to one, but still loses quiet P2 and seed 2. The latter loses its pilot to an
  asteroid at 150.13 seconds. This does not warrant more reflection or margin
  tuning on the same few seeds.

The next experiment should measure destinations before choosing a successor.
An immediate combat branch, continued escape within the original deadline, and
a transfer into measured cover need comparable evidence. The previously rejected
automatic transfer rule remains rejected; the forecast probe remains read-only.

## Destination-cover integration plan

Inspection confirms that changing the current `site_request` planet is
insufficient. Mission observations restrict detailed landing sites and ground
routes to the actual approach planet. All six earlier handoffs reported
`site_query: not_requested`, zero sites and zero cover entries. Unknown cover
must not be treated as either clear or blocked.

The next bounded slice can reuse existing material checks without constructing
a full remote ground graph during escape:

1. Add an explicit remote request/result alongside the local sensor request.
   Start with at most two eligible destinations and two candidate bearings each,
   requested when escape begins. Nominal planet geometry supplies only the
   shortlist. Keep one pending candidate per actor and cancel it when a local
   capture or recovery task needs the budget.
2. Reuse `vehicle_landing_site_with_queries` for actual feet, hull, hatch and
   boarding clearance, followed by the three existing `LandingCover` rays at
   heights 7, 30 and 60. Only surviving material of that destination can count
   as cover; detached fragments and arena walls cannot. Current opponent
   position defines that measurement, not a promise about the whole flight.
3. Charge before every query under the **same** allowance as live objective
   planning. The existing early-candidate adapter already performs a bounded
   atomic site check using up to 192 queries per actor. Generalize its query
   reservation/accounting for this second caller rather than granting another
   budget. Permit at most one such candidate check per actor per tick; local
   landing checks take priority, and recorded charges survive cancellation.
   A budget below the atomic cap explicitly defers this initial probe. If that
   becomes common, a resumable snapshot-backed site job is the next extension.
4. Publish `not_requested`, `pending`, `deferred`, `incomplete`, `no_landing`,
   `measured` or `stale` distinctly. An exhausted site check is incomplete even
   if its partial return resembles rejection. Each candidate carries its own
   measurement tick, terrain revision, planet frame, ship form, opponent pose
   and query charge; measurements from different ticks are not one coherent
   survey. Revalidate selected geometry and cover before commitment and expose
   invalidation when terrain or blockers change.
5. Keep this first addition diagnostic. A valid landing site with material
   cover still has **unmeasured capture/return-route feasibility**. Reuse the
   existing joint-trip planner when the destination becomes locally relevant;
   neutral planets without a flag must not be forced into a fabricated
   flag-objective job. Only after this contract works should mission selection
   consume the results.

Acceptance for that slice: both actors share one fixed quota; zero/small budgets
and repeated observations cannot overspend; exhausted work is not negative
evidence; moving blockers, excavation and detachment invalidate current claims;
local landing/recovery keeps priority; and diagnostic-only requests preserve
controls. Capture, recovery and survival after a handoff remain the behavioral
evaluation, including the quiet P2 regression and the existing seed 2 success.

## Reproduction and verification

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --disengagement-boundary true \
  --probe-disengagement-handoff true \
  --seed 7725194555774358125 --asteroid-interval 0 --trace true \
  --trace-start-tick 10718 --trace-end-tick 11800 \
  --out /tmp/boundary-escape-quiet-p2
```

Omit `--disengagement-boundary true` to replay the older escape policy. Omit
`--disengagement-seats` and its dependent options for ordinary v11. This last
control retains its main asteroid P2 loss at 459.58 seconds; the optional escape
still does not improve that earlier 128.45-second loss.

`target/boundary-escape/` contains exact argument arrays, three prototype
matrices, the final binary, seven final controls, six final enabled replays,
comparison scripts and test logs. The 34 new complete match reports pass physics
and shared-quota audits. Final controls are compared against `dd0c1d2` on full
recorded observations, actions, telemetry, outcomes and allocation rows; final
enabled runs reproduce v1 after removing only the added configuration field.
Timing fields are excluded from equality; every actual allocation is audited.

Final checks pass: 92 AI unit tests (including the new physical brake fixture),
20 mission integration tests and the physical combat/rebuild test. Client
compilation, AI all-target clippy and formatting also pass.

A verified evidence archive is retained under
`/home/oldman/.codex/visualizations/2026/09/19/bot-boundary-escape/`, including
prototype patches and the exact prior comparison artifacts. These are desktop
behavior experiments, not Pi timing results or evidence of a stronger default bot.
