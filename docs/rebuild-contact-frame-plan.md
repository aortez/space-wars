# Compare native rebuild placement in the completed planet frame

The holding experiment regressed both retained cases. Its traces show a roughly
0.4-unit disagreement between a native build point and the nearby preview.
The solver's world contact predates planet integration. Claims already use the
contact's local point; rebuild placement still uses the stale world point and
normal. Test that native correction with holding disabled.

Freeze four modes before execution: the preceding `control_walk` and
`control_handoff`, plus one contact-frame candidate for each. The candidate
transforms the selected support's local point and normal through the current
Rapier planet pose. Existing support, balance, speed and ownership checks stay
unchanged, as do construction time, offset search, landing and hatch guards.
The switch is off by default and enabled only after the unchanged prefix.

Each mode replays the original prefix and forks recorded and live controls.
Keep original task birth 16820, completed crossing 23226, activation 23767,
ground allowance 5400, four-relocation limit and fixed end 29421. Keep staging,
map-handoff and search limits exactly as in each matched control. P1 follows
the original tape. No fresh games or adaptive candidate tuning in this run.

Record selected support in solver, local and completed planet frames in a
separate diagnostic stream for both forks of all four modes. It never enters
bot observations. Verify its point and normal against the native candidate;
compare fresh placement reports with the same-frame selected support when
contact remains available. Record missing contact after native ship insertion
without inventing a measurement. Each control must retain all 26 preceding
files byte for byte, proving that diagnostics do not change those trajectories.

Freeze all source hashes, four commands and one release binary. Require exact
prefixes, recorded opponent controls and clean native world audits. Record
builds, damage, supported settling, actual boarding and termination in both
forks. Judge each candidate against its own control and report regressions.
Full-chain success needs native rebuilding and settled boarding with a living
pilot and no additional ship loss. These are two paths in one retained case;
they do not establish full-match or frontier qualification.

Run focused native, bot and harness tests, Python audits, formatting, locked
Rust 1.89 release/default builds and Clippy, retaining existing native diagnostic
failures separately. Archive every raw file losslessly and export the evidence,
source provenance and validation logs. Preserve all negative outcomes. Partial
simulations cannot be retried; any audit-only repair must reuse hash-verified
raw outputs and be disclosed. No default change, push, PR, merge or deployment.
