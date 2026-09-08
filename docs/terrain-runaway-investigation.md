# Terrain runaway investigation — 2026-09-07

The recorded fragment runaway starts in the planet gravity calculation. An
inverse-square point source remains at the planet center after excavation makes
the interior accessible. A fragment's center of mass can approach that source
closely enough to receive an enormous velocity change in one tick. Collision
solving then transfers that motion into other bodies and rapid spin. Continuous
collision detection (CCD) accounts for most of the resulting physics cost.

During this investigation, gameplay source and the deployed playtest build
were left unchanged. A bounded interior gravity prototype was evaluated in an
isolated copy of the working tree. The subsequent implementation and validation
are described in the [terrain endurance report](terrain-endurance.md).

## Direct evidence

The existing seed-42 `fragments` workload cuts the planet into a grid at tick
300 (5 seconds). The 88 initial detached pieces begin with at most 2.019 units/s
of speed and 0.035 rad/s of spin, inherited from the parent. The split itself
does not create the extreme motion in this reproduction.

At tick 321, starting at 5.35 seconds, fragment `1125899906842671` has its center
of mass 0.766 world units from the planet center:

| Stage | Speed (world units/s) | Spin (rad/s) |
| --- | ---: | ---: |
| Before edits | 146.82 | -12.92 |
| After edits | 146.82 | -12.92 |
| Before gravity | 146.82 | -12.92 |
| After gravity | 72,152.84 | -12.92 |
| After physics | 15,625.27 | 223.33 |

Its mass (121) and rotational inertia (2440.17) stay unchanged across these
stages. At tick 327 another fragment is only 0.0601 units from the center:
gravity raises its speed from 27,080 to 11,749,426 units/s; physics then produces
479,360 rad/s of spin. The one-second endurance samples miss this larger peak.

The late excavation case has the same mechanism. At tick 6011 (100.183 seconds),
fragment `1125899906842626` is 0.172 units from the center. Its speed rises from
14,925 to 1,432,605 units/s during gravity, with no preceding edit-stage change
to its motion, mass, or inertia. The physics stage then increases its spin from
-79 to 7,031 rad/s.

Both unchanged diagnostic runs matched **every original recorded one-second
body-motion hash** over their respective 7- and 101-second intervals. Adding
the stage observers and CCD timers preserved the recorded motion.

## Controlled comparisons

All short runs use the same seed-42 grid-cut workload at 60 Hz. Maximum speeds
below include all rigid bodies at each of six observed stages per tick.

| Gravity policy | Peak speed | Total physics time (ms) | Worst physics step (ms) | CCD share of physics time |
| --- | ---: | ---: | ---: | ---: |
| Current point-source gravity | 11,749,426 | 4539.65 | 73.91 | 94.4% |
| Gravity disabled for terrain fragments | 64,003 | 92.18 | 1.94 | 66.2% |
| Planet gravity disabled for everyone | 861 | 52.11 | 0.68 | 59.6% |
| Bounded interior gravity for everyone | 824 | 123.41 | 2.52 | 30.6% |

Fragment-only protection leaves other bodies exposed. At tick 394, rover body
`50000`, role `22`, receives a gravity-stage speed of 64,003 units/s at distance
0.815 from the center. This transient is also missed by the one-second audit.
The eventual fix needs to describe the planet's field consistently for all
recipients, including rover frame compensation, ships, debris, and particles.

The prototype retains inverse-square gravity at and beyond the nominal planet
radius. Inside that radius, pull decreases linearly to zero at the center and
matches the exterior force continuously at the surface. It changes the force
law; it does not clamp body speed, delete fragments, or change collision shapes.
The planet's original source mass and prescribed motion remain in place.

## Longer prototype runs

The bounded-interior prototype completed all four seed-42 workloads for 180
simulated seconds each on the desktop. Every one-second health audit passed,
and remaining plus removed material matched the initial amount at every audit.
No world-crossing speed anomaly appeared at any observed stage of any tick.

| Workload | Peak speed across all stages | Physics P95 (ms) | Worst physics step (ms) |
| --- | ---: | ---: | ---: |
| Cannon | 914.66 | 0.288 | 0.787 |
| Excavation | 824.32 | 2.207 | 4.419 |
| Fragments | 824.32 | 2.278 | 3.460 |
| Multi-planet | 981.55 | 8.405 | 14.581 |

These are physics timings from the diagnostic binary, not full-client frame
times. They exclude rendering, trace writing, audits, and workload scheduling.
The modified gravity changes subsequent contacts and shell targets, so long-run
physics populations are not identical to the original runs. For example, the
bounded excavation run reaches 66 sampled fragments; the failing original run
has 29 near its slowdown. The strongest causal evidence is the stage trace and
the short controlled comparisons, not a ratio of unequal long-run timings.

The long prototype checks cover one seed per workload on one desktop. Other
seeds, Pi performance, controller feel, and timestep policies remain to be
validated during implementation. Finite speeds and conserved material do not
prove every remaining collision or spin is physically plausible.

## Additional diagnostic finding

The Rapier 0.34 aggregate `stages.ccd_time` counter currently read by
`PhysicsWorld::step` remains zero: the relevant CCD paths do not start that
timer. Its separate time-of-impact counter also does not encompass every CCD
operation. The investigation added timers around the two CCD sections in an
isolated copy of Rapier to measure their actual cost. Those timers preserve
the original sampled body-motion hashes.

System `perf` sampling was unavailable because the host restricts performance
events. No host permissions were changed. The CCD attribution comes from the
explicit section timers, not from a sampling profile.

## Recommended implementation scope

1. Represent a finite-radius planet field in the shared gravity code while
   retaining existing point-source behavior for callers that request it. Apply
   the same planet field to every recipient. Preserve the exterior force law.
2. Add regressions for a center crossing, finite and continuous interior force,
   unchanged exterior behavior, and the short grid-cut reproduction including
   its rover. Keep material and fragment motion checks.
3. Add a cheap per-tick anomaly trigger with a short diagnostic history, so a
   brief pre-collision spike can be preserved without full CSV tracing during
   normal runs. Make CCD timing reporting accurate or explicitly unavailable.
4. Rerun the established multi-seed desktop and Pi endurance matrix, then
   controller-playtest movement and mining inside damaged planets.

How source mass, center, and motion should change as material is removed remains
a separate gameplay/model decision. The prototype does not implement gravity
from detached fragments or remove the original gravity source when terrain is
gone.

## Evidence archive

The local `terrain-endurance-20260907/runaway-investigation` artifact directory
contains `findings.json`, per-stage body CSVs, contact CSVs, per-tick physics
timings, original-hash verification, and a comparison plot. Its `README.md`
documents the isolated source archive and reproduction commands. The nine
final runs are the four `fragments-*-7s` controls, `excavation-baseline-101s`,
and four `*-bounded-interior-180s` workloads. Earlier exploratory outputs are
retained separately and are not the measurements in the tables above.
