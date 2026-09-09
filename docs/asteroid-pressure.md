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

## Validation — 2026-09-09

Gameplay source is `d20b8b1c81800395cd255237fcd90a16c36489a4`. Final binaries
completed 200 three-minute simulations across desktop and Raspberry Pi 5:
ten simulated hours, excluding earlier investigation runs and the ordinary
Spacewars baseline suites.

| Suite | Desktop | Pi 5 | Acceptance |
| --- | ---: | ---: | --- |
| Controlled impact recovery | 36/36 | 36/36 | Recover, rebuild and depart; physical audit passes |
| Pod/ship jetpack routes and flag approaches, including former stalls | 16/16 | 16/16 | Each case's route/recovery objective and physical audit pass |
| Sustained asteroid pressure | 48/48 | 48/48 | Full duration, physical/material audits and hazard coverage pass |
| Capture sorties inside the pressure matrix | 43/48 | 45/48 | Capture and departure complete before the deadline |

The seed-7/P1 sideways pod now exits at tick 2394 on both machines. It completes
the enemy-flag/rebuild/departure journey at tick 5532 on desktop and 5533 on Pi.
The original Pi cover-retry failure (seed 7, subject P2, unmirrored, asteroids
Off) now lands at tick 3535, claims at 3728 and completes departure at 4244,
using two cover replans. It later loses its ship and completes recovery too.

Each pressure row below contains eight arrangements per platform. These are
small diagnostic samples, not estimates of a balanced win rate; heavier rocks
can also disrupt the opponent.

| Mean arrival interval / strength | Desktop capture completions | Pi capture completions |
| --- | ---: | ---: |
| Off / Mixed | 7/8 | 7/8 |
| 8 seconds / Mixed | 8/8 | 8/8 |
| 3 seconds / Mixed | 7/8 | 8/8 |
| 1 second / Mixed | 6/8 | 7/8 |
| 3 seconds / Light | 7/8 | 7/8 |
| 3 seconds / Heavy | 8/8 | 8/8 |

The eight incomplete capture sorties comprise three ship losses, three
exhausted approach budgets, and two occurrences of inaccessible hatch footing
in the same Light-pressure arrangement. Ten completed capture sorties later
encounter a terminal ground-route or hatch-access failure during recovery.
One desktop case stops making progress on its ground route; the other reports
say no measured route or no reachable standing site with hatch access. These
remain explicit failures. Other late ship losses are still recovering when
the three-minute observation ends. Capture success therefore does not imply
indefinite recovery success on continuously damaged ground.

The enabled streams record 593 vehicle contacts and 2,732 queued terrain edits
across both platforms. The 1-second setting exercises the live-arrival cap
(two skipped spawns on desktop, one on Pi); the Pi also records one terrain-edit
budget skip. Natural ownership neutralizations occur, including in Off runs
where weapons still damage material. These are observed contacts and gameplay
events, not scripted hits on the player or flag.

The largest per-run Pi step p95 is 0.406 ms, largest single measured step
12.12 ms, and largest AI p95 0.338 ms. These headless timings exclude rendering;
they do not establish a multi-planet frame budget or bound every survey spike.

The workspace/all-target suite passes 1,057 tests with 22 display-dependent
tests ignored. After the final launcher mapping fix, all 237 client tests pass
again, and six material UI workflows pass under Xvfb in both renderers. Settings,
launch, pause and restart are exercised. The ordinary navigation and strategy
baselines still match their six and twelve episodes. Workspace Clippy completes
with existing warnings, and formatting/diff checks pass.

Raw reports, rejected controller experiments, binary hashes, logs and image
provenance are retained outside the repository at:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/asteroid-pressure-20260909/
```

Final evidence directories are `final-desktop-pressure`, `final-pi-pressure`,
`desktop-impact`, `final-pi-impact`, `verified-desktop-jetpack` and
`final-pi-jetpack`. The final native combat and recovery executables are
byte-identical to the executables used in their earlier named result directories;
the jetpack matrix was rerun with the rebuilt final executable. Aggregated
pressure results are in `aggregate-results.json`.

## Pi deployment and playtest

The Yocto image for `d20b8b1` was deployed to `spacewars.local`, switching the
active root from slot B (`/dev/sda3`) to slot A (`/dev/sda2`). The installed
`engine-client` matches the executable extracted from the archived image:

```text
image SHA-256:  4e958e817505d58e5ec765c255481277dd04f98f528b72a06b5f3c656316303f
client SHA-256: 01e1fe62cb23e364d81fcc921fb1da009ac380612b5c7bb8f0c60d3d6e4d5341
```

A live three-minute duel used Capture, 8/4-second combat breaks and Mixed
asteroids at a 3-second mean. Four screenshots/status samples at 30, 75, 120
and 180 seconds report 59.8–60.1 FPS/UPS with no service restarts. P1 had claimed
and departed by the first sample; the second shows excavated ground and a
neutral planet. P2 is landing its escape pod at the final sample. That last
recovery is still in progress; the live check does not claim a completed rebuild.
The launcher layout was also inspected on the Pi's 800×480 display.

The Pi is left in a fresh, paused `spacewars-terrain-combat` round: P1 human,
P2 Capture, 8/4-second combat breaks, and Mixed asteroids at the gentler 8-second
mean. Start resumes. A tipped pod can use **A + d-pad down** (keyboard
**Space + S**) for the shared recovery lift, then turn upright. Release before
trying another lift. Arrivals can be disabled or increased in Launcher Settings.

This closes the reproduced pod/cover-stall slice and establishes the sustained
pressure test bed. The next navigation work is the saved damaged-ground and
hatch failures above, followed by a controlled two-planet objective loop.
Generated multi-planet matches, lethal pod/spaceling damage and world-scale
hazard balance remain outside this checkpoint. Nothing has been merged or pushed.
