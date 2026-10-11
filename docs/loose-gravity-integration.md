# Gravity integration for loose Space Wars material

A pile in the rotating roof fixture kept reporting moving grains long after the
collapse. This was primarily contact jitter: many grains stayed within a small
fraction of a cell while their velocities repeatedly pointed out of the pile.
A stationary-cave diagnostic reproduced the problem, so rotation alone was not
the cause.

Space Wars supplies gravity as a velocity change per fixed tick. Applying the
whole change as an impulse before Rapier's solve overloaded the early solver
substeps. Changing only the rubble helped, but the ships resting on it continued
to shake the pile with their own gravity impulses.

## Integration

When a loose-material pool is enabled, all gravity-driven rigid bodies use the
existing `PhysicsWorld::apply_acceleration` binding. The adapter divides the
sampled velocity change by the tick duration, preserving its integrated amount
while Rapier distributes the force across solver substeps. Ships, spacelings,
rover bodies, debris, detached clumps and individual grains use the same path.
Rover frame compensation and debris's existing sampling cadence are retained.
Visual particles retain their existing update.

The scenario clears applied forces once before each gravity/control pass. This
prevents accumulation and removes an old force when a source's output becomes
zero. Blasts, weapon recoil and jump impulses remain instantaneous. Worlds
without a loose-material pool retain the previous gravity integration. The mode
is derived from the pool at each gravity boundary, including in cloned worlds.

The change is in the Space Wars adapter to the canonical shared physics world.
Scorched Earth already supplies continuous gravity to Rapier. There is no second
solver, extra scenario step, added damping, speed clamp, forced sleep, altered
quiet threshold, or loosened packing/clearance rule. Gravity strength, material
strength and the prescribed planet motion are unchanged. Collisions and impact
chipping can follow different trajectories because gravity is integrated
differently within the tick.

## Verification

All **837 tests** pass across engine-terrain, engine-rapier, Scorched Earth,
Space Wars and Terrain Lab. The cave regression now also requires over 90% of
the late grain-motion samples to be quiet relative to the rotating planet, for
both shapes. Another regression checks that free-fall gravity integrates exactly
once per tick, does not accumulate, and disappears when its source stops.

The asteroid lifecycle test tracks peak loose material and verifies capacity
throughout the run. It permits all rubble to have repacked at the endpoint;
previously it required leftover rubble. Existing conservation, release, impact,
actor support, replay and world-clone checks remain in place. Core/Scorched
strict Clippy passes; Space Wars retains its 17 existing warning locations.

## Results

In the 30-second Weak roof encounter, one-second samples from seconds 21–30
classified **86.6% → 0.0%** of Round grains and **89.6% → 1.9%** of Angular grains
as moving. At the final checkpoint both shapes have zero grains classified as
moving, versus 66 Round / 67 Angular before.

The Weak roof pile still has **zero deposits** at 30 seconds with either shape.
Its remaining reasons are blocked placement, repose stability, available space
and quiet-time waiting. The next packing investigation can now inspect those
quiet groups without treating persistent velocity noise as physical rolling.
The Off-strength roof, which has no detached roof clumps or repose rule, returns
32 Round / 16 Angular cells instead of zero; these are different workloads, not
a reason to remove the Weak preset's stability checks.

The benchmark matrix compares both shapes in the existing 120-second Scorched
barrage, hillside, quarry, curved planet, default and combined Space Wars runs,
plus both games' Off/Weak/Strong roof cases. Timings exclude diagnostic scans,
replay and rendering. Physics changes can alter the amount of later work, so
these timings are workload comparisons, not a solver-only speedup claim.

The complete matrix ran **88 benchmark executions / 128 scene results** across
both hosts. All 64 corresponding scene results and every emitted diagnostic
checkpoint match across architectures. All 28 expected unchanged before/after
scene comparisons match: Scorched Earth and Space Wars with loose dirt disabled.
There were 64 benchmark executions with per-second replay checks.

Pi 4 whole-scenario updates, mean / p95 / maximum in milliseconds:

| Weak workload / shape | Before | After |
| --- | --- | --- |
| Roof / Round | 6.205 / 7.501 / 22.034 | 6.256 / 9.507 / 21.442 |
| Roof / Angular | 6.909 / 8.321 / 25.996 | 6.985 / 9.854 / 23.587 |
| Quarry / Round | 3.870 / 5.044 / 22.021 | 4.078 / 5.718 / 23.035 |
| Quarry / Angular | 4.472 / 5.699 / 21.168 | 4.928 / 7.159 / 21.213 |
| Curved / Round | 6.422 / 9.953 / 21.549 | 6.238 / 9.805 / 36.039 |
| Curved / Angular | 8.205 / 11.639 / 23.037 | 8.209 / 15.812 / 31.834 |

These are single runs. Desktop timing overlapped builds and other workspace
work; the Pi kiosk was paused during headless timing. The newer trajectories
can do more packing work: the Round curved case returns 59 cells, versus one
before. Its tick 293 costs 35.1 ms in a separate Pi profile, including 29.9 ms
in packing and 0.86 ms in physics. The matching desktop profile locates the same
spike. Profiling is separate from the table's uninstrumented timing, and profile
spans are inclusive. Reducing this packing spike is still needed for a consistent
16.7 ms update budget; this pass makes no rendered-60-FPS claim.

Reproduce the affected case:

```sh
cargo +1.89.0 run --locked --release -p scenario-spacewars \
  --example destruction_benchmark -- --roof --strength weak --shape angular \
  --seconds 30 --diagnostics --replay --require-roof
```

Use `--shape round` for the other grain proxy. The regular Space Wars launcher
uses the same integration when loose dirt is enabled; the cave fixture remains
a headless development scene.

[Measurements and build identities](data/dirt-gravity-20261008.json) ·
[per-second diagnostics](data/dirt-gravity-traces-20261008.jsonl.gz) ·
[packing profiles](data/dirt-gravity-profiles-20261008.jsonl.gz).

## Review builds

The rebuilt client is deployed to `sw-picade.local`; installed client/CLI hashes
match the fast-update bundle, and the kiosk is active with no restarts or UI
error. It is left paused in Space Wars. The local window is a fresh paused
seed-42 material combat duel with Weak/Angular dirt and heavy asteroid arrivals
every second. Resume to watch; the launcher settings can select the other shape.

The branch remains uncommitted while the behavior is being reviewed.

The [packing preview follow-up](packing-preview-reuse.md) investigates the
remaining cave obstructions and reduces the cost of the large curved-pile
query while preserving this pass's simulation outcomes.
