# First learned-guidance match hybrid

This is the integration plan for [#291](https://github.com/aortez/space-wars/issues/291).
Use Recovery bot v1 as the host and test a retained arrival component during
one short, unobstructed approach to a destination planet. The
[first census](../learned-guidance-approach-census.md) found short approaches
in both seats, but none satisfying a conservative distance-and-speed filter
based on the retained practice starts. The
[eight-case approach experiment](../learned-guidance-approach-probe.md) then
found 4/8 successful native handoffs for the retained genome, below the frozen
8/8 gate. The arrival reference completed 8/8 using the same nine inputs.
The [bounded continuation experiment](../learned-guidance-match-training.md)
improved those approaches to 6/8 and reserved approaches from 6/16 to 12/16,
but also regressed three old moving-target arrivals. Its acceptance gate failed.
The [broader retention experiment](../learned-guidance-match-retention.md)
then reached 16/16 on four new validation worlds and 20/24 inspected approaches,
but five other original arrivals regressed. The
[failure diagnostic](../learned-guidance-failure-diagnostic.md) now provides
tested full-suite retention support and shows a braking/settling tradeoff:
forcing the brake on completes all 43 inspected match starts but only 9/157
older arrivals. The five v2 precision failures enter the target window but do
not sustain its required dwell. The
[full-retention experiment](../learned-guidance-full-retention.md) now trains
against all 157 older starts with the same network, fitness order, and 15 match
training starts. Two of its 16 requested new validation starts are missing;
the bounded run can measure joint improvement but cannot pass its full gate.
Learned control in ordinary matches and a selectable match brain remain gated
on that evidence.

The Training workflow, saved practice assemblies, and history browser are
delivered in #287, #308, and #319. This slice connects those component artifacts
to a match host; it does not require whole-brain evolution or another organism.
See the [guidance contract](guidance-component-v1.md) and the
[larger training design](evolutionary-training.md).

## Host and candidate

The handwritten reference is the current **Recovery bot v1**: Value v13 mission
policy, `terrain_recovery_supported_hatch_ordinary_offsets_v1`, and
`material_hunt_clearance_handoff_v1`. Use its real paired recovery-observation
driver and the client host's destination/flag planning schedule. Other opt-in
mission experiments stay disabled. Two humans remain the launcher default.

Start with the retained mixed-arrival **seed-1** finalist. Choosing the lowest
declared training seed gives a reproducible choice without using match results
to select among the three successful finalists.

| Identity | Value |
| --- | --- |
| Artifact | [`seed-1.json`](../../crates/engine-agent/fixtures/ship-arrival-mixed-v1/seed-1.json) |
| Genome | `693447c6df8315017a67f8dba8809568cf08f3ec66131302bfd026fa49f343fb` |
| File SHA-256 | `a1d127eb9eb96f190616d5d3ee377490d5a06ac2023302c9eaeb27a971945199` |
| Organism / role | `material_ship_open_v1` / `arrival_guidance_diagnostic_v1` |
| Observation / action | `arrival_local_9_v1` / `turn_tanh_thrust_brake_v1` |
| Cadence | One decision per physics tick |
| Proposed match adapter | `recovery_arrival_match_v1` |

[Original provenance](../../crates/engine-agent/fixtures/ship-arrival-mixed-v1/provenance.json)
records all three finalists and their original executable identity. Its arrival
results concern one stationary material world and build. Moving planets, match
speeds, combat, and destructible approaches require new evidence. Rerun the
retained arrival suite on the eventual integration executable; keep that receipt
separate from the original results and from match evaluation.

## First phase: direct destination approach

The proposed phase is the end of `MissionGoal::Transfer`, after the nearest
physics frame has become the selected destination and before native capture
starts. Launch, long-distance travel, obstacle detours, combat, landing,
on-foot work, and recovery remain handwritten.

The existing mission path already supplies an entry point at the destination
radius plus 85 world units. It starts capture only when the ship is in the
destination frame, within radius plus 105, slower than 18 units/s relative to
the planet center, and has ready queries. These are existing host gates, not
learned success declarations. The ordinary transfer has a 60-second overall
budget and a 20-second progress budget; preserve their original clocks.

At admission, the proposed adapter freezes the radial bearing of that entry
point. Thereafter the marker follows the destination center with that fixed
world-space offset, and its velocity is the center velocity. This gives a
consistent position/velocity pair independent of subsequent model steering.
It differs from the ordinary entry's continuously recomputed radial bearing;
version and test that difference explicitly. Planet acceleration is also new
relative to the constant-velocity practice marker.

The host must still verify that the frozen entry has a direct unobstructed route
each tick. A new detour, frame change, destination change, or native arrival
ends delegation. Do not pass a landing site, opponent, or clearance waypoint
through this adapter as if it were the same task.

### Admission is an experiment gate

Require a living pilot aboard the assigned available full ship, armed controls,
the trained open-wing flight limits, no wing transition, no contact or supported
feet, no landing assistance, finite observations, and no saturated features.
Require the destination frame, direct transfer, and no higher-priority host
task. Losing any condition ends the attempt.

The practice episode permits radial altitude 40–220, but those limits are
failure boundaries, not evidence that every state inside them was trained.
Likewise, the encoder's distance and speed scales are normalization constants,
not a supported flight envelope. The retained development starts include
short offsets up to 35 units and initial speed up to 6 units/s; their sampled
paths and moving-target combinations need separate inspection.

The directed comparison now establishes a gap beyond the old practice starts:
in both distinct failed network starts, it turned under full thrust without
braking until the native host required a climb. Add a match-relevant arrival
curriculum using the existing framework and nine-input contract. Treat the
inspected starts as development cases, retain the original arrival regressions,
and freeze separate validation cases before evaluating a new candidate. Freeze
numerical admission bounds only after development measurements, and do not widen
them simply to obtain activations in the eventual qualification matches.

## Control ownership and fallback

Put the optional delegation at the mission's transfer-guidance branch, after
ordinary priority, deadline, arrival, and route decisions. Do not calculate a
complete handwritten movement command and then replace it in the client: that
would advance the handwritten steering accumulator for controls never applied.
The coordinator continues observing physical progress every tick; only the
selected low-level guidance owner advances its steering memory.

The learned phase owns steering, thrust, and brake together. Emit open wings
and neutral weapons, interaction, and mining every tick through canonical
actions. The current mission encoder emits three channels while guidance emits
four, including mining; the integration must deliberately handle the complete
control set. Combat aiming cannot be combined with the learned turn output.

Use an explicit neutral control tick when changing owners. Create a fresh
guidance controller after handoff, reset, vehicle/actor replacement, target
replacement, or physics-frame change. Bind requests to episode, actor,
spaceling, vehicle, planet, target, control epoch, and tick. Pause consumes
neither a decision nor a handoff tick. The existing guidance controller rejects
skipped/conflicting ticks, so it cannot simply resume after a handwritten gap.

The initial proposed bound is one attempt per mission selection and vehicle,
at most 300 physics ticks (five seconds), further limited by the original
mission deadline. This bound must be frozen with the admission profile before
candidate testing. Timeout, model error, damage during the attempt, lost
eligibility, or a higher-priority task releases control and latches handwritten
fallback for that selection. No reset of mission progress, retries, capture
failure history, recovery/scuttle state, or native support/transfer gates is
allowed. Fallback does not turn a failed learned attempt into an arrival success.

Record the host and adapter identities, exact genome, active control owner,
target position/velocity and identity, admission tick/deadline, binding epoch,
raw and encoded features, saturation, applied actions, and exit/fallback reason.
Count eligible opportunities, admissions, owned ticks, successful native capture
handoffs, and failures separately. A match in which the model never ran is not
a successful learned-guidance trial.

## Development census

The ignored client diagnostic
`recovery_guidance_approach_census` uses the normal `spacewars` launcher with
Recovery v1 in both seats. It records direct-transfer geometry through the
completed-step free-flight projection without running a model or changing
controls. Each observed run has an unobserved control: compare mission telemetry
and free-flight state each tick, and scenario payloads every second and at a
native match finish.

The fixed development sample is seeds 42 and 10101, weak Angular and Round dirt,
ordinary combat, environmental asteroids off, and the first 180 seconds of a
native ten-minute match or an earlier native finish. These inspected prefixes
measure opportunities; they are neither held-out cases nor completed-match
strength evidence. Record the full settings, source diff, executable hash,
case termination, and output checksums alongside the report.

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_GUIDANCE_CENSUS_OUT=/tmp/spacewars-guidance-census-NEW \
cargo +1.89.0 test -p engine-client --bin engine-client \
  --locked --offline --profile ci recovery_guidance_approach_census \
  -- --ignored --nocapture
```

The output directory must be fresh. `approaches.jsonl` contains every transfer
decision already in its destination frame, including rejected physical states;
`summary.json` preserves phase coverage and the control comparison. The entry
position is the current ordinary radial entry, with center velocity recorded
for comparison. This measurement is not yet the frozen-bearing adapter or a
certificate of obstacle/asteroid safety.

## Qualification sequence

1. **Establish capability.** The census and frozen eight-case comparison are
   complete. The first candidate failed at 4/8; the arrival reference achieved
   8/8 and ordinary mission guidance 6/8. Two bounded training continuations
   improved approach success but failed their gates through remaining approach
   failures and moving-target regressions. The second retained its expanded
   28-case practice set and passed 16 new validation approaches, yet lost five
   other old arrivals. The controlled diagnostic now traces those settling
   failures and shows that a brake-only intervention solves inspected match
   starts while breaking precision. Full original-suite retention is supported
   and tested. The third frozen training recipe uses all 157 original starts
   and records settling traces after selection. Two missing new validation
   starts already prevent its full gate; no joint solution is qualified yet.
   Retain contact, assistance, damage, timeout, and failed native handoffs as
   outcomes. Require the development gate and fresh validation before building
   a match selector; the current result does not qualify live delegation.
2. **Test the component and host.** Rerun retained arrival cases on the exact
   new executable. Exercise the directed match phase against the original host
   and the handwritten arrival reference using identical initialization. Include
   both seats, target/frame changes, reset/pause, wing transition, assistance,
   contact, damage, vehicle loss, recovery, saturated/invalid features, model
   failure, timeout, and native capture handoff. Add checks that disabled or
   never-eligible operation preserves the original host's actions and deadlines.
3. **Freeze and run paired matches.** Predeclare four new world seeds, two dirt
   shapes, and asteroid-off/8-second-mixed conditions. Each of those 16 physical
   configurations runs a Recovery-v1 control and two hybrids, placing the
   candidate in one seat at a time: 48 native ten-minute matches, ending earlier
   only by native outcome. Keep all cases, including zero-admission ones. Freeze
   exact settings, seed manifest, source/build, genome, admission/fallback
   profile, and quantitative retain/reject thresholds before any outcomes.
4. **Decide and expose.** Retain the candidate only with correct ownership,
   useful activation coverage, and acceptable mission, recovery, survival,
   win/draw, no-progress, and cost results. Inspect paired differences by seat,
   dirt shape, and asteroid condition; report the shared-world dependence.
   Rejection or a narrower supported scope is a valid result. A retained
   candidate then gets an explicit per-seat selector, clear host/genome identity,
   client tests, and a controlled 1× device check. Changing defaults is a separate
   evidence-backed decision.

Zero stale actions, unauthorized ownership, or budget resets is a hard gate.
Directed supported cases must complete the native arrival handoff without new
damage or recovery failures; adversarial cases must fall back as specified.
The quantitative match and device-cost thresholds remain to be fixed with the
case manifest after development measurements, before qualification. None of the
steps above establishes general superiority from replay agreement alone.
