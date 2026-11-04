# Spacewars TUF client patch

Upstream: [`awslabs/tough` 0.24.0](https://github.com/awslabs/tough), source
commit `98d8eb8b2ce63515d9b4981c938ef6453c5b5771` (the `tough` subdirectory).
The published crate archive SHA-256 is
`35b378d98765c2ae9cdc3e9963ea7e670da8cdd9ee39611b8d722083c7f1ac11`.
The original crate manifest, source, tests, and MIT/Apache licenses are retained.

The local changes are limited to:

- `RepositoryLoader::clock(Arc<dyn Clock>)` supplies an already admitted device
  clock to both expiration validation and the datastore's backwards-time guard.
  The upstream system clock remains the default for other consumers. Device
  verification explicitly injects time; fixtures never change/read the host's
  wall clock. Expiration enforcement stays enabled.
- `check_expired` rejects equality with the expiration instant. TUF requires
  expiration to be strictly later than the update start, and upstream target
  reading already uses this strict comparison.

Signature algorithms, canonical signatures, thresholds, delegation verification,
root rotation, and metadata rollback checks remain upstream implementations.
The `engine-ota` wrapper commits its complete persistent trust state atomically;
the upstream datastore is a private working copy during verification.

Run `cargo test --locked -p engine-ota` for the integration qualification of
these changes, including exact expiry, injected backwards time, signature
thresholds, root rotation, and retained verification across reopen. The complete
upstream test suite is retained but has not been claimed as part of that check.

Recheck this small delta when upgrading the pinned dependency, and remove it
when equivalent clock injection and expiry behavior are available upstream.
