# Local Rapier 0.34.0 correction

This is the published `rapier2d` 0.34.0 crate. Its crates.io archive SHA-256 is
`691626cdcb70312199acd86322b57ddabea6da4f4399b4b1ddad0f2ff140dd71`.
The workspace pins this version and uses a Cargo path patch. Dependencies and
features otherwise retain the workspace lockfile's versions.

Only `src/dynamics/solver/velocity_solver.rs` differs from the published Rust
source: retain the interpolated velocity of kinematic bodies even when their
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

Upstream metadata identifies commit
`a1ef31035613154dfb97a9e1d480c6a5eb9d0010`, with a dirty publishing tree; the
published archive is the source of truth for this copy. The Apache-2.0 `LICENSE`
was added from that commit's repository root, where the package omitted it.
Its SHA-256 is
`ceacfa4d7fa67df64ab09a56fe248c50a3bfc9bc00374d69bd74ad763b14b89c`.
The registry cache marker `.cargo-ok` is omitted.
