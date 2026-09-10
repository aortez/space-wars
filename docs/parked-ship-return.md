# Returning to parked ships on material corners

This follows [landing on generated material planets](large-planet-landing.md).
The target is seed 0/P1 mirrored in the generated quiet arena: the previous Pi
build landed, exited and claimed planet 2, then waited beside its healthy ship
for the rest of the three-minute run.

## Recorded cause

Replays of the archived `cc5b030` binaries match all 180 per-second motion hashes
on each platform. The Pi completes no departure; the desktop control completes
two. A temporary read-only contact trace also retains every Pi motion hash.

Both feet still touch retained planet material. At 59.5 seconds their contact
normals have outward components 0.978 and 0.717, qualifying for initial landing.
By 60 seconds the round foot on a voxel corner has settled to 0.692, just below
the per-foot 0.7 cutoff. At 150 seconds it is 0.643. Both contacts remain within
contact slop and nearly motionless relative to their supporting surface. The
hatch position remains measurable, but the landing gate becomes `Settling`.

Ground navigation considers the nearby hatch reached and repeatedly refreshes
its progress time. A later recovery task inherits the same misconception. The
existing fallback only recognizes a missing hatch or a landed ship on another
planet, so this present but unboardable hatch never reaches it.

## Changes

Initial touchdown retains its existing per-foot 0.7 alignment threshold and
quarter-second settling gate. An already landed full ship on material terrain
can retain two-foot support if each current contact has outward alignment at
least 0.5 and their mean remains at least 0.7. Strictly qualifying contacts keep
their original selection. The tolerance only applies on the same planet and
only while both real retained contacts survive; contact separation, separating
velocity, ship angle, speed, spin, wings and thrust still gate landing. It is
lost on failed revalidation, so it cannot be used to acquire a new landing.
`corner_support` reports when the retained pair uses this tolerance. Material
edit checks record the same qualifying foot cells.

This changes support recognition, not ship orientation, gravity, forces or the
physics step. It gives humans and bots the same boarding behavior on corners.

`ground_navigation_v9` waits at a visible hatch when the shared world reports
`ShipNotSettled`. Arriving there no longer continuously renews host progress.
After fifteen seconds from supported arrival it reports `UnsettledShip`.
World motion, local hatch movement and brief contact loss do not restart that
wait. An actually settled ship returns to ordinary boarding, and active jetpack
crossings retain their existing completion and interruption rules.

`recover_ship_v8` can route this typed failure into the existing bounded
replacement action. The balanced spaceling must have real support and stable
relative motion. The nearby assigned full ship must be in the same planet
frame, within 24 units, moving radially and tangentially below one unit/second,
and spinning below 0.2 radians/second relative to that frame. A visible hatch
alone cannot cancel replacement while the ship remains unsettled. A real landed
hatch or ready boarding gate releases the chord. The ordinary human three-second
scuttle and eight-second rebuild rules perform the loss and replacement; the
policy makes no world writes. The one-replacement limit and existing task
budgets remain in force.

## Validation

Five new tests cover physical corner contacts and prior landing, a complete
original-ship return after generated flight/capture in both reflections, the
bounded visible-hatch wait, settlement during that wait, and replacement
cancellation/unstable-motion exclusion. Existing terrain-removal, ground-route,
jetpack, impact/recovery, landing and interplanetary regressions remain required.

The corner-contact unit fixture uses an archived physical pose and supplies
prior landing state explicitly to isolate the gate. The mission regression and
long-run replays earn landing through ordinary flight and physics. Decision
fixtures isolate timeout and cancellation rules; real losses and replacement
remain covered by the unchanged physical return and impact matrices.

Artifacts and exact replay/build/validation scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/parked-ship-return-20260910/
```

## Results at `e37a042`

The complete matrix contains 48 generated missions, 52 fixed-world missions,
36 impact recoveries and 24 return trials per platform: **320 three-minute
runs / 16 simulated hours**. Every physical audit passes finite state, bounded
speed and retained-plus-removed material conservation. All 72 impact trials
recover and depart. All 48 return trials board and depart; the sixteen reachable
controls keep their original ships and the other 32 perform exactly one
replacement. Their departure ranges remain 5.65–6.07 seconds for reachable
ships, 32.37–32.52 for tipped ships and 17.35–17.50 for foreign-planet ships.

The Pi batch driver exited with signal 15 after collecting twenty of its
24 return results. It had copied two further reports without recording their
exit status. No remote runner remained active. All four unconfirmed cases were
rerun successfully with the same archived binary; completed cases were retained.
The counts above describe the final complete matrix, excluding interrupted or
unconfirmed attempts. The interruption and resumed driver log are archived.

The workspace suite passes 1,097 tests with 26 display-dependent tests ignored
in that invocation. Eight example tests bring the total to **1,105**. All ten
terrain UI workflows pass explicitly under Xvfb across both renderers; their
82 artifact files are archived. Formatting passes, Clippy completes with
existing advisories outside the changed files, and all six `navigation-v1` and
twelve `strategy-v1` frozen episodes match.

The final targeted Pi trace matches all 180 motion hashes of its final matrix
run. It matches the old physical trajectory through 62 seconds; the first
changed second includes ordinary boarding after capture. Its measured stages are:

| Stage | Time |
| --- | ---: |
| Earned landing | 59.53 s |
| Exited | 59.55 s |
| Planet claimed | 62.63 s |
| Boarded original ship | 62.65 s |
| Departed planet 2 | 66.43 s |

It subsequently departs planet 0 at 121.12 seconds and is attempting planet 1
at the cutoff. It loses no ship and completes no replacement. All four original
large-planet seed-0/P2 cases still depart all three planets. The seed-0/P1
reflections on both platforms all depart their first destination using their
original ship. Magnified before/after renderer frames at 60 seconds preserve the
same physical pose and show the corrected support recognition.

Compared with the archived `cc5b030` build, using identical configurations:

| Generated outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: any completed trip | 24/24 → 24/24 | 23/24 → 24/24 |
| Quiet: three distinct completed trips | 15/24 → 15/24 | 12/24 → 13/24 |
| Quiet: all three owned at cutoff | 19/24 → 19/24 | 15/24 → 16/24 |
| Interceptor: any completed trip | 9/12 → 9/12 | 9/12 → 9/12 |
| Interceptor: three distinct completed trips | 3/12 → 3/12 | 3/12 → 3/12 |
| Duel + asteroids: any completed trip | 9/12 → 9/12 | 9/12 → 9/12 |
| Duel + asteroids: three distinct completed trips | 1/12 → 1/12 | 2/12 → 2/12 |

The second improved Pi quiet subject is seed 1/P2 in the original reflection:
it now completes all three trips without a recovery, compared with one trip
and one recovery previously. Quiet subjects have no final blocked task.

Fixed-world mission outcomes are unchanged on each platform. Both complete
all twelve quiet two-planet routes and all eight deliberate-loss recoveries,
including both trips. Under combat/asteroid pressure, both complete sixteen of
32 two-planet routes; desktop records eleven recoveries and Pi nine. Each
platform retains two final blocked pressure subjects.

Across the one hundred mission trajectories per platform, 99 desktop and
95 Pi runs preserve every previous per-second motion hash. All fixed desktop
trajectories are unchanged. Changed trajectories and concurrent headless jobs
mean these runs are not a controlled timing comparison; live rendering is
measured separately after deployment.

## Remaining boundary

This establishes the corner-landing return fix. It does not establish general
combat or damaged-ground route reliability. Generated final blocks remain in
three desktop subjects and four Pi subjects. Pi seed 1/P2 in the original
interceptor case now reports a bounded replacement failure instead of continuing
to cycle through waits: its entire physical trajectory matches the old run, and
neither build completes a trip. The pilot is unable to establish the supported,
stable footing required for replacement. This is an explicit report of an
existing stalled return, not a successful recovery or a new ship loss.

No final matrix subject needs to complete a replacement specifically through
the new `UnsettledShip` classification. That branch is covered by decision tests
for waiting, motion exclusion and cancellation. Actual scuttle, claim, rebuild
and boarding remain covered by the unchanged physical return/impact fixtures.
The targeted natural failure is resolved by retaining the original landing.

The two fixed Pi pressure blocks remain seed 7/P1 with three-second Mixed
arrivals and an interceptor: pod stabilization stops progressing in one
reflection, and pod landing exhausts retries in the other. Other known failures
concern measured ground routes or unavailable replacement conditions. Ordinary
match outcomes and Pi rendering performance remain later integration work.

## Pi deployment and live check

Source was checkpointed before the final runners and image were built. Yocto
completed all 6,608 tasks, with 21 rerun, and the existing Ubuntu 26.04 host
validation warning. The archived image and extracted client identify the build:

```text
source commit: e37a042b7ccb70a0f5902bc5d26a99781bae7fdc
image SHA256:  3de2457ad82b6c1ae5025c8266a5886cce5b5fe792b26c25f1bf2d6f65d94577
client SHA256: e26ec09b4d6ff327a79cce37faa27feeb5515efaf46eb9c2b6edb93aa5252b54
```

The Pi boots slot B (`/dev/sda3`) with that exact client hash. The kiosk is
active and running with zero restarts and a zero exit status.

A live arena duel ran for three wall-clock minutes with three-second Mixed
asteroid arrivals and raster rendering at scale 2. Screenshots and status
samples at 30/75/120/180 seconds recorded 48.7/51.1/54.2/53.7 FPS and
59.6/59.9/60.2/60.7 simulation updates per second. Each capture finished about
one second after its requested time, and all four recorded zero service restarts.
P1 was on foot raising the first flag at 30 seconds, approached planet 2 by
75 seconds, and reached “patrol / planets secured” by 180 seconds. P2 was still
jumping along a ground route toward the enemy flag on planet 1. This verifies
the installed combined loop; it is not a completed ordinary match or evidence
that all ground routes succeed.

The final controller session is a fresh, paused `spacewars-terrain-arena`
round at UI revision 38: P1 human, P2 mission bot, eight-second Mixed
arrivals. B or Start resumes. Live and ready screenshots, settings, UI history,
final state and installation hashes are archived with the simulation reports.
This results update changes documentation only; installed source remains
`e37a042`. Merging remains deferred.
