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
