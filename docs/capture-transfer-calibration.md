# Controlled transfer calibration

This follows the [31 live-priority transfer probes](capture-transfer-probes.md).
Eight allowed transfers ended when the coordinator chose to pursue an opponent.
We need to distinguish incomplete flight evidence from that mission decision
before changing a transfer cost or acceptance rule.

## Predeclared paired experiment

Use the complete prior record under
`target/capture-flag-survey/transfer-probes-v2`. First replay all 31 ordinary
probes and require their full controller trace hashes and outcomes to match.
Retain the 22 previous nomination refusals. Pair **all nine previously accepted
nominations**, including the completed transfer, with the controlled mode.
Selection depends on nomination acceptance, not subsequent success.

The second intervention is `--probe-transfer-pursuit defer_new`. The source
nomination uses ordinary priorities. From the following tick through the same
accepted destination/selection identity, defer only creation of a new pursuit.
The control method receives this opt-in on every call; no persistent bot setting,
pursuit cooldown or observation changes. Existing pursuit maintenance,
disengagement, recovery, solar/boundary guidance, commitment gates, progress
watchdogs, weapons and opponent/asteroid physics remain active. This changes
mission priority and can change the actor's firing and resulting combat exposure.
It does not promise identical combat conditions after divergence.

A copy of the bot also computes ordinary controls at each controlled state. It
never supplies physics actions or advances another world. This records each
suppressed opportunity's reason, tick and ordinary actions. Pairwise observations,
encoded controls and mission telemetry must be exact before the first suppression;
that boundary must correspond to the ordinary branch's actual pursuit event.
Pairs with no suppression must remain exact throughout. Suppression on a terminal
contact observation is counted separately from executed controlled ticks; contact
still takes precedence and neither terminal command is executed.

The cap and endpoints remain those of the prior probes: at most 3,600 steps,
actual landing handoff or conservative contact/loss/recovery/retarget/match
interruption. Deferral remains active on the cap observation, so its expiry
cannot create a spurious last-tick pursuit. This measures handoff, not landing,
capture, or the analytic reference's rest-to-rest endpoint.

## New sources

Freeze two new world seeds using the first eight SHA-256 bytes, little endian,
of `transfer-calibration-v1:0` and `transfer-calibration-v1:1`:
`13100125988314582075` and `13516619674560914923`, each under quiet
and three-second asteroid pressure. Both actors use v13/native observations.
Each of these four discovery runs lasts at most 180 simulated seconds.

Discovery is observational and pre-intent. Between ticks 60 inclusive and 10,800
exclusive, retain the first source per seat having an alternative that passes
the ordinary nomination gates. Require an armed, available full ship with
positive health, ready queries, aboard location, and Launch/Transfer/Capture
mission state. An alternative must differ from both the current destination and
current approach frame. Sort by planet index and retain at most two alternatives,
recording truncation. Existing ownership, height, commitment, cooldown/defer and
once-per-trip gates apply. Higher-priority controls can still refuse a nomination.

Keep every discovered alternative regardless of whether its analytic reference
is supported, obstructed, beyond the horizon, or otherwise unknown. Keep missing
sources and actual nomination refusals; do not retry at a more convenient tick.
Write all discovered pairs before running either mode. Each pair replays the
frozen discovery prefix and exact source inputs, then uses the same endpoint
rules. Maximum work is 40 historical branches, 32 fresh branches and four
observational discovery runs.

## Accounting and reproduction

Controller phase segments use executed observations `[source, terminal)`.
Launch, Transfer, avoidance and approach-frame changes are recorded separately;
they do not map directly to analytic settle/turn/climb/cruise terms. Record
first suppression, repeated suppressed observations, interruptions and completed
handoff durations. These are correlated engineering cases, not strength trials
or automatically valid planning costs. No constants are fitted in this slice.

Source identity remains bit-exact for f32 inputs. Diagnostic geometry is checked
independently, including sun/boundary rejection, while supported references and
unknowns without geometry are retained. Reconcile probe rows with the actual
controller trace and final world state. Probe construction, source sampling,
same-state comparisons and IO are outside live planner fuel and cannot establish
Pi FPS or worst-case bot CPU cost. Native/default play invokes none of them.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/calibrate-transfer-references.py \
  --shadow-reference target/capture-flag-survey/value-shadow-final \
  --previous-probes target/capture-flag-survey/transfer-probes-v2 \
  --out target/capture-flag-survey/transfer-calibration-v1
```

The output directory must be new. The runner writes seed and historical plans
before physics, writes the complete fresh plan after discovery, and preserves
commands, source/binary/raw-log hashes and every audited result.

## Results: 26 September 2026

The complete record is [capture-transfer-calibration-v1.json](data/capture-transfer-calibration-v1.json).
Raw reports, compressed controller traces and probe logs remain under
`target/capture-flag-survey/transfer-calibration-v1`. Physics ran from clean
`43c03cc`, binary SHA-256
`e00adc0f0a464a47587134dcba6fd7de072e80cd57333571537258d77364136b`.
The later checker correction at `724b9df` handles null contact diagnostics when
a ship body disappears. It adds no runtime change. Re-auditing all 17 pairs with
that checker reproduces every recorded result; this corpus has no missing-body
terminal case. A regression exercises that interruption separately.

All 31 ordinary replays have the same full controller trace hashes, diagnostics
and outcomes as the previous study. The 22 nomination refusals remain in the
record. All four fresh discovery runs found both seats' first eligible source,
with one alternative each and no truncation. The resulting experiment contains
17 pairs, including all nine historical eligible nominations and all eight fresh
alternatives.

| Cases | Ordinary priority | Defer new pursuit |
| --- | --- | --- |
| Nine historical nominations | 1 handoff, 8 pursuit interruptions | 9 handoffs |
| Eight fresh nominations | 5 handoffs, 2 pursuit interruptions, 1 contact | 7 handoffs, 1 contact |

Ten pairs first diverge at an actual new-pursuit decision. The other seven remain
exact throughout. There are 3,204 deferred observations, of which 3,200 supply an
executed command. The other four are terminal handoff observations. This is
16/17 controlled handoffs in a correlated engineering corpus, not a success-rate
estimate or evidence that deferring combat improves match results.

### Historical flights

World 0 remains unchanged: handoff after 391 ticks (6.52 seconds), with no
suppressed pursuit or avoidance. The four short World 1 enemy-planet flights also
use no avoidance. Each changes approach frame once and hands off after:

| Source tick | Enemy planet 1 | Neutral planet 2 |
| --- | --- | --- |
| 3816 | 8.47 s | 28.20 s |
| 3876 | 7.20 s | 26.93 s |
| 3934 | 6.55 s | 26.23 s |
| 3997 | 6.00 s | 25.62 s |

Ordinary priority interrupts all eight at tick 4189. The controlled neutral
flights execute 714–793 avoidance ticks: 177–251 around departure planet 0 and
537–542 around the sun. Their first analytic rejector is still planet 0; it does
not describe all later guidance. All nine historical controlled ships retain
their source health and have no solver/debris contact before handoff.

### Fresh flights

Sources are ticks 132/134 in fresh world 0 and 131/131 in fresh world 1 for seats
0/1. Quiet and asteroid conditions share source geometry because these samples
precede asteroid pressure. The table gives controlled durations from nomination
to handoff, except for the retained interruption.

| World / pressure interval | Seat 0 to planet 2 | Seat 1 to planet 0 |
| --- | --- | --- |
| Fresh 0 / quiet | 28.23 s | 39.43 s |
| Fresh 0 / 3 s | 28.25 s | 39.40 s |
| Fresh 1 / quiet | 38.75 s | 21.60 s |
| Fresh 1 / 3 s | Asteroid contact after 7.02 s | 21.60 s |

Only fresh world 0 seat 1 defers a pursuit (both pressure conditions). The other
six pairs remain identical, including the asteroid interruption. All eight
fresh references reject static overlap with their departing-frame planet. No
supported reference, first-rejected sun/boundary case or missing source occurred;
the sampler would retain them, but this corpus does not validate them physically.

The fresh handoffs execute 432–1,941 avoidance ticks, including sun guidance.
Fresh world 1 seat 0's quiet flight also avoids planet 1. Several trips change
approach frame twice. Successful ships keep full health. At the sole interruption
(tick 552), an asteroid spawned at tick 298 produces a nonplanet hull contact with
separation -0.929 and health falls from 100 to 99.503. This is a damaging asteroid
contact, not evidence of hitting the planet that rejected the analytic corridor.

### What this establishes and what follows

Pursuit selection caused the earlier historical interruptions. With the explicit
priority intervention, ordinary flight guidance can reach all of those handoffs.
This does not make their currently rejected transfer references valid: the live
controller climbs, changes frames and guides around bodies, while the reference
checks a simpler corridor. All fresh rejected stage sums are 31.64–38.33 seconds,
already beyond the reference's 30-second horizon. Clearing the first geometry
rejection alone would therefore leave another reason to withhold those costs.

Observed handoffs use the actual range `< radius + 105` and relative speed `< 18`
gates. The analytic reference aims at a different rest-to-rest endpoint near
`radius + 85`. Its rejected stage arithmetic is recorded for diagnosis only;
subtracting handoff time would not measure a validated prediction error. Likewise,
controller Launch/Transfer counts are not measurements of the analytic stage
terms. These probes end before landing, capture and return-to-ship performance.

The next bounded model experiment should explicitly represent departure from
the current body, subsequent avoidance and arrival at a defined endpoint. Start
with an offline or shadow candidate and preserve unknown results when its phase,
geometry or time budget is insufficient. Check it against these retained paired
traces and new predeclared worlds before admitting finite mission costs. Keep
mission-priority policy separate: this study supplies no match-value comparison
for declining a hunt. Live acceptance, rankings and pursuit behavior stay as they
were; this slice fits no constants and introduces no new policy version.

## Validation and device

170 AI tests, 359 Python tests, formatting and strict AI Clippy pass. The native
three-minute observer test passes in 59.63 wall seconds with exact ordinary
controls, evaluator outputs and physics, including actual shadow admission.

Independent review recomputed report/probe/source and decompressed-controller
hashes, then reconciled all 36,889 paired probe rows to controller state, actions,
contacts and terminal precedence. It rebuilt every phase segment and checked all
22 arrival predicates across both modes. It also checked 153,016 pre-source
seat-control rows, 38,038 prior evaluator records, both source observations, first
fresh-source eligibility and every diagnostic geometry. Review fixes cover
terminal suppression accounting and missing-body loss diagnostics. No remaining
review blockers were found.

Revision `724b9df` was built and deployed to **sw-picade.local** with the fast
application updater. Installed client SHA-256 is
`9932d758084f5ef8533c9b386941ad6389c5f4621e939671215134a62bc37d34`;
CLI SHA-256 remains
`0d33f4d82a80df22cec0a56d74a3903d4fe05fe389100c2b174f752f4a4ca1f3`.
The kiosk is active (PID 788, zero service restarts), with automatic P1 v10 / P2
v13 matches retained. Calibration and source sampling are headless opt-ins; the
device continues using ordinary combat priorities. Logs are
`target/capture-flag-survey/calibration-{unit,python-final,native-parity,pi-build,deploy}.log`
and `calibration-pi-health.txt` in the same directory. This is a functional smoke
check, not a controlled device performance comparison.
