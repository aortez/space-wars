# Test supported arrival at recovery hatches

The retained Round boarding witnesses establish that ordinary controls can
recover the blocked pilot. The original task stops horizontal steering when an
unsupported actor passes within 2.8 units of its hatch. Test that arrival gate
alone before adding retries, stronger steering or a jump policy.

Add a separately selected `hatch_supported_arrival` option, disabled by default.
For hatch destinations only, proximity must not trigger arrival until the pilot
has support on its retained planet or the native boarding sensor reports Ready.
Otherwise continue the existing measured route and its proportional steering.
Keep map validation, query/input gates, jump/get-up behavior, waypoint rules,
time limits, ship-settling limits, flag claiming and rebuild arrival unchanged.
Native Ready continues to authorize the recovery task's normal Interact press.
Selection preserves controller memory and cached decisions; reset retains the
selected configuration. The option remains limited to experimental hosts.

First reconstruct the seed-20202 Round/collapse-off candidate-P2 mission to
tick 24743, before its first post-rebuild decision. Verify the original input,
decision and scenario checksums. Clone the complete world, both controllers
and host queues. Change only the candidate's option; reversing that selection
must reproduce the original controller debug state exactly. Preserve the
disarmed input state and let the real controller supply its ordinary neutral
release. Continue both arms through native match end, with no input override,
reset, physics edit or diagnostic boarding assistance.

Then run sixteen paired generated-world cases from tick zero: seeds 10101 and
20202, Round and Angular, collapse off/on, and the terrain-recovery candidate in
P1 or P2. Each control uses the existing candidate profile and must exactly
reproduce the corresponding retained mission, including its full saved final
state. Its paired candidate adds only supported hatch arrival. Retain every
outcome, including cases that never exercise hatch recovery. Do not count them
as successful recovery demonstrations. Each match uses the existing ten-minute
limit, asteroid pressure, reactive opponent and host planning quotas.

Log ordinary actions, physical support and arming, native jump/get-up counts,
route decisions, boarding gates, completed recoveries, subsequent mission
activity, material conservation, speed bounds and policy divergence. Require
actual native boarding followed by recovery-task completion, not proximity or
an airborne waypoint, for the targeted success. Track new blocked recovery and
mission periods and any change to completed recoveries in the broader pairs.
Do not infer a win-rate improvement from this small deterministic set.

Decision-contract tests cover supported arrival, continued airborne steering,
brief contact loss, cached ticks, clone/reset, neutral rearming, dirty queries,
missing routes and existing deadlines. Keep quiet flag-claim and rebuild
landing behavior, and native boarding priority. Run the AI library, ground
navigation, recovery and relevant physical gameplay tests; retain strict AI
lint, default-feature compilation and formatting receipts.

Freeze the source and binary before retained simulations. Record failures and
do not tune this same runtime after inspecting outcomes. If the candidate
fails or regresses recovery, leave it opt-in and report the next evidence needed.
Keep the Angular enclosure, collapse-on placement-distance gate and placement
cost separate. Final published #174 integration and device qualification remain
pending; this comparison uses the supplied frozen terrain integration.
