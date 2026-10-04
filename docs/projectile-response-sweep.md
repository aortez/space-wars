# Broader matched projectile-response trials

## Frozen plan

The [first physical comparison](projectile-response.md) improves the retained
speed-limited case with a half-second brake or left turn, but braking worsens
the older clear-entry path. Test the **unchanged** candidates on a broader
collection before considering integration. Keep the first warning, two-second
circle screen, 600-unit / 64-projectile sensor, 30 wall ticks, one attempt per
match, native priority guards, original transfer deadline, weapon controls and
braking-preserving steering exactly as frozen in `d97756d`.

The only runtime addition is `--probe-projectile-response-seat reporting|0|1`.
Its default preserves the old reporting-seat behavior. The suite always supplies
the evaluated seat explicitly. Existing P2 match commands intentionally retain
`--seat 0` for reporting while installing the evaluated policy and options on
P2; changing that reporting argument would confound replay comparisons. The
new selector affects only the laboratory probe. Ordinary mode remains `none`.

### Cases fixed before outcomes

Use all **eight primary armed cases** and **three health cases** from
`target/transfer-speed/v1/summary.json` (SHA-256
`0f6d1217d577f14ba590781ac00987a4ee595db99bab04c8264b9d1f742d865b`).
These cover generated worlds 0 and 1, both evaluated seats and walking/powered
capture variants. Preserve all existing seeds, physics, policies, opponent
settings, seat-specific options, 600-second limits and planner budgets.

Add **four fresh powered cases**: worlds 2 and 3, each evaluated as P1 and P2.
Continue the existing seed namespace and hash rule (first eight SHA-256 bytes,
little endian), without looking at their outcomes:

| World | Namespace | Seed |
| --- | --- | ---: |
| 2 | `powered-mission-integration-v1:2` | 12252556585289075392 |
| 3 | `powered-mission-integration-v1:3` | 15448949250423620345 |

Clone the matching world-0 powered seat configuration and replace only the seed
and descriptive item identity. Fresh cases use the primary policy configuration;
do not silently pool them with health cases or add seeds based on results.

Run observe, brake and left on all 15 cases. Add a disabled native control for
each fresh case: **49 full matches** total (4 disabled + 15 observe + 15 brake +
15 left). Run at most two simulations concurrently. Execute disabled controls,
then all observe controls, then brake, then left. Freeze code, tests and this
plan before any of these simulations; copy and hash the profiled binary.

Retain the complete previous 17-run summary, SHA-256
`925e00db0de04e329c51273fb64722a3b94d345c86dd1b2172aa9b8e63cba600`, and all its
raw-file hashes. In particular, keep the short-warning clear-entry braking
regression visible. Those development outcomes are historical comparisons,
not additional independent trials in this suite. Right steering is not a
candidate in the new suite.

### Checks and interpretation

Every retained observe run must match its prior full native observations,
actions, physical outcomes, sensor counts, policy telemetry and allocation
ledger. Fresh observe runs must match their disabled controls, including full
projectile streams. Brake/left runs must match their paired observe controls
through the trigger for both seats; require full parity whenever no physical
action changes. Independently reconstruct every current trigger, eligibility
check, fixed deadline, priority stop and action byte. Ordinary native controls
must resume after the pulse. Never infer coverage from a passing no-op case.

Report eligible ticks, projectile sampling, trigger frequency, early priority
stops, unchanged runs and changed controls. Distinguish no eligible transfer
from an eligible transfer with no warning. Audit physical captures/departures,
full match outcomes, ship destruction (including unoccupied ships), recovery,
pilot deaths and arrival handoffs. Existing native physical and planner-budget
audits remain active. Retain pulse witnesses, contact/damage receipts and full
raw reports. Contact metadata with ambiguous launch identities must be labeled
ambiguous, not attributed to one missile.

Report each of the eight retained primary, three health and four fresh primary
cases, plus separate group totals and paired deltas. Do not pool correlated
health cases as independent confirmations. Keep the stronger earlier no-escape
reference for the eight retained primary cases (2 wins, 28 captures, 27
departures), in addition to the immediate speed-limited reference (2 / 26 / 25).
Do not tune controls, duration, eligibility or warning horizon after results,
or extend sampling until a favorable case appears. Low activation coverage is
a limitation to report, not evidence that the response works generally.

All work, results and commits remain local. No policy defaults, deployment,
push, PR or merge is part of this comparison.

## Control coverage

All 15 observe controls reproduce their references exactly: 11 retained matches
match the previous speed-limited runs, and four fresh matches match their new
disabled controls. Reporting stays on P1 while the laboratory probe correctly
records the explicitly evaluated seat, including P2.

| Cohort | Observe cases | Eligible transfer cases | Cases with a warning |
| --- | ---: | ---: | ---: |
| Retained primary | 8 | 1 | 1 |
| Health | 3 | 1 | 1 |
| Fresh primary | 4 | 0 | 0 |

Both eligible cases are the **same retained world-1 P1 powered trajectory**,
with and without health handling. Each has 516 eligible ticks, starting at
8376, and 462 sampled ticks through the first warning at 8837. They flag missile
100025, launched at 7977, with the same projected 0.900-second circle entry.
The probe stops sampling after selecting its one attempt, as designed.

The other **13 cases have zero committed escape-transfer attempts**, not an
eligible transfer with no projectile warning. This includes every fresh case
and every evaluated P2 case. Their full-match diagnostic streams exist, but the
response gate never admits them. These trials test unchanged behavior and
correct seat selection; they provide no new physical avoidance examples.
There are **zero newly activated world/seat trajectories** in this suite.

## Complete outcomes

All **49 full matches and audits pass**. There are 41 full parity checks:
15 observe controls plus 26 inactive brake/left runs. The 13 untriggered
configurations account for 39 observe/brake/left trials. Four additional runs
are fresh disabled controls. Only **four runs change controls**: brake and left
on the retained world-1 P1 powered case and its health variant.

All four pulses start at 8837 and finish after exactly 30 ticks at 8867; none
stops early or renews the original 11976 transfer deadline. The six development
observe/brake/left runs reproduce the previous experiment's complete native
action/observation streams, projectile streams, physical visits, match results
and allocation totals exactly. No candidate duration, direction, warning rule
or eligibility condition was adjusted, and no simulation was rerun.

Each cell below is **evaluated bot result, physical captures / departures** over
the full match. Results for P2 refer to P2, not the reporting seat. Counts include
captures before any response and later recaptures; they are not final ownership.

| Retained primary case | Observe | Brake | Left |
| --- | --- | --- | --- |
| World 0 P1 walking | loss, 4 / 4 | loss, 4 / 4 | loss, 4 / 4 |
| World 0 P1 powered | win, 5 / 5 | win, 5 / 5 | win, 5 / 5 |
| World 0 P2 powered | loss, 4 / 3 | loss, 4 / 3 | loss, 4 / 3 |
| World 0 P2 walking | loss, 4 / 4 | loss, 4 / 4 | loss, 4 / 4 |
| World 1 P1 powered | loss, 1 / 1 | win, 7 / 6 | win, 3 / 3 |
| World 1 P1 walking | loss, 1 / 1 | loss, 1 / 1 | loss, 1 / 1 |
| World 1 P2 walking | win, 4 / 4 | win, 4 / 4 | win, 4 / 4 |
| World 1 P2 powered | loss, 3 / 3 | loss, 3 / 3 | loss, 3 / 3 |

| Health case | Observe | Brake | Left |
| --- | --- | --- | --- |
| World 0 P1 walking | loss, 3 / 3 | loss, 3 / 3 | loss, 3 / 3 |
| World 0 P1 powered | loss, 3 / 3 | loss, 3 / 3 | loss, 3 / 3 |
| World 1 P1 powered | loss, 1 / 1 | loss, 6 / 5 | win, 3 / 3 |

| Fresh primary case | Observe | Brake | Left |
| --- | --- | --- | --- |
| World 2 P1 powered | loss, 1 / 1 | loss, 1 / 1 | loss, 1 / 1 |
| World 2 P2 powered | win, 3 / 3 | win, 3 / 3 | win, 3 / 3 |
| World 3 P1 powered | loss, 1 / 1 | loss, 1 / 1 | loss, 1 / 1 |
| World 3 P2 powered | win, 4 / 4 | win, 4 / 4 | win, 4 / 4 |

The four fresh disabled controls match these fresh outcomes. Their full matches
end at ticks 7879, 36000, 18452 and 13664 respectively; all three modes retain
those endings exactly.

### Separate cohort totals

| Cohort | Mode | Wins | Captures | Departures |
| --- | --- | ---: | ---: | ---: |
| Retained primary (8) | Observe | 2 | 26 | 25 |
| Retained primary (8) | Brake | 3 | 32 | 30 |
| Retained primary (8) | Left | 3 | 28 | 27 |
| Health (3) | Observe | 0 | 7 | 7 |
| Health (3) | Brake | 0 | 12 | 11 |
| Health (3) | Left | 1 | 9 | 9 |
| Fresh primary (4) | Observe | 2 | 9 | 9 |
| Fresh primary (4) | Brake | 2 | 9 | 9 |
| Fresh primary (4) | Left | 2 | 9 | 9 |

Every improvement comes from the known development case. Against the eight-case
immediate baseline, braking adds one win, six captures and five departures;
left adds one win, two captures and two departures. Against the stronger earlier
no-escape reference of **2 wins / 28 captures / 27 departures**, left restores
the capture/departure totals and adds one win here. This is a measured aggregate
on the retained suite, not evidence of broader playing strength.

Physical follow-through also remains exact. Braking avoids the initial old
missile, hands off to capture at 9182 and claims destination 2 at 10795. Later
missiles destroy ships at 10818 and 30640; recovery produces the subsequent
captures. The primary case rebuilds twice and wins at the 36000-tick limit,
while the health case rebuilds once and loses on final ownership at that limit.
Both evaluated pilots survive.

Left steering still **takes the initial hit**, losing 18.769 hull at 8891. It
hands off at 9145, but a later cannon hit destroys the unoccupied ship at 10711
before that destination is claimed. The pilot rebuilds, captures two later
objectives and survives to win when the opponent dies at 26830. The physical
loss counter and before/after observations retain that on-foot destruction;
unchanged ship form cannot conceal it.

The earlier clear-entry short-warning regression remains in force: braking
there causes ship loss at 9173 instead of 9185, with no extra capture. Those
results are retained from the previous frozen experiment and are not counted
again in the new cohort totals. Neither favorable aggregate totals nor inactive
fresh cases erase that counterexample.

## Verification and retained evidence

The seat selector, runner, tests and plan were frozen in **`d3dfeb6`**. All
**1,093 Rust tests** and **763 Python tests** pass. Formatting, strict AI Clippy
and profiled/ordinary release builds pass. Scenario Clippy retains the same
seven pre-existing findings at the same locations. Added checks cover explicit
P2 selection without changing reporting or policies, fixed seed selection,
untriggered parity, separate cohort accounting, and ambiguous launch metadata.
The archive labels ambiguous missile attribution rather than assigning one
receipt to multiple same-tick launches.

All **2,427,452 projectile rows** pass their physical-frame, identity, ordering
and capacity audits. The maximum is four retained/in-range projectiles and
23 scanned debris entries; no unavailable shell or capacity truncation is
reported. These observations are not a general runtime bound or a deployment
performance measurement. Existing native physical capture and planner-budget
audits also pass.

The frozen binary is
`target/projectile-response-sweep/surface_mission_soak-d3dfeb6`, SHA-256
`5c88e0d3bbc7c43b8321ec328712b0e2dab788c7738885f94cfa2922e31d5936`.
The completed summary is `target/projectile-response-sweep/v1/summary.json`,
SHA-256 `aaeea92ed3c022db0c0ffd95091171e8b63496a55c0f00601120baa5f663a483`.

The [manifest](data/projectile-response-sweep-v1.json) contains all 49 outcomes,
coverage, cohort totals, per-case deltas, six exact development comparisons,
input/tool/binary hashes and **776 raw-file hashes**. The
[compressed archive](data/projectile-response-sweep-v1.json.gz) includes the
original runner summary, physical visit audits, pulse witnesses, active-case
loss/contact receipts and projectile tracks, report checkpoints and final
reports excluding their large samples/events arrays. Final recovery counters
are retained for every case. Complete raw reports and streams remain at their
hashed local paths. All inputs and archived raw files were hash-verified.

```sh
python3 tools/validate-projectile-response-sweep.py \
  --prior target/transfer-speed/v1/summary.json \
  --responses target/projectile-response/v1/summary.json \
  --binary target/projectile-response-sweep/surface_mission_soak-d3dfeb6 \
  --out /tmp/projectile-response-sweep
python3 tools/analyze-projectile-response-sweep.py \
  --summary /tmp/projectile-response-sweep/summary.json \
  --out /tmp/projectile-response-sweep-results.json
```

Use fresh output paths and a clean checkout for the runner.

## Decision

Keep both responses as laboratory candidates and leave defaults unchanged.
The larger suite reproduces the existing success and verifies inactive cases,
but does not supply another activated trajectory or a physical P2 response.
Before broadening the gate, the useful next measurement is to inspect projectile
warnings during **ordinary native transfers** using the saved read-only streams:
measure their frequency, warning time and actual contacts in both seats. That
can identify concrete additional states for a separately frozen intervention
experiment without changing eligibility based on favorable outcomes here.
All work remains local; nothing was pushed or deployed.

The subsequent [ordinary-transfer survey and response experiment](ordinary-transfer-response.md)
completes that read-only survey and a separately frozen eligibility extension.
Its 47 full matches include both newly exposed warning states and exact legacy
compatibility checks; the extension remains opt-in.
