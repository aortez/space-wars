# Claim footing recovery

This follows [ground get-up recovery](ground-getup-recovery.md). The shared
capture/recovery ground task is now `ground_navigation_v7`.

## Recorded failure and behavior

The Pi seed-42/P1 mirrored duel with 3-second Mixed asteroid arrivals reaches
an enemy flag, lowers it, and starts raising a replacement. An asteroid edits
planet 0 at tick 9343 (155.72 seconds). After another interrupted raise, the
spaceling has balanced contact with retained material but its precise contact
point supplies no occupied anchor cell. The old bot interprets an absent flag
as arrival and remains on a neutral planet through the 180-second cutoff.

A supported bot now waits half a second to distinguish transient contact loss
from unusable claim footing. At the existing half-second ground survey cadence,
an additive sensor offers at most sixteen nearby standing positions, between
1.25 and 8 units away. They use the existing retained-material ray and standing
capsule clearance measurements, plus the same cell inset/occupancy check as a
real flag anchor. If a flag still exists, candidate anchors remain within its
interaction range. Detached debris supplies neither the floor nor eligibility.
The survey is read-only and unavailable while physics queries are dirty.

The AI chooses a measured walk/jump route of at most twelve units. Ordinary
inputs move the spaceling; reaching a proposed point only begins a settling
wait. Actual ready, raising, lowering, contested or secured claim status ends
relocation. The authoritative human support, balance, relative-speed, flag
range, ownership and three-second claim rules are unchanged.

A proposal loses eligibility on support loss or terrain revision changes. The
bot can try at most four distinct nearby proposals within twelve seconds;
it waits up to two seconds at each endpoint for real eligibility. The existing
ninety-second ground-task deadline also remains in force. Failure is reported
to the containing capture/recovery task, without teleportation, terrain edits,
free claims or a restarted mission deadline. Telemetry records relocation
attempts and the currently proposed local standing center.

## Reproduction and acceptance

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror true --mode duel --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --require-claim-recovery true \
  --out /tmp/claim-footing-recovery
```

The extra assertion requires the designated subject to attempt relocation and
subsequently finish a real claim. Reports retain the relocation and completed
claim ticks separately from ordinary capture-sortie departure counts. Physics
and gameplay success remain separate checks. This saved failure is Pi-specific;
its desktop trajectory is different and need not trigger that assertion.

The development Pi replay preserves all per-second motion hashes through second
157. It selects new footing at 157.52 seconds, resumes raising at 158.48, survives
another brief interruption and completes its flag at 162.95. The old run still
has no flag at 180 seconds. This proves recovery of the claim; the bot is still
returning to its ship at cutoff, so it does not establish a second completed
capture-and-departure sortie.

Artifacts, scripts, traces, frame JSON and build manifests:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/claim-footing-recovery-20260909/
```

## Testing as development proceeds

1. Save and replay a specific physical failure on the affected platform; compare
   the unchanged lead-in and the actual completed objective afterward.
2. Check decision contracts with synthetic observations: transient waits, fresh
   support, candidate identity/geometry, dirty queries, destruction, replay and
   clone behavior, actual claim progress, contests, retry limits and deadlines.
3. Exercise ordinary physics through the existing landing, mining, claim,
   jetpack, impact and ship-recovery fixtures, then run three-minute missions.
4. Keep physical audits (finite motion, bounded speeds, material accounting)
   separate from outcomes (claims, both departures, recovery and blocked tasks).
5. Verify the full release suite and frozen ordinary-game navigation/strategy
   baselines, then deploy one identified build and check live Pi behavior.

Final validation and deployment results follow after the frozen source build.
