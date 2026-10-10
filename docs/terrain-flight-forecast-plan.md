# Bounded high-terrain forecast experiment

The two retained native probes crossed the recovery ledge, but ended before
claiming, rebuilding or boarding. This experiment tests a read-only controller
forecast and the complete recovery sequence. It does not promote a default bot.

## Frozen implementation

Use real surveyed endpoint nodes and the existing fixed-step flight controller,
orbital/gravity recurrence, capsule clearance, 98% launch gate and 5% landing
reserve. Keep the actual pod/ship colliders; terrain forecasts never substitute
a prospective vehicle hull. Both independently recharged directions must pass
at launch offsets 0, 15 and 30 ticks in moving environments. Each leg is limited
to 720 steps. The forecast expires 30 ticks after measurement.

On each staggered 30-tick on-foot pod survey, consider only the nearest high,
disconnected adjacent gap: endpoint rise above 7 and below 17 units and distance
at most 12. Examine the existing eight symmetric endpoint margins and heights
3, 5, 7 in order, using retained nodes, distance 1–24 and angular span at most
0.55 radians. Admit only cruises above the old ten-unit bound and below twenty.
Publish the first fully approved pair, with its exact node IDs. Across all
candidates, cap prediction work at 8,192 integrations/warmups and 8,192 capsule
queries, including ordinary corridor clearance with its full capsule margin
along the high ascent; exhaustion publishes nothing. This is synchronous sensor work, separate
from the live objective planner quota. Record actual work and rejection reasons.

Ordinary lower corridors retain their behavior. High flights require current
launch approval, fresh survey revalidation, the real reserve and the original
12-second flight and 90-second ground limits. Never reset the recovery deadline.
The harness switch `--terrain-flight-forecast true` explicitly enables this
candidate. It defaults off; frontend/default/frontier selection is unchanged.

## Fixed five replays, zero fresh games

First replay the affected arrival-baseline game with the candidate disabled and
the existing two exact control forks at ticks 22575 and 22965. Reproduce the two
retained native flights and require every action, observation and native state
to match their archived receipts. Add diagnostic forecast measurements without
exposing them to those controllers. Check the forecast current at each actual
launch, including its selected endpoints, plan, duration and fuel against the
retained 275→283, margin-one/height-three flights. Preserve
negative results; a model rejection is not a native impossibility claim.

Then enable the candidate in these four retained complete games, preserving all
other arguments from the arrival experiment:

1. World 1 / P2 integrated control.
2. World 3 / P1 integrated control.
3. World 3 / P1 no-stop control.
4. World 1 / P2 no-stop affected case.

Freeze source/input hashes, executable hash, exact commands and predecessor
summaries before running. No tuning, retries, fresh seed search, timer changes
or changes to combat/pod control within the experiment. Auditing existing raw
output may be repaired explicitly without rerunning games.

## Qualification

Require the native physical/capture/observer audits, correct destination footing
on completed terrain flights, fresh launch approval, bounded query work, reserve
and deadline retention. Record first observation, control and native-state
differences from the arrival baseline. Compare outcome, deaths, ship losses and
completed recovery against both that baseline and the original winning runtime.

Advancement requires retained control-game physical outcomes and a verified high
crossing followed by native claim, rebuild and boarding/completed recovery, with
survival and winning outcome retained against the original winning case. A
forecast, partial route, isolated flight or corrected counter does not qualify.
Keep all failed outcomes and hash-verified archives. Report sensor cost separately
from the objective planner's existing budget. Broader validation follows only
after this selected-case screen; there is no default promotion here.
