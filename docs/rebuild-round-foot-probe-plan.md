# Predict first contacts with the actual round feet

The rejected support-alignment gate uses thin-ray normals. Its failed live
coarse build has minimum sampled alignment 0.63595, while a genuinely settled
recorded handoff has a lower minimum, 0.62749. A scalar cutoff cannot distinguish
them. Compare the round feet's first contacts before proposing another gate.

Add a read-only engine ball sweep that reports the struck collider's world-space
point and outward normal, travel distance, and convergence/overlap status. Test
a corner that a center ray misses, translated and rotated geometry, compound
children, collision filters, sensors, exclusions, invalid inputs, initial overlap,
and unchanged physics. Use the real 0.45-unit foot radius at each inserted ship's
actual rotated foot center. Sweep four units from spawn, independently along the
predicted ship normal and radial down. Retain the preceding ray/footprint probe
inside each new diagnostic and keep non-planet obstructions visible.

Predeclare the simple prediction: both first-contact normals must align at least
0.7 with radial up at the corresponding ship origin at impact. Missing hits,
non-retained surfaces, initial overlap and unconverged results are inconclusive.
Use the existing native threshold without tuning. Record unequal collision
distances: independent straight sweeps do not represent simultaneous contacts
or account for rotation, sliding, force balance or terrain motion. Compare both
direction variants with actual native settling, retaining every false result.
Even complete agreement on three known builds is diagnostic evidence, not a
validated settling gate or a complete recovery result.

Freeze committed inputs, commands and a copied Rust 1.89 release binary. Run
only the preceding support experiment's two controls with that rejected gate
off. Keep all original history and bounds: two prefixes, four continuations,
three expected builds (live handoff, live coarse, recorded handoff). The recorded
coarse fork must still build nothing. Probe both live and recorded builds.
Require all preceding raw files to match byte for byte before explicitly reusing
their existing audits; retain all 38 preceding archive files per control.

Do not retry or retune simulations. Auditor-only repairs may reuse hash-verified
raw results without changing runtime or frozen plan inputs. Export all probes,
contact witnesses, predicted versus actual outcomes, frozen sources and checks
in a verified portable bundle. Run engine query tests, native rebuild tests, bot
recovery/ground tests, harness and Python suites, formatting, locked release and
default checks, bot Clippy and the native baseline comparison. Any new engine
Clippy diagnostics must also be resolved. No placement behavior, physics,
boarding condition, bot default or frontier choice changes in this experiment.
