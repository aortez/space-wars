# Landing recovery and sustained asteroid pressure

This slice adds a shared recovery lift for tipped pods, a bounded commitment
policy for capture descents, and a repeatable environmental asteroid stream
in the one-planet material combat and duel scenes. Ordinary Spacewars retains
its existing controllers and asteroid generator.

## Pod recovery

The seed-7/P1 pod start at offset -0.5 reproduces a pod resting about 102 degrees
from upright. Its hull is against retained material, neither rear foot supports
it, and its relative speed and spin are almost zero. The old controller stops
after fifteen seconds without progress. Eight ordinary-control experiments
(turning both ways, releasing the brake, and thrust bursts) remain trapped.

Brake plus thrust now redirects the pod's existing engine outward for at most
1.5 seconds. Starting requires a live occupied pod, armed controls, actual hull
or foot contact with retained planetary material, more than 30 degrees of tilt,
low surface-relative motion and low spin. A flying pod or detached debris cannot
authorize a lift. Releasing the chord stops the lift and rearms it. Holding it
cannot start another lift. Turning, braking, gravity and collisions remain
physical; the lift supplies no pose snap or permission to land or exit.

`recover_ship_v4` observes this equipment state and uses the same chord. It
retains the existing no-progress, overall recovery and ground-route deadlines.
The HUD explains the chord when a tipped pod can use it.

## Landing commitment

The saved Pi seed-7/P2 capture run abandoned four sound sites as the opponent
moved and changed their cover. It exhausted retries after about fifty seconds.
Current `tactical_sortie_v3` still seeks cover before descent. A high approach
is abandoned only after two continuous seconds of exposed ground; final descent
below 35 units remains committed. Cover searches have their own bounded budget
of eight, separate from four failed/invalidated landing approaches. The overall
150-second mission budget stays fixed. Ten seconds without descent progress
triggers a replan. Progress compares successive one-second windows so recovery
from a knockback does not require beating the pre-impact closest distance.

A transient obstruction without a material revision gets up to one second to
clear while the ship holds away from the ground. Removed/revised ground is
revalidated immediately. Historical `tactical_sortie_v1` keeps its old policy.

Comparisons rejected unconditional commitment to an exposed approach: that
removed cover thrashing but caused extra ship losses. Combat losses and
exhausted approach budgets remain separate outcomes in the final test bed.

## Environmental stream

Launcher Settings in material combat and duel expose **Asteroid arrivals**
(Off, mean 8/3/1 seconds) and **Asteroid strength** (Light, Mixed, Heavy).
Existing settings default to Off. The mean interval can also be set to an
integer from 1 to 60 seconds by the headless runner.

| Strength | Radius, world units | Initial speed, units/second |
| --- | ---: | ---: |
| Light | 1.5–2.5 | 25–60 |
| Mixed | 2–4 | 40–120 |
| Heavy | 3–5 | 80–160 |

At each fixed step an independent seed/tick random draw decides whether to
spawn. Arrivals start 220 units above the planet's nominal radius, distributed
around the planet, with a broad impact parameter that includes misses. The
generator never reads a player's location, selected site, flag or mission phase.
This is a controlled one-planet environmental distribution, not yet generated
world hazard balancing.

The rocks use ordinary asteroid collision, damage, gravity and breakup. Fresh
material contacts closing at least 12 units/second can queue terrain damage:
brush radius is the incoming radius rounded up and bounded to 1–5 cells; damage
work is closing speed times radius, bounded to 240. At most two such edits are
queued per step. Contact-local coordinates preserve the actual hit surface,
including detached fragments. Resting support does not repeatedly excavate.
Normal terrain commit, support invalidation and flag neutralization rules apply.
Pods and spacelings retain the material prototype's existing invulnerability;
these tests exercise their motion and support loss, rather than lethal damage.

The stream retains at most 24 live arrivals. Arrivals expire after 30 seconds;
expiration does not create additional breakup debris. Spawn skips, expirations,
contacts, terrain edits and budget skips are recorded. Event buffers contain
only the latest step; the external runner archives the complete history.

## Test bed

```sh
cargo build --locked --release -p spacewars-ai --example surface_combat_soak
python3 tools/run-asteroid-pressure-trials.py \
  --binary target/release/examples/surface_combat_soak --out /tmp/asteroid-pressure
```

The full matrix contains 48 three-minute runs: seeds 7/42, both subject seats,
both initial arrangements, and six pressure settings. Four settings compare
frequency with Mixed strength (Off/8/3/1 seconds); three compare strength at
the same 3-second mean. `--quick` runs one arrangement at all six settings.
The runner also supports SSH and retains every report when a case fails.

Each run uses Capture against an armed interceptor with 8/4-second combat
breaks. Reports separate capture completion, ship loss, recovery, ownership,
terrain removal, actual asteroid contacts, one-second physical/material audits,
and step/policy timing. A clean physics run does not count as a successful
mission. The diagnostic runner returns nonzero for audit, report or hazard
coverage failures; gameplay failures remain explicit report fields.

Validation and deployment results will be recorded after the final runs.
