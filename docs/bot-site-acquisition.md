# Before the first landing choice

The [probability comparison](bot-landing-risk-probability.md) left seventy normal
attempts without a first landing choice. This audit separates their mission
arrival, available candidates, objective-route evidence and termination. Most
are not local site-search failures: **52 end before recorded arrival**. The
remaining eighteen reveal a smaller but expensive acquisition problem involving
enemy flags, repeated invalidations and outward flight while waiting.

This is a retrospective replay of the same **56 recordings on eleven worlds**,
not new validation or a policy comparison. Runtime remains
`43764580869c1ad56e6b7e9c4b7a21f485496220`; controls, planner allowances and all
timing/risk profiles are unchanged.

The follow-up [native acquisition diagnostics](bot-acquisition-diagnostics.md)
now identify the publication and candidate-rejection checks in seven retained
reproductions, with full control/physics parity. The retrospective results below
remain the original population audit.

## Population and elapsed time

Normal matches contain 164 attempts across 32 runs and eight worlds. The
mission's `arrived` event means its local capture-controller handoff: the target
is the current planet frame, distance is below radius +105, relative speed is
below 18 and physics queries are ready. It is not touchdown or proof that the
ship will stay nearby.

| Normal cohort | Attempts | Time from arrival to choice or attempt end |
| --- | ---: | ---: |
| No recorded arrival, no choice | 52 | Not applicable |
| Arrived and acquired a choice | 94 | 42.97 s total; 0.017 s median; 21.48 s maximum |
| Arrived without a choice | 18 | 633.08 s total; 19.48 s median; 150.05 s maximum |

All 52 prearrival endings are explicit mission abandonment: 43 switch to nearby
combat and nine require ship/surface recovery. Their 511.33 seconds before
termination belong to transfer/mission selection, not local acquisition time.
These are observed policy decisions, not evidence that the target has no ground.

The eighteen arrived/no-choice attempts account for **93.64%** of observed local
time before first choice. Sixteen end by leaving the destination approach frame,
one exhausts the capture time/retry budget, and one is censored by match end.
None has a missing prechoice observation. All enter with an existing enemy flag
requiring objective access; a single last observation switches local frame.

The 94 acquired choices separate usefully:

- 59 have no existing-flag requirement at choice: median 0.017 s, maximum 0.217 s,
  total 1.38 s after arrival.
- 35 require an existing flag: median 0.567 s, maximum 21.48 s, total 41.58 s.

Selection alone is not successful landing or capture. Those 94 attempts later
yield 59 completed trips, 32 abandonments and three match endings. In particular,
the 21.48-second acquisition eventually abandons its approach.

Ten arrived/no-choice attempts occur without random asteroids, consuming
535.17 seconds; eight with three-second asteroid arrivals consume 97.92 seconds.
They are concentrated in four worlds. Worlds 3 and 7 account for 88.76% of that
wait, so neither pooled elapsed time nor repeated scans establishes a general
failure probability. Quiet runs still include combat and its terrain damage.

Controlled trials stay separate: 24 recordings on three worlds have twelve
unavailable setups and twelve observed attempts. Nine acquire a choice and
complete; three lose their ships without choosing, each 1.85 seconds after the
controlled capture start. Chosen trials have median acquisition time 0.333 s,
maximum 10.77 s. Their synthetic arrival is the explicit fixture start, not an
interplanetary handoff. Repeated walking-band trials are not independent worlds.

## What the waiting observations establish

The new `tools/inspect-site-acquisition.py` binds the existing lifecycle/index,
reports and raw trace hashes, then retains dense compact prechoice ledgers.
Selection/arrival/choice/stop boundaries remain separate. Wait intervals exclude
their choice and stop observations; endpoint snapshots are retained separately.
Gaps count as missing time, never as scans or inferred counter increments.

Normal ledgers account for all **161,229 prechoice ticks**, including transfer;
controlled ledgers account for **1,326**. All windows are dense. The normal
arrived/no-choice partition contains 37,985 ticks:

| Observed evidence state | Ticks |
| --- | ---: |
| Candidate scan deferred | 35,348 |
| Candidates without an age-current route survey | 1,164 |
| Objective work marked stale | 1,140 |
| Powered routes left unclassified by this audit | 162 |
| Candidates with matching recorded complete walking/jumping round trips | 128 |
| Candidates without a complete recorded round trip | 23 |
| Scan not requested at handoff | 18 |
| Outside target frame / controller failed | 1 / 1 |

These mutually exclusive bins describe observations, not the exact controller
branch taken. The scheduled deferred rows occupy most time by design; their
empty candidate arrays do not mean absent ground or prove cadence is the
bottleneck. No prechoice row in this cohort reports an empty *measured* scan.
Increasing scan rate alone is therefore not the supported next fix.

Likewise, age-current/complete recorded route evidence is not a renewed native
permission. Actor/objective matching, joint endpoint checks, powered trajectory
validity and solar approach/parking/departure checks remain authoritative. The
audit deliberately leaves powered routes unclassified instead of duplicating
their physical validation.

The eighteen attempts increment `replans`, `objective_replans` and
`live_invalidations` **1,034 times each before any choice**. These counters
overlap; do not add them. The controller handles stale objective work by clearing
the plan and incrementing all three even when no site has ever been selected.
They are not 1,034 failed landings, independent failed searches, or evidence of
terrain changing 1,034 times. Counter reset/gap boundaries are not differenced;
initial capture-clock transitions also remain explicit unknown intervals.

Native code excludes live invalidations from the four-ordinary-retry limit,
while retaining the overall 150-second capture limit. Deferred scans and an
unsuccessful candidate selection request an outward speed of twelve; a stale
objective requests five. Those are useful short clearance commands, but a
prolonged acquisition wait can keep climbing until the nearest planet frame
changes. Mission reconsideration after that frame change does not defer the
planet; an explicit failed capture uses the existing thirty-second deferral.

## Retained diagnostic cases

`target/bot-site-acquisition/cases.json` binds the compact observations and the
original planner CSV/report hashes. These are post-hoc examples, not additional
validation samples. Queue `Ready` means computation completed, not that the
result was published, accepted or physically executed.

**Completed computation without a published survey.** In
`world3-asteroids0-seat1` (seed `6202239557458602572`), selection 14,130 arrives at
14,915 and ends at 21,053. The 102.30-second local wait spans 205 observed planner
generations; 204 have a `Ready` dispatch, with maximum observed job age 21 ticks.
The controller receives no route survey anywhere in that wait, alternating
5,934 pending and 204 stale observations. Its radial altitude rises from 72.29
at arrival to 417.10 after thirty seconds and 657.35 at the last observation.
It then leaves the destination frame. This is not a queue that never finishes.
The existing trace does not attach a native publication-rejection reason to
each completed job, so the exact dependency failing each time remains unknown.

**Negative routes, repeated invalidation and timeout.** In
`world7-asteroids0-seat0` (seed `17831127643995960170`), selection and arrival are
26,421; the attempt ends at 35,424. It records 287 live/objective invalidations
and 300 planner generations with `Ready` dispatches. The first published survey
at 26,688 contains eight outbound `disconnected` results. All 3,312 repeated
route entries seen during its 414 ready observations are disconnected; these
are not 3,312 distinct searches. The capture fails after 150 seconds at 35,423,
and the mission records termination on the next tick. Radial altitude reaches
392.55 after thirty seconds. Whole-run telemetry also contains route dependency
invalidations, but it cannot assign their precise cause to each job in this
attempt. A disconnected bounded search is not proof that no physically possible
route exists anywhere on the planet.

**Positive ground evidence still does not imply a selectable landing.** In
`world5-asteroids0-seat0` (seed `4046799272964505232`), selection 5,591 arrives at
5,992, receives a current candidate with complete recorded round-trip legs at
6,015, and leaves the destination frame at 6,162 without choosing. Thirty-two
observations have matching positive ground evidence and no native replan.
Other no-choice cases also contain powered or positive ground evidence. The
existing record does not identify which remaining selection/safety gate rejects
them. Do not weaken those gates based on this report.

## Next bounded implementation

Make acquisition an explicit bounded task before adding it to the mission cost
model. Keep the current shared planner and physical permissions:

1. Expose compact native acquisition status and rejection reasons: work pending,
   a completed search with no usable route, a publication invalidation, candidates
   rejected by endpoint/flight safety, or a selected site. Bind status to the
   target/objective/revision and original measurement tick. A generic `stale`
   counter cannot represent all these outcomes.
2. Give prolonged acquisition a progress deadline and bounded flight behavior.
   Preserve immediate collision/solar escape, then keep the ship near its target
   while valid work is pending instead of indefinitely repeating a clearance
   climb. Repeated unchanged unusable results should return a reason to the
   mission coordinator and its existing deferral mechanism. Missing or deferred
   measurements must not become an unreachable-planet verdict.
3. Compare the unchanged baseline and candidate on these saved cases, then on
   fresh worlds with seats swapped and both asteroid settings. Include the
   quick neutral/flag acquisitions as regression cases, actual captures and
   departures, ship survival, time waiting, frame exits, and planner allowances.
   Computation completed or fewer invalidations alone is not a gameplay win.

This closes the population/elapsed-time investigation. It does not provide an
acquisition probability or a universally suitable timeout. Runtime work should
make these states observable and controllable before fitting acquisition cost
or declaring an enemy planet unreachable. Remote transfer and forecasts beyond
the existing fixed landing checkpoints remain separate missing components.

## Verification and reproduction

All **325 Python tests pass**, including fifteen acquisition tests covering
half-open boundaries, missing/uncertain arrivals, deferred versus negative
evidence, counter resets/gaps, actor/clock consistency, controlled scope,
unavailable setups and source-hash rejection. Full replay verifies all 56 raw
recording hashes and reproduces the earlier lifecycle's 103 first choices.
Every elapsed tick is accounted for; final and preliminary cohort counts agree.
The selected planner CSV windows also remain within their recorded allowances.
No physics simulation or device benchmark was rerun for this read-only change.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/inspect-site-acquisition.py \
  --manifest target/bot-landing-risk-probability/normal/risk-inputs.json \
  --out target/bot-site-acquisition/reproduced-normal
```

Repeat with the `controlled` manifest and a different new output directory.
The working evidence root contains `design.json`, `final-replay.json`, both
scope reports/ledgers, `analyze.py`, `analysis.json`, `cases.json`, `checks.json`
and test logs. Raw traces and source lifecycle derivatives are preserved in the
preceding `bot-landing-risk-probability` archive; the new archive retains these
derived ledgers and their dependency binding.
