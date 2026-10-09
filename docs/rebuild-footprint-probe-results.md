# Predicted foot support uses the wrong reference frame

**Follow-up:** the [support-alignment experiment](rebuild-support-alignment-results.md)
found a real two-foot landing despite a sampled alignment of 0.62749. That
counterexample disproves treating the sampled-normal test proposed below as a
necessary condition for actual settling. The original probe measurements remain
valid; its proposed hard gate is not qualified and stays disabled.

The failed coarse placement already predicts a foot normal that cannot satisfy
native landing support. Its old ray check accepts alignment **0.77485** against
the standing pilot's query direction, above its 0.65 threshold. The same sampled
normal has alignment **0.63595** against radial up at the predicted resting ship
origin. Native landing requires **0.7** for each foot. The overall ship-angle
check passes, but does not establish that both surface normals support the ship.

| Retained build | Sample normals versus query up | Sample normals versus resting ship up | Actual outcome |
| --- | --- | --- | --- |
| Handoff, 27114 | 0.99698 / 0.99600 | 0.99735 / 0.99555 | Settles and boards |
| Precise coarse, 25373 | 0.77485 / 0.98582 | 0.63595 / 0.99959 | One supported foot; scuttled |

The probe recomputes the existing two-ray floor and verifies that its raised
pose matches the actually inserted ship. Rotated foot bottoms differ from the
sampled floor points by about **0.00965** units in the failed case and less than
0.00018 in the handoff. Rotation alone therefore introduces a small geometric
offset here; the support-direction mismatch is present even at the sampled
points themselves.

The round feet also encounter the material at different distances. Sweeping
both actual foot colliders down from the coarse spawn gives first collisions at
**1.26745 / 1.51923** along the predicted normal, or **1.84076 / 1.47282** along
radial descent. In the handoff the corresponding gaps between first collisions
are below 0.00074. These are read-only geometric sweeps, not simulated contacts
or a prediction of the whole landing trajectory.

The retained simulation confirms the failed stage: the coarse ship never gets
two supported feet. At 25535, before the next terrain revision, one real contact
has radial alignment 0.33790 and the other foot has contacts above 0.99. The
sampled and actual bad normals differ, so the ray check cannot claim to predict
the exact solver contact. It nevertheless accepts sampled support that already
fails a necessary native condition. The later 15-second wait and three-second
scuttle remain as documented in the [arrival results](rebuild-precise-arrival-results.md).

The focused correction is to check both measured surface normals against the
existing native 0.7 support threshold in the predicted resting ship's radial
frame. This should reject this pose without changing landing physics or
boarding requirements. Rejection alone is not complete recovery; a separate
frozen comparison must establish whether relocation finds a usable alternative
while preserving the successful handoff.

The [plan](rebuild-footprint-probe-plan.md) froze source **246253a**, **997 input
hashes**, six changed inputs and a copied release executable. Two prefixes and
four continuations completed with **zero simulation retries and zero audit
resumes**. Both paths preserve their preceding **36 files byte for byte**.
After verifying runtime equality, the runner explicitly reuses the preceding
audit files. Those earlier auditors were not rerun. All new queries retain the
physics snapshot and native contacts, and their output never reaches the bot.

Passed: **183 Rust tests**, **982 Python tests**, bot Clippy with warnings denied,
formatting, locked Rust 1.89 release build and default-feature check. Native
Clippy retains the same 16 existing diagnostic identities. The bundle includes
full logs for the new validation batch; the Python and 63-test harness checks
retain completion receipts rather than full console logs.

The [manifest](data/rebuild-footprint-probe-v1.json) and
[portable bundle](data/rebuild-footprint-probe-v1.json.gz) include ray and collider
measurements, post-build contact witnesses, source, validation, exporter and
hashes linking the preceding evidence. Verified raw archives remain under
`target/rebuild-footprint-probe/v1/archives/`. No placement behavior or defaults
changed during this diagnostic experiment.
