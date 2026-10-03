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
