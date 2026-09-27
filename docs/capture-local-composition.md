# Source-bound transfer and local references

Continue the [destination comparison](capture-transfer-comparison.md) with a
read-only local cost snapshot at each forecast source. This remains an observer
in PR #122. No controller selection, policy default or deployment changes.

## Frozen contract

The current and nominated branches use the same observation and evaluator view.
Read raw retained measurements and the source observation, not a previously
completed `MissionEvaluation`. Preserve original measurement and route source /
validation ticks. Check material, ownership, radius, flag location and interaction
range, claim duration, source age and local gravity. Explicit failed site/cover
measurements cannot revive cached successes. A missing cadenced route can retain
its original age within the existing route cadence. Local measurements from a
different gravity frame stay unknown; remote neutral-planet measurements retain
their separate no-flag reference basis. The separate published flag-survey shadow
is not implicitly imported into this snapshot.

Keep an independent diagnostic clock for a continuously observed selected site
and visit. Missing observations, a changed site/visit, material/flag or gravity
reset that clock. Subtract its elapsed ticks only from the landing reference for
the **current** uncommitted approach. Alternatives start a full site-choice
reference. No later cost publication updates the captured source snapshot.

An actual uncommitted capture, or a cloned accepted nomination that immediately
enters capture, explicitly has zero additional *transfer* time. That evidence
comes from controller state, never from a generic forecast refusal. Keep the
original free-flight unknown alongside it. Other branches require a completed
kinematic handoff before assigning numeric transfer time.

The empirical landing medians start at **observed site choice**, not at flight
handoff. Report the sum of known travel and local components separately from a
remaining trip reference. An unselected hypothetical site leaves the intervening
acquisition interval unmeasured and the whole trip unknown, even when both
components are numeric. Only a current approach bound to its observed selected
site can publish a conditional remaining reference. Successful phase medians
still do not predict stalls, interruption, survival or completion probability.
There is no mission-value ranking or live result consumer.

Candidate setup is bounded separately from graph work. Use the existing final
ranking operation to compose up to three references; motor equations, round
robin and allowances remain unchanged. Comparison tokens additionally invalidate
when surface identities, contributing evidence ages or observable local gravity
change, including after publication. A frame change cancels a contributing
local gravity reference. Archived results remain historical. Never subtract
publication delay from a hypothetical trip; a live proposal needs a new source.

The bounded diagnostic site/visit clock runs in every `MissionEvaluator::observe`,
including when transfer comparisons are disabled. Its time belongs to ordinary
evaluator construction, outside the separately reported comparison timings.

## Fixed replay plan

Use **all 21 source groups in the previous 13 ordinary trial specifications**,
at the previously tested shared graph allowance **64**. Reuse the archived
baseline comparison runs, verifying their recorded hashes. Run all 13
specifications again with this observer, including the same quiet/asteroid and
paired-seat conditions, sources and durations. Do not add, replace or tune sources
after observing results. Retain every unknown and cancellation in the denominator.

Require exact ordinary controls, observations, physics, evaluator/survey outputs
and upstream allocation. Audit old motor predictions or exact partial prefixes,
the unchanged one-unit final composition charge, source/candidate identities,
local evidence provenance, elapsed-time arithmetic and explicit missing phase
boundaries. New cancellations are recorded rather than forced to match the old
lifetimes. Unit regressions cover identity shifts, independent clocks, negative
observations, stale work, cadence gaps, gravity frames, new visits and expiry.

Preserved prior binary: `target/capture-flag-survey/surface-mission-soak-9f51d33`,
SHA-256 `ebf4d8d974bac8116effd4839ebf8c4bdfda4ec4b96a549a433cc472ac9b0e43`.
Archived comparison summary SHA-256:
`5009fcfc2c8e9f25e5fd6d2e1826a7746c6c65d19d71103a5f141a60fed3f8ce`.

This tests an engineering contract and its coverage, not bot strength, full-trip
accuracy or Raspberry Pi performance. No deployment is part of this slice.
