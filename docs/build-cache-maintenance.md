# Build-cache maintenance

Local compiler outputs can accumulate many generations of test executables and
incremental compilation state. Keep these disposable caches distinct from
experiment evidence, which currently also lives under `target/`.

## First policy: native Cargo cleanup

Before a normal or fast Pi build, `yocto/scripts/build.mjs` checks this checkout's
host `target/debug` profile. It invokes Cargo's **package-scoped** clean if either:

- The Yocto build filesystem has less than **40 GiB available**, or
- This checkout's debug profile occupies more than **60 GiB**.

These are cleanup triggers, not reservations or guaranteed cache-size limits.
Cargo cleans all generations of this workspace's debug artifacts, retaining
third-party dependency artifacts. Remaining dependencies may themselves exceed
the budget. Cleaning can make the next local build slower; it does not rebuild
anything or affect the already deployed application.

This uses explicit `--package` arguments obtained from `cargo metadata`, so it
works with Rust 1.89 as well as newer Cargo. It never invokes unqualified
`cargo clean`, which would also erase experiment evidence. Package-scoped Cargo
cleanup takes Cargo's build-directory lock; active Cargo builds finish before
cleanup proceeds. Maintenance times out rather than waiting indefinitely.

Before deletion, a verbose Cargo dry-run must identify only paths inside the
local debug profile. Symlinked/relocated target trees, symlinks within that
profile, unexpected cross-target paths, and incomplete package selections are
rejected. Different cache/build filesystems are skipped. Cargo path overrides
are explicit, commands are offline/locked, and no cleanup touches a sibling
checkout. The target tree must not be manually moved or reconfigured during
maintenance.

An explicit pin, `target/.spacewars-cache-keep`, opts this checkout out. After an
applied cleanup, `target/build-cache-maintenance.json` records the selection,
Cargo summary, before/after usage, and completion time. Only the latest report
is retained. Reported allocated bytes deduplicate hard links; concurrent disk
activity can make the filesystem's free-space change differ from the cache size.

## Commands

```sh
# Read-only preview; does not remove artifacts.
npm --prefix yocto run cache:check

# Apply the policy now, using Cargo to remove workspace debug artifacts only.
npm --prefix yocto run cache:clean

# Optional one-off policy thresholds.
npm --prefix yocto run cache:check -- --min-free-gib 40 --max-profile-gib 60

# Builds normally apply the policy. These overrides preview or disable it.
SPACEWARS_BUILD_CACHE=report ./update.sh --fast --target sw-picade-2.local
SPACEWARS_BUILD_CACHE=off npm --prefix yocto run build
```

No system timer or background service is installed. Ordinary `cargo build` and
`cargo test` do not run this policy; Pi builds and the explicit commands above
do. Update `--dry-run`, `--skip-build`, and build `--help` do not trigger cleanup.
Yocto's existing 25/20/10 GiB warning/stop/halt thresholds remain in force; this
preflight never lowers them. A pin or lack of eligible artifacts can still leave
too little room, in which case more space must be made available separately.

## Deliberately protected

- Experiment directories, raw traces, archived regression cases, screenshots,
  and binaries copied outside Cargo's managed debug output paths.
- Release, CI, cross-compiled, and vendored-workspace profiles.
- Sibling checkouts and all shared Yocto downloads, sstate, work directories,
  images, deployment bundles, ROMs, settings, and backups.

This is not a general file-age janitor. An old file is not necessarily unused.
The next policy can manage completed experiments with explicit manifests,
verified archives, reference/pin tracking, and bounded retention. Existing
unclassified recordings must not be silently enrolled in it.

## Shared caches and retiring old cache roots

Cargo's downloaded crates and Git sources are already shared through
`CARGO_HOME` (normally `~/.cargo`), while each checkout keeps its own compiled
outputs under `target/`. This policy preserves that separation; it does not
merge build directories or configure a new compiler cache.

Yocto already shares downloads, sstate recipe outputs, and its Space-Wars
compiler cache between checkouts. `SPACEWARS_CACHE_ROOT` overrides the root;
otherwise `kas-spacewars.yml` selects `/home/data/linux1/yocto-cache` when
`/home/data/linux1` exists, or `/home/data/workspace/yocto-cache` as a fallback.
Separate roots may therefore remain after a machine's storage layout changes.
Yocto work directories and deployment images stay separate per checkout.

Do not infer that a second cache root is redundant from its age or directory
name. Before retiring one, inspect all consuming projects' configuration and
environment overrides, active build processes, and references such as symlinks
or Git alternates. Downloads can contain sources absent from the newer root;
matching filenames and sizes alone do not verify identical contents. Keep
unique sources until they are verified elsewhere. Cache retirement is a
separate, explicitly scoped operation, not part of the automatic preflight.

## Tests and upstream behavior

```sh
node --test yocto/tests/build-cache.test.mjs
RUSTUP_TOOLCHAIN=1.89.0 node --test yocto/tests/build-cache.test.mjs
```

The suite uses temporary directories only, including a tiny real Cargo workspace
to verify dependency reuse, evidence preservation, and build-lock contention.
It is also included in the existing `npm --prefix yocto test` CI job.

Cargo documents [package selection and dry-run cleanup](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).
Its [automatic global-cache cleanup](https://doc.rust-lang.org/cargo/reference/config.html#global-caches)
does not currently manage workspace build artifacts. Package-scoped locking is
implemented by Cargo's [clean operation](https://github.com/rust-lang/cargo/blob/rust-1.89.0/src/cargo/ops/cargo_clean.rs)
and [build layout](https://github.com/rust-lang/cargo/blob/rust-1.89.0/src/cargo/core/compiler/layout.rs).
