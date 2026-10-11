# Use native foot-support alignment in rebuild placement

The frozen footprint probe identifies a necessary-condition mismatch. At native
build 25373, one predicted foot normal passes the old 0.65 check against the
pilot's query direction with alignment 0.77485, but has alignment 0.63595 against
radial up at the predicted resting ship origin. Native support requires 0.7.
The other foot passes, and the overall ship angle is below 20 degrees. The
actual ship never earns two supported feet and is eventually scuttled.

Add a default-off replay switch. After the existing ship-angle check, evaluate
both sampled material normals against radial up at the predicted resting ship
origin. Share the unchanged native 0.7 threshold and reject failure as
`landing_unsupported`. Publish the two alignments only when the switch is on.
The shared placement function applies this condition to both relocation previews
and native rebuild attempts, including refined offsets.

Keep query directions, offsets, spawn poses, hull/hatch checks, native support
and boarding, construction timing, holding, precise arrival, deadlines and
relocation counts unchanged. Sampled normals are only a necessary condition;
they do not predict all round-foot contacts or guarantee physical settling.

Freeze source, inputs, exact commands and a copied release binary before four
original prefixes and eight continuations. Both controls use the preceding
precise-arrival paths, with footprint probes removed. Each candidate differs
only in the support-alignment switch. Activate after the unchanged 23767 prefix.
Retain all 36 preceding non-probe files for each control byte for byte. Preserve
the successful handoff and the failed coarse case; no seed search or retries.

Re-run existing native, search, ground, holding, contact, orientation and arrival
audits. Check every published alignment and unsupported rejection. Compare first
changes in reports, actions and native physics. Recorded forks keep recorded
actions, but native placement is deliberately allowed to change with the new
gate; report any resulting physical difference rather than assuming retention.

Full qualification requires both live candidates to build, physically settle,
board and finish recovery alive with no additional ship loss. A rejected pose
or a preserved successful control alone does not qualify. Keep every failure and
all raw evidence. An auditor-only repair may reuse verified raw output without
changing the frozen runtime. No fresh games, default promotion or deployment.

Validate the retained bad and good support frames under rotation/translation,
both-foot threshold boundaries, valid read-only placement, cloning and dirty
query handling. Run native rebuild, bot recovery/ground, harness and Python
tests, formatting, locked Rust 1.89 release/default checks, bot Clippy and the
existing native Clippy baseline comparison. Export source, validation, report
and contact witnesses, comparisons and hash-verified archives.
