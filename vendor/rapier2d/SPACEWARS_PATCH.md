# Local Rapier 0.34.0 corrections

This is the published `rapier2d` 0.34.0 crate. Its crates.io archive SHA-256 is
`691626cdcb70312199acd86322b57ddabea6da4f4399b4b1ddad0f2ff140dd71`.
The workspace pins this version and uses a Cargo path patch. Dependencies and
features otherwise retain the workspace lockfile's versions.

`src/dynamics/solver/velocity_solver.rs` retains the interpolated velocity of kinematic bodies even when their
own CCD is disabled. `TOIEntry::try_from_colliders` needs that velocity to reject
pairs moving too slowly relative to each other to require a CCD sweep. Zeroing
it mistakes shared transport for closing speed. A small passenger can then be
clamped against ground at its prescribed future pose and become embedded.

This does not enable CCD on kinematic bodies, alter their prescribed endpoints,
replace the CCD sweep or change solver settings. The existing adapter correction
for position-based kinematics during CCD subdivisions remains necessary.

The workspace regressions in `crates/engine-rapier/src/world/tests/compound.rs`
cover a small passenger moving with tiled ground, both kinematic modes, separate
and compound colliders, half/full walking speeds and one/four CCD substeps. A
second regression requires real impact events and velocity response when a fast
ball catches a moving wall. Run them with:

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p engine-rapier \
  world::tests::compound::ccd_
```

See `docs/moving-ground-ccd.md` for the gameplay reproduction, rejected sweep
experiment, acceptance results and remaining limitations. Remove this vendor
copy and Cargo patch only after an upstream version passes these checks.

Two small backend extensions support the shared compound-sweep optimization
described in `docs/compound-ccd-pruning.md`:

- `src/geometry/narrow_phase.rs` adds `set_query_dispatcher`. Snapshot loading
  restores the engine's stateless dispatcher without dropping Rapier's contact
  graph or manifold workspaces. That dispatcher delegates all persistent contact
  queries to the original implementation, so the workspaces remain compatible.
- `src/pipeline/physics_pipeline.rs` and `src/counters/ccd_counters.rs` time both
  first-impact searches and motion clamping, accumulating across all CCD
  substeps. The original aggregate CCD timer was unused; its clamping timer
  restarted on every substep. These counters are diagnostics, never simulation
  inputs. No CCD activation, stepping, solver or motion-clamping logic changes.

The pruning algorithm is in the engine adapter. The late-match CCD experiment
also changes `src/dynamics/ccd/ccd_solver.rs` and `toi_entry.rs` to reuse compound
shape thickness across a complete `PhysicsPipeline::step`. After applying user
changes, the pipeline shares one local cache across its first-impact searches,
motion clamping and CCD substeps. Poses change during the step, but collider
geometry does not; the cache is discarded before callers can edit shapes or
remove/reuse handles. Primitive shapes retain direct property queries. The
original public `CCDSolver` methods still allocate a per-call cache, and
`TOIEntry::try_from_colliders` still computes its own thickness. Their public
signatures and the serialized unit `CCDSolver` are unchanged. The same thickness
sum, comparison, cast order and response are retained; no cache enters snapshots
or persistent solver state. Private re-exports in `src/dynamics/mod.rs` and
`src/dynamics/ccd/mod.rs` expose the cache only to the pipeline.

The compound regressions also cover first-impact/clamping reuse with multiple
CCD substeps, reset between steps, in-place shape edits, compound/primitive
transitions, reused collider arena slots and exact snapshot continuation. Run
the feature-gated work-count assertion with `--features ccd-diagnostics`; it
fails against a per-pass cache (five compound evaluations instead of one).

The opt-in `ccd-diagnostics` Cargo feature enables `src/counters/ccd_probe.rs`
through `src/counters/mod.rs`, with counters around the existing TOI prefilter.
The engine adapter separately counts shape casts and compound tree visits.
These clocks and counters are absent from normal builds. Instrumented timings
include their overhead; comparison timings disable collection.

The residual-cost follow-up extends those opt-in counters to CCD setup/search,
swept bounds, BVH updates, candidate traversal, existing-contact lookups and
compound-thickness cache misses. The counters themselves do not change CCD
filtering, cached values, shape-cast ordering or response. See the GPU-default
follow-up in the same investigation document; its timings include diagnostic
overhead and are not a new optimization result.

The Pizza cascade investigation also defers the existing-contact lookup in
`ccd_solver.rs` until `toi_entry.rs` has checked relative travel against shape
thickness alone. The original rejection threshold adds
`smallest_contact_dist.max(0.0)` to that thickness, so a candidate rejected by the
lower threshold must also fail the original test. Surviving candidates retain
the original addition, comparison, cast order and response. The public
`TOIEntry::try_from_colliders` signature is unchanged; only its private helper
takes a lazy contact-distance closure. No contact cache or new persistent state
is introduced. This applies to first-impact searches, clamping and resweeps.

With diagnostics enabled, contact-lookup time now nests inside TOI time; do not
sum those inclusive timers. The feature-gated
`ccd_shared_motion_rejects_before_contact_graph_lookup` regression exercises
fast bodies travelling together with one/four CCD substeps. Existing moving
ground, impact, compound, snapshot and filtering regressions cover the remaining
CCD paths. See `docs/pizza-reference-comparison.md` for measured work and replay
equivalence; instrumentation timings are separate from uninstrumented runs.

The #327 contact-batch experiment adds a default no-op
`PhysicsHooks::modify_solver_contacts_batch` in `src/pipeline/physics_hooks.rs`,
re-exported by `src/pipeline/mod.rs`. `physics_pipeline.rs` invokes it once per
actual solver substep, after kinematic velocity interpolation and before island
construction/velocity solving. The context reads the complete retained contact
set and can set bounded restitution on an existing manifold through a private,
generation-checked `NarrowPhase::contact_pair_mut` accessor. Existing local contact modifiers run first;
their tangent commands are preserved. The default does no work and introduces
no persistent state or snapshot changes. External forces are not yet integrated
at this phase. See `docs/design/material-contact-batch.md` and the real-contact
regressions in `crates/engine-material/tests/contact_batch.rs` for scope and
remaining adapter work.

The shared-energy follow-up adds the 2D-only `ContactImpulse` and checked
`SolverContactBatchContext::apply_contact_impulses` in
`src/pipeline/normal_response.rs`. The context now holds the mutable body set
privately and exposes read-only `bodies()`. The operation stages bounded velocity
records, rejects the whole request on invalid/stale/duplicate targets or
remaining approach, then applies paired normal impulses and wakes dynamic bodies.
It admits only touching frictionless contacts without tangent drives. It clears
targeted restitution and warmstarts, including tracked contact data, to avoid
reapplying the old response; contact topology and solver constraints stay active.
Default hook behavior, global solver settings and snapshot schemas are unchanged.
Injected impulses are not reported as part of the subsequent solver force events.
The host must retain its own work records and prove complete contact/constraint
admission. `tests/shared_response.rs` covers actual default-solver energy,
momentum, reversed insertion, moving supports, timestep changes, stale warmstarts
and atomic rejection. See `docs/design/material-shared-energy.md` for the pulse
allocation hypothesis and remaining production limits.

These twelve Rust files and the diagnostic feature in `Cargo.toml` are the
complete source/configuration differences from the published crate. See
`docs/picade-late-match-physics.md` for the retained inputs and qualification.

Upstream metadata identifies commit
`a1ef31035613154dfb97a9e1d480c6a5eb9d0010`, with a dirty publishing tree; the
published archive is the source of truth for this copy. The Apache-2.0 `LICENSE`
was added from that commit's repository root, where the package omitted it.
Its SHA-256 is
`ceacfa4d7fa67df64ab09a56fe248c50a3bfc9bc00374d69bd74ad763b14b89c`.
The registry cache marker `.cargo-ok` is omitted.
