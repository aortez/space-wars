# Compare future-start settling forecasts before native construction

Test a default-off native placement selector on the two retained recovery paths.
The preceding two-body model preserved all three settling classifications, but
only observed candidates; this experiment may change actual construction. It
does not qualify a production bot sensor or change defaults.

After the original eight-second build interval and existing recovery conditions
are satisfied, check the ordinary native placement. If none is geometrically
valid, preserve its failure. Otherwise forecast the current preferred offset
first, then the remaining four base and 22 existing refined offsets in their
existing order, without duplicates. Include refined alternatives even when the
base offset was geometrically accepted. No new offsets or query directions.

Each candidate targets construction **40 ticks after capture**. Advance only the
copied planet and physical gravity ephemerides for 40 projected ticks, insert the
hypothetical ship in that future planet frame, then project its existing
120-step settling horizon. Perform at most four total warmup/ship physics steps
per native update. This finishes 160 steps on the scheduled construction tick.
Query at most one new alternative per update. The initial native selection and
fresh ground-map/placement queries retain their existing finite bounds; record
physics work separately from those queries. Do not change support rules, landing
assist, ship geometry, construction progress, task deadlines or relocation limits.

Keep each search's query anchor fixed in the planet frame. Cancel on lost native
recovery conditions, changed planet or terrain revision, dirty queries, or pilot
footing more than one unit from the anchor. Fresh hull/occupancy and hatch-route
queries use the current world and actual pilot footing. Publish the optional
anchor in placement reports so offsets are not misrepresented as relative to the
current pilot. A pending forecast leaves native status `rebuilding`; it does not
count as a failed placement or reset a task clock.

Only a completed positive forecast may build. Re-run all placement checks on
that single anchored offset, including the current hatch route and other actors.
Require the current tick to equal the forecast's scheduled launch, unchanged
physical source parameters and predicted orbital phases, current terrain
revision, and agreement with the forecast's launch frame. Reject local center
differences above 0.002 units, local normal differences above 0.0001, planet
position differences above 0.002, wrapped angle differences above 0.00001 radians,
origin-velocity differences above 0.02 and spin differences above 0.001.
The fresh pose is used for the actual build. Failed, unavailable, invalidated or
stale forecasts never authorize construction; continue the bounded alternatives
or expose ordinary placement failure after exhaustion. Do not treat failure to
settle within two seconds as physical impossibility forever.

Trace every search, query, work slice, forecast result, cancellation, revalidation
and accepted build. Compare accepted forecasts with actual first contact, two
supported feet, settling, boarding, health and loss counts. Preserve failures and
small trajectory differences. Forecasts remain conditional on unchanged terrain
and absent interference; actual gameplay determines success.

Freeze committed inputs and a copied Rust 1.89 release executable before four
prefixes and eight continuations: `control_handoff`, `control_coarse`,
`selection_handoff`, `selection_coarse`. Each pair differs only in the selector
flag. Disable the old timed local-forecast probe in all four modes. Require all
other control raw files to match the preceding run; restore timed forecasts and
audits from verified archives, retaining all 47 handoff / 44 coarse files before
reusing them. Both candidate forks retain their original action policies; the
recorded fork's physical outcome may change with native construction.

Audit prefix/task identity, unchanged action sources and bounds, search/arrival/
holding/contact behavior, anchored query directions, forecast scheduling and
every accepted native build. Full qualification requires both live candidates
to rebuild, settle, board and complete recovery alive without another ship loss,
and preservation of the known recorded positive landing. No simulation retries,
seed search, tuning after outcomes or default promotion. Auditor repairs may
reuse hash-verified raw files with disclosure.

Test delayed launch against independent native motion/settling, budget enforcement,
dirty queries, moved footing, lost ownership, wrong launch time and changed
gravity/motion inputs. Run native rebuild, bot ground/recovery, harness and Python
tests, formatting, locked release/default checks and the retained Clippy baseline.
The engine implementation is unchanged. Export sources, forecasts, actual
witnesses, costs, controls, validation and archive receipts in a portable bundle.
