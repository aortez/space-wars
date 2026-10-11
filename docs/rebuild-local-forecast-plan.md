# Test a preconstruction forecast with limited state and resumable work

Use the exact full-world native forecasts as references for a smaller model.
Capture each selected replacement pose inside native recovery, before inserting
the real ship. Construct an independent hypothetical replacement using the same
spawn helper, hull, round feet and origin/COM velocity correction. The diagnostic
does not delay construction or decide whether it is allowed.

Copy only the selected planet's fixed/kinematic body and solid colliders into a
fresh Rapier world. Share immutable geometry, copy no contacts or solver history,
and reject requests exceeding 128 planet colliders or 16,384 shape parts/vertices.
Support balls, cuboids, convex polygons and bounded compounds; reject unsupported
meshes. Add the hypothetical ship, yielding two physical bodies. Copy physical
gravity and prescribed-motion inputs for at most 32 planets plus the sun. Use the
native gravity solver, source order, orbital recurrence, landing assist, collision
response and earned two-foot settling gate. Controls remain neutral and wings
remain open. No other actors, debris, RNG, damage, hazards or terrain evolution
enter the model. The whole selected planet's geometry is privileged information;
this is not yet an observation-limited production sensor.

Run 120 steps of 16,666,667 ns, at most four per `advance` call, including when a
caller requests more. Zero budget and completed jobs do no work. In the replay
harness, advance once per live tick, so completion normally spans 30 frames.
Continue after settling. Stop inconclusively if the ship moves more than 16 local
units from its initial origin or another planet wins the approach-distance test.
Any scope failure invalidates even an earlier positive prediction. A completed
negative means only not settled in this two-second model. Results never authorize
construction, transfer, holding a site or a bot-default change.

Measure setup, physics work, sample generation and every chunk's elapsed time.
Report total computation and maximum chunk separately. These are single
workstation samples, not a hard wall-time bound or Raspberry Pi measurements.
Test preconstruction provenance, native fixture agreement, independence of one-
versus four-step scheduling, budget clamping, source retention and fail-closed
scope/geometry limits before freezing the retained comparison.

Freeze committed source hashes, commands and a copied Rust 1.89 release binary.
Run the two existing controls once: two prefixes, four continuations, three known
builds. Preserve the support gate off and existing round-foot probes. Disable the
old synchronous native forecast flag because its diagnostic files contain wall
timings. Require every other old raw file to match byte for byte; then restore
the old native forecast files and audits from their verified archives. Verify
all 44 handoff / 42 coarse archive files before explicitly reusing prior audits.
Never describe those restored timed forecasts as fresh measurements.

Compare initial ship/planet poses and reports, every projected motion/contact row,
first contact, two-foot support and settling against both archived full-world
forecasts and actual windows. Retain all differences, misses and unavailable
results. Do not retry simulations, adjust the horizon, change inputs or tune to
these outcomes. Auditor-only repairs may reuse verified raw data if disclosed.
These known cases can demonstrate preservation, not held-out qualification or
success of a search across alternative offsets.

Run engine tests, native rebuild, bot ground/recovery, harness and Python tests,
formatting, locked release/default checks and Clippy with the existing native
baseline. Export the frozen sources, old reference provenance, new traces,
comparisons, costs, validation logs and archive receipts in a portable bundle.
