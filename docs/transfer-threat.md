# Transfer exposure and the old-missile hit

## Finding

The [relative-speed experiment](transfer-speed.md) loses its ship to a missile
launched **before the transfer began**. Neither ship fires during the candidate
trip. The closing opponent is visible and our hull is weak, but those facts do
not establish that an enemy-distance retreat rule would avoid this projectile.
Keep the speed option disabled by default and investigate projectile observation
before changing steering again.

This is a read-only diagnosis of both affected world 1 P1 powered cases, with
and without the optional pursuit-health gate, and their immediate clear-entry
baselines. The two cases have identical trip measurements and are correlated
diagnostics. No simulation was rerun and no policy, physics or defaults changed.

## Damage provenance

The saved report at tick **8940** retains a damage event at **8892** and a cannon
contact at that same tick. Both name cannon damage; the damage event records
**31.1742515564** hull lost and `last_ship_lost: true`. The contact's
`last_contact_spawn_tick` is **7977**. The dense action record at 7977 contains a
P2 cannon request and no P1 cannon request. The native loss trace independently
records `last_hit_source: cannon` and `last_hit_taken_tick: 8892`.

| Recorded event | Tick | Relation to transfer |
| --- | ---: | --- |
| P2 requests the missile later identified by the contact receipt | 7977 | During the earlier escape |
| Last earlier hull damage, from laser | 8218 | Before transfer |
| Escape ends on its deadline; transfer starts | 8376 | 399 ticks after missile launch |
| Ship lost to the retained missile contact | 8892 | 516 ticks after transfer starts |
| First periodic report sample retaining that loss receipt | 8940 | Sample time, not impact time |

The missile's recorded age at contact is **915 ticks / 15.25 seconds**. Complete
per-seat action coverage over **8376–8891** proves zero laser or cannon requests
from either ship. Actual launch counters also stay at **5 for P1 / 11 for P2**
in every saved sample from 8340 through 8940, bracketing the trip. P2's cannon-hit
counter increases from 12 to 13 across the loss; P1's debris-contact count goes
from 22 to 23. A new shot during this trip is not the explanation.

The scenario [records contact spawn ticks before debris cleanup](../scenarios/spacewars/src/surface_sortie/impact.rs).
Its debris cleanup removes dead or out-of-bounds debris; it does not impose an
age-only expiration. The retained records identify this old contact but do not
contain the missile's intervening trajectory. Do not infer an orbit, ricochet,
an earlier collision with the same missile, or a successful dodge from its age.

## Opponent exposure and our weapons

The ship starts with **31.174 hull**, against **94.295 opponent hull**. The
opponent is visible on all **516** candidate transfer ticks, with no ground
occlusion. Range initially grows from 399.318 to a maximum of 419.445 at 8463,
then closes. This reversal precedes the speed limiter's first intervention at
8555; the limiter did not initiate the closing trend.

Positive opening speed below means separation is increasing. These are current
relative-motion measurements, not projectile predictions.

| Candidate observation | Tick | Opponent range | Opening speed |
| --- | ---: | ---: | ---: |
| Transfer starts | 8376 | 399.318 | +20.200 |
| First negative opening speed / maximum sampled range | 8463 | 419.445 | −0.601 |
| First inside the existing 350-unit escape-clear range | 8617 | 349.652 | −43.405 |
| First inside 300 units | 8688 | 299.537 | −39.941 |
| Defensive combat controller enters engagement range | 8748 | 259.549 | −40.062 |
| First inside laser range | 8762 | 249.526 | −44.046 |
| First inside cannon range | 8799 | 219.717 | −54.650 |
| Last intact-ship observation | 8891 | 143.403 | −33.943 |

Both weapons are ready throughout the candidate trip. The defensive combat
controller reports `route around planet` for 372 ticks and `engage ship` for
144. Transfer borrows its eligible weapon actions while retaining its own flight
controls; it does not turn toward the opponent merely because that controller
reports engagement.

The native firing window requires a bounded-lead aim error below **0.08 radians**
(about 4.6 degrees). The candidate meets that aim window on 13 ticks, all outside
weapon range. Once within laser range, its smallest absolute aim error is
**2.308 radians / 132.233 degrees**. There is no overlapping firing opportunity.
Visibility, ammunition and energy are not the blockers in this candidate.

The older last-hit record counts as recent for the first 22 transfer ticks, but
the opponent is outside the pursuit classifier's 300-unit response range then.
There is no new damage until the fatal tick. A reaction that waits for a fresh
hit cannot act before this loss. This does not show that an earlier turn toward
the opponent would be safe or effective.

## Baseline comparison

| Per-case measurement | Clear-entry baseline | Speed-limited candidate |
| --- | ---: | ---: |
| Transfer commitment ticks before loss | 809 | 516 |
| Opponent visible ticks | 800 | 516 |
| Our laser / cannon request ticks | 0 / 0 | 0 / 0 |
| Opponent laser / cannon request ticks during commitment | 17 / 1 | 0 / 0 |
| Ship-loss tick and recorded source | 9185, laser | 8892, cannon |
| New claims / completed departures during this trip | 0 / 0 | 0 / 0 |

The baseline's later firing is a different interaction. Its death cannot be used
to describe the candidate's old-projectile loss. Full match outcomes and the
stronger earlier no-escape reference remain in the speed experiment's report;
this investigation adds no evidence of improved playing strength.

## Reproduction and checks

```sh
python3 tools/analyze-transfer-threat.py \
  --summary target/transfer-speed/v1/summary.json \
  --out /tmp/transfer-threat-v1.json
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

Use a fresh output path. The analyzer requires completed summaries and checks
the prior summary and all **16 consumed run files** against retained SHA-256
hashes; it records the current summary's hash. It requires dense
action coverage for both seats, preserves exact loss/launch records and report
samples, and rejects stale contact receipts. The [manifest](data/transfer-threat-v1.json)
records source hashes, per-case measurements and native code hashes. Its
[compressed evidence](data/transfer-threat-v1.json.gz) contains the extracted
observations and records used by the diagnosis. Full streams remain at their
hashed local paths.

All **740 Python tests** pass, including nine new tests for relative-motion
signs, frame invariance, bounded aim lead, weapon gates, contact provenance,
dense coverage, action decoding, sample bracketing and input integrity. The
Python aim calculation approximates native f32 arithmetic; the candidate's
in-range error is far outside the firing threshold. No Rust source changed.

## Next bounded change

Add an opt-in diagnostic for nearby projectile motion before implementing an
avoidance response. Record stable identity, launch tick, radius, position and
velocity relative to the controlled ship, with explicit range/count bounds and
deterministic ordering. The generic ship observation already has a bounded
hazard selector; inspect reuse with the material scenario's physical frames.
The current mission combat observation has opponent motion and last-hit history,
but no incoming-projectile track.

First replay these retained cases with the diagnostic enabled and require
unchanged controls, state, native sensing and planner quotas. Identify whether
the missile can be observed with useful warning time and how its approach
compares with ship turning/braking authority. Only then freeze a bounded dodge
hypothesis, preserving planet clearance, arrival gates and the original transfer
deadline. A proximity-only opponent constraint or extra post-damage retreat is
not yet supported as a fix for this specific loss.
