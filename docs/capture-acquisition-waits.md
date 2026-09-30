# Separating scan time from landing-site selection

The [three-site arrival comparison](capture-arrival-neighbors.md) left native
acquisition time unknown. Before adding a timing model, distinguish the local
scan schedule from the controller's ability to select a candidate. This is a
retrospective audit of existing evidence, with no new simulations, gameplay
changes or fitted cost coefficients.

## Scope and method

Audit all 164 attempts in the 32 normal recordings of the
[historical acquisition study](bot-site-acquisition.md), including attempts
that never arrive or never select a site. Verify the archived manifest, archive
and every consumed member hash. Its dense derived observation ledgers retain
scan availability and the native choice endpoint, but predate native rejection
telemetry and do not retain the full claim state. A missing enemy-flag requirement
therefore does **not** prove eligibility for the neutral-idle timing model.

Separately audit the acquisition-probe windows in all 16 frozen three-site
arrival runs. Verify their summary against the tracked projection, then verify
each report and compressed trace. The eight ordinary windows have no acquisition
probe; keep them outside that denominator. The controlled nearest/neighbor modes
repeat the same playing trajectories. Identify identical full acquisition
windows by a canonical raw-row hash; retain both records but summarize each
unique window once. Four original source ticks from one world remain correlated.

For each arrived attempt, the waiting interval is `[handoff, first choice)` or
`[handoff, interruption/censor)`. The choice observation is a separate endpoint.
Require every waiting tick and the choice endpoint, with matching actors and
destinations. Preserve gaps as audit errors, not zero-time waits. Retain observed
scan states separately from actual native rejection reasons; accept native
telemetry only at its current tick, planet and material revision. Solar escape
can leave old local-controller telemetry behind.

The existing FourHz mission sensor cadence has a 15-tick no-flag period, with
seat offsets of zero and seven ticks. A first scan for a different planet or
ship form is immediate; a retained matching survey can defer the next scan.
An enemy-flag approach uses the route-survey schedule instead. Check recorded
no-flag deferrals against the 15-tick phase, and check explicit next-scan ticks
where available. Do not reconstruct absent historical survey stamps.

Neither scan availability nor a negative result at one bearing determines the
future native choice. The [two negative neighbor samples](capture-arrival-neighbors.md)
also remain a separate geometry question. They precede local acquisition and
do not establish a delay in the full native scan.

## Reproduction

```sh
python3 tools/audit-acquisition-waits.py \
  --historical /home/oldman/.codex/visualizations/2026/09/23/bot-site-acquisition \
  --arrivals target/capture-flag-survey/arrival-neighbors-v1 \
  --out target/capture-flag-survey/acquisition-waits-repeat.json
```

Use a new output path. This reads the archived ledgers directly without
extracting the older multi-gigabyte raw match corpus. Their original trace
hashes and prior verification remain in the report; this audit does not claim
to reverify those raw historical traces. Current native windows are read from
the hash-verified physical replay traces.

## Findings

The historical audit retains all 32 recordings, eight worlds and 164 attempts:

| Observed local domain | Attempts | Chose a site | Wait before choice |
| --- | ---: | ---: | --- |
| No enemy-flag requirement throughout | 59 | 59 | 57 at one tick; two at 13 ticks |
| Enemy-flag requirement throughout | 52 | 35 | 13–1289 ticks (up to 21.483 seconds) |
| Approach context changes before the attempt ends | 1 | 0 | Interrupted without a choice |
| No recorded arrival | 52 | 0 | No local acquisition clock |

All **59 no-enemy-flag choices occur on their first observed available scan**.
Their complete prechoice time is 83 ticks: 59 initial `not_requested` handoff
observations and 24 deferred observations. There are no observed measured empty
scans or candidate-bearing scans before those choices. This does not establish
that every future scan will succeed.

Both longer examples are from `world0-asteroids0-seat1`, seed
`10605928304781745952`:

| Mission selection | Destination | Handoff | First scan and choice | Deferred observations |
| ---: | ---: | ---: | ---: | ---: |
| 13944 | 1 | 14260 | 14273 | 12 |
| 26314 | 0 | 26710 | 26723 | 12 |

Every deferred tick and both selection endpoints agree with the seat-one
15-tick scan phase. Each final native survey supplies 64 candidates. The old
ledgers do not contain the previous survey stamp or exact native rejection
telemetry, so those values remain unknown. These are two attempts in one match,
not two independent worlds.

The one context-changing attempt starts with an enemy flag, then observes a
different approach planet on its last waiting tick. It is the historical
world-three/seat-one attempt selected at 21054, arriving at 21903 and ending at
23248. Preserve it separately; the changed objective requirement is not evidence
that its destination flag was destroyed. Thus the previous study's 18 arrived
no-choice attempts still all **start** with an enemy flag: 17 keep that local
domain, and one changes approach frame. The 52 prearrival interruptions are
neither zero-time acquisitions nor local selection failures.

The current corpus has eight controlled recordings, deduplicated into **four
correlated acquisition windows**, plus eight ordinary recordings without an
acquisition probe. Each controlled window has one `not_requested` handoff tick,
then a survey and an actual `selected_site` decision. All four have a neutral-idle
claim at both observations. Native selection sees 57, 58, 57 and 57 usable sites,
with **114, 116, 114 and 114 eligible directions**, respectively; it records no
candidate rejections. These are native selector counts, not observer samples,
physical-query counts or independent trials. No stale local telemetry is
used as a current rejection reason.

This explains why the two earlier negative neighbor measurements did not delay
these choices: the later full native scan had many usable candidates. It does
not identify the exact footing-ray/slope branch behind those earlier negatives,
and it does not turn them into positive measurements retrospectively.

## Consequence for planning

The evidence supports modelling **the next scan opportunity** separately from
the time until a site is selected. For an ordinary neutral handoff, the former
can use the cloned controller's retained survey stamp, actor phase and one-tick
handoff. A new survey context permits the next-tick scan; a matching retained
context may wait for the 15-tick phase. That is a schedule calculation, not a
worst-case bound on acquisition. A scan can still yield no usable or eligible
candidate, and interruptions can prevent selection entirely.

The next bounded implementation should expose that conditional scan clock in
the transfer shadow, compare it with recorded native scan ticks, and preserve
unknown selection time after rejection. Only then compose a conditional
first-scan-success reference with the existing local trip reference. Keep the
conditions explicit: unchanged neutral claim/material context, a usable native
candidate and solar-eligible direction, and uninterrupted control. Enemy-flag
route acquisition remains a different model, already covered by the
[native diagnostics](bot-acquisition-diagnostics.md) and
[bounded-acquisition experiment](bot-bounded-acquisition.md).

No universal one-tick acquisition constant, increased physical-query budget,
live destination-ranking change or complete trip estimate follows from this
audit. It narrows the next implementation and avoids importing the historical
enemy-flag tail into an unrelated neutral timing reference.

## Validation and retained evidence

The [complete audit record](data/capture-acquisition-waits-v1.json) preserves
all historical attempt identities and original raw trace hashes, 33 consumed
archive-member hashes, both longer no-flag wait ledgers, current native choice
endpoints, repeated-mode bindings and skipped ordinary records. It is copied
unchanged from `target/capture-flag-survey/acquisition-waits-v1-reviewed.json`,
SHA-256 `5f33836ee4fe2d374f5e8d97e61e3697d743915842fd7657314db01e91f65e28`.
The report records the audit tool and both imported helper hashes.

All **541 Python analysis tests** pass, including 21 focused acquisition-audit
tests. Tests exercise stale native telemetry, deferred versus empty scans,
changed objective domains, missing/duplicate ticks, wrong actors, earlier
choices and interruptions, an artificially retimed handoff, unwitnessed stop
events and unobserved match/runner censors. No Rust or device code changes.

Independent review identified missing links between the probe clock and its
actual transfer arrival, plus missing interruption witnesses. The audit now
reuses the existing probe's first-choice and endpoint predicates, checks the
arrival event and dense observation counters, and retains a separate observed
stop witness. It also verifies the complete sixteen-run mode coverage and
records helper hashes. The reviewed rerun leaves every measured group unchanged;
the initial audit artifact remains in `target` for comparison. No physics was
rerun and no cases were dropped or added during these corrections.
The final independent review reports no remaining substantive findings; its
51 focused and helper tests pass and the notes match the retained evidence.
