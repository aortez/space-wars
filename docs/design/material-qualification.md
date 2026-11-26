# Material qualification

#332 qualifies the shared material model across Spacewars, Scorched Earth and
Clock's Tanks event. This page records its pinned run of 2026-11-25: what ran,
what each game's impacts took from the model, and how the paired bot matches
moved under it. It ends with where the rest of #332's evidence is. The
[material guide](material-guide.md) is the model's entry point.

## The run

- **The tree.** The code main has from #736 on: main 949bb305, after #726,
  with the code of #727, #734 and #736. The run built it as a local commit,
  0916ffec, before #736's docs were written.
- **The build.** Rust 1.89.0 and the tests' `ci` profile, on the workstation,
  in a target directory of its own. The run measured what the model takes,
  not its cost, which #734 timed on the workstation, the Pi 4 and the Pi 5
  ([Spacewars](material-contact-charges.md#spacewars-host-330),
  [Scorched and Clock](material-contact-charges.md#game-worlds)).
- **What ran.**
  - Every test of engine-material, engine-rapier, Scorched, Clock, Spacewars,
    Material Lab and the Spacewars bots. All pass.
  - #294's host tests: the eight #604 added, seven of which #294 names. All
    pass.
  - `reference_cases`, which rewrote [the model's report](../data/material-model-v2.md)
    unchanged. For each reference case the report gives its bodies with their
    masses and inertias, its patch and sections, and its approach and load,
    beside what the event took.
  - Spacewars' native exposure measurement. Every figure
    [durability balance](../durability-balance.md) gives for today's rules
    measures the same.
  - A census of every impact in #331's 96 Scorched duels and 48 Clock fights,
    and in durability balance's 16 paired bot matches.
- **The records** stay on the owner's workstation, in
  `/home/data2/space-wars3-artifacts/material-qualification-332-20261125`.
  Its README lists each file and figure and the scripts that made them.

## The census

A probe, never committed, had each game's world record every impact it priced
or left unpriced:

- the two surfaces as the host kept them just before the charge: each one's
  material, represented volume, capacity, the work it could still take,
  thickness, depth and radius;
- the two bodies: kind, mass, inertia, velocity at the contact and spin;
- the contact's normal, point and closing speed;
- the charge: the patch the law priced across, with the section behind each
  side, the load, the outcome and the sides the host shielded, or why it went
  unpriced.

A `Charged` charge reports its patch and load
([host contract](material-contact-charges.md#host-contract)), so each record
holds all the law took. The probe priced the law again on each record and
compared the outcomes exactly. All 16,230 priced impacts reproduce: 3,532 in
Scorched, 4,479 in Clock and 8,219 in Spacewars.

## Units and health

- Lengths are world units, masses mass units and times seconds, so energy and
  work are mass units times world units squared per second squared
  ([units](material-damage.md#units-and-geometry)). The materials' values are
  calibration for play, not measured properties.
- A region's capacity is its material's toughness times its represented
  volume. A health bar is presentation: it shows the work a region can still
  take as a share of its capacity, so 1 health is 1% of it. Scorched keeps
  each tank's region and shows its integrity. Spacewars keeps health, and its
  `HealthMap` makes the region from it, so the crash rules it keeps change
  the health directly.

  | Body | Capacity | 1 health |
  | --- | --- | --- |
  | Spacewars ship | 88,455 | 884.6 |
  | Spacewars escape pod | 17,724 | 177.2 |
  | Spacewars pilot on foot | 2,632 | 26.3 |
  | Scorched tank | 37,440 | 374.4 |
  | Clock tank at 16:9 | 4.85 million | 48,481 |
  | Clock tank at 4:3 | 16.0 million | 160,000 |

- Clock's arena is Scorched's duel scaled by 3.373 at 16:9 and 4.547 at 4:3.
  Its armor and rock are copies with density divided by the scale and
  stiffness, strength and toughness multiplied by it, in volumes that grow
  with its cube, so a contact costs the share of capacity it would in a duel
  ([scale](material-guide.md#adopting-it-in-a-host)). Its speeds divided by
  the scale are the duel's.
- Each side's charge is capped by what its region can still take, and what it
  can't take becomes the event's excess
  ([failure](material-damage.md#energy-ownership-and-failure-disposition)).

## What the games' impacts took

### Scorched Earth

#331's 96 CPU duels to the first kill: on each running gear, rigid and
sprung, 24 sweep duels and 24 dodging duels at Scorched's range.

- The tanks' 3,524 impacts, 792 rigid and 2,732 sprung, are all on the hills'
  rock but four on fallen terrain. They closed at up to 8.1 u/s, with
  effective masses from a wheel's 2.9 to 42.5. The rock took their work. One
  cost a tank anything: the rigid hull's corner #331 found, 0.11 health at
  5.1 u/s ([contact review](../scorched-earth.md#contact-review-2026-11-25)).
- 30 went unpriced, all sprung tanks' budgets that didn't converge, as for
  #331. #331 counted 3,511 impacts before #726 made a burst's push on a tank
  come out of its share of the payload.
- The 1,121 shell hits cost the tanks a median 7.5 health and at most 39.2.
  1.21 million of the shells' energy stopped on bodies outside the model.
- Every duel ended with a winner. No batch or binding was refused.

### Clock's Tanks event

#331's 48 fights, 24 seeds each at 16:9 and 4:3, from the raise to the
withdrawal.

- The tanks' 4,487 impacts, 3,802 at 16:9 and 685 at 4:3, are all on the
  hills' rock, and they cost nothing. The fastest closed at 6.6 u/s in the
  duel's units. 17 went unpriced, budgets that didn't converge. Each impact's
  effective mass is about the hull's own.
- [Clock](../clock.md#tanks) gave #331's count, 4,935 impacts with 18
  unpriced. #331's probe summed the duel's list of a step's contacts on every
  tick of the event, and on ticks the duel doesn't step the list keeps the
  last step's, so some were counted again. Run beside the census, that sum
  agrees at 16:9 and adds 942 at 4:3, all while the tanks blew up, beamed up
  and withdrew.
- The fights had 439 shell hits, and each ended with a winner.

### Spacewars

Durability balance's 16 paired bot matches. In 7 of the 8 pairs both seats
play out the same.

- 8,504 impacts between bodies with regions, 8,219 priced. 281 budgets didn't
  converge, 276 of them asteroids' on planets, and 4 more asteroids' on
  planets had more rows than a budget takes.
- The sun and the planets are shielded rock. What each body's priced impacts
  took:

  | Body | Struck | Priced | Costing | Fastest, u/s | Most |
  | --- | --- | --- | --- | --- | --- |
  | pilot on foot | planets | 4,542 | 2,033 | 16.0 | 0.53 health at 16.0 u/s |
  | pilot on foot | the sun | 6 | 6 | 38.9 | 5.65 health at 38.9 u/s |
  | escape pod | planets | 152 | 8 | 25.9 | 1.71 health at 25.9 u/s |
  | escape pod | asteroids | 20 | 10 | 69.5 | 6.42 health at 69.5 u/s |
  | ship | planets | 664 | 153 | 8.1 | 0.54 health at 8.1 u/s |
  | ship | asteroids | 17 | 17 | 104.8 | 18.3 health at 101.4 u/s |
  | asteroid | planets | 2,565 | 943 | 135.6 | broke 550 |
  | asteroid | the sun | 243 | 235 | 110.7 | broke 131 |
  | asteroid | ships | 17 | 15 | 104.8 | broke 6 |
  | asteroid | asteroids | 6 | 6 | 131.3 | broke one in each |

The asteroids wear by their charges. The crash rules set what ships, pods and
pilots lose, as #330's policy
([Spacewars host](material-contact-charges.md#spacewars-host-330)), and the
census shows what charges would have taken instead:

- At speed, pods and pilots would lose far less. In six matches a pilot on
  foot hit the sun at 37.0 to 38.9 u/s, with 74.7 to 89.9 health, and died:
  the crash rule takes 4 health per u/s over 12. The law prices each at 3.2 to 5.7. An
  asteroid struck a pod at 69.5 u/s, where the crash rule's 2 health per u/s
  over 12 takes 100, and the law prices it at 6.4.
- Slow contacts would cost a little. 2,033 of the pilots' impacts on planets
  cost something, at most 0.53 health at 16 u/s. The crash rule takes nothing
  up to 12 u/s and 16 at 16 u/s.
- Ships would lose at most 0.54 health on planets, and up to 18.3 from an
  asteroid at 101 u/s. The law would have failed one ship left with 0.25
  health, twice, as it landed on a planet at 5 u/s.

That bears out #330's reason for the policy: the materials that hold weapon
fire as #330 calibrated them don't give today's crashes.

## The paired bot matches

[Durability balance](../durability-balance.md#paired-bot-matches) gives the
matrix's results on the pinned tree beside #154's. Player 1 won all 16. Every
death was player 2's pilot on foot by impact: six on the sun, above, and five
on the universe's boundary at 50 to 156 u/s, which has no region.

The same matrix before #330's first PR and after two of its steps, each
built on its own and run with the pinned tree's runner, shows where the
matches moved:

| Tree | Ordinary: deaths / pod losses / median round | Heavy: the same | Wins: player 1 / player 2 / draws |
| --- | --- | --- | --- |
| #154's tuned build | 6 / 4 / 211.08 s | 6 / 5 / 258.23 s | 10 / 6 / 0 |
| Before #669, #330's first PR | 8 / 5 / 392.69 s | 4 / 3 / 438.95 s | 11 / 3 / 2 |
| After #676, the weapons | 6 / 6 / 182.60 s | 3 / 1 / 600.00 s | 13 / 3 / 0 |
| After #711, the asteroids' wear | 8 / 8 / 192.39 s | 3 / 0 / 600.00 s | 16 / 0 / 0 |
| Pinned | as after #711 | as after #711 | 16 / 0 / 0 |

- The matches had moved from #154's record before #330 began: two pilots died
  to missiles and two to the sun's heat, and two matches were draws.
- The weapons and the asteroids' wear each moved them. Nothing after #711
  did: the pinned tree, census probe and all, plays each match as #711 does.
- The matrix is 9 distinct matches. It can't tell a seat's advantage from
  chance, as durability balance says of #154's.

## The rest of #332's evidence

| #332 asks for | Where it is |
| --- | --- |
| reference and host cases on a pinned build, with their records | this page, and [the model's report](../data/material-model-v2.md) |
| parameter choices, units and health | [the guide's values](material-guide.md#materials-and-values), [units](material-damage.md#units-and-geometry), and [units and health](#units-and-health) above |
| safe knocks and supports, damaging work, materials, extreme impacts | [the law's evidence](material-damage.md#limits-and-verification), the report, and [contact charges' evidence](material-contact-charges.md#evidence) |
| connection overload | [connection loads](material-connection-loads.md) |
| nothing charged twice | #726: Scorched's burst push on a tank comes out of the payload, and Spacewars' death impulse is a game rule ([pushes](material-guide.md#limits)) |
| partitions, contact order, step rate, a moving frame | [contact charges' acceptance](material-contact-charges.md#acceptance), and #727's host tests |
| deaths, ejection, protection, a lost firer, reset, pause, clones, replay, capacity | #727's host tests, the [guide's lifecycle](material-guide.md#adopting-it-in-a-host), and the matches above |
| caps and cost | capture's bounds and the Pi timings in [contact charges' game worlds](material-contact-charges.md#game-worlds) and its [Spacewars host](material-contact-charges.md#spacewars-host-330), budgets in [the guide's limits](material-guide.md#limits), and the caps below |
| the model, presets, guide, limits and migration matrix | [the guide](material-guide.md) |
| consumers | below |

#727's host tests:

- **Spacewars:** a pause before an asteroid's strike, a firer lost while its
  shell flies, forty strikes in one step that a budget can't take, a game
  with room for two contacts open at once, and a crash in a drifting frame
  and at 120 Hz.
- **Scorched:** drops across the terrain's seams, and a knock in a world with
  room for two contacts open at once.
- **Clock:** a fight replayed tick for tick and paused, a Tanks preview that
  resets a fight, and a firer destroyed while its last shell flies.
- **Engine:** a scene turned and spun as one about the point two boxes meet
  at, priced alike.

The caps besides capture's and the budgets':

- **Sources.** Scorched keeps at most `MAX_SHELLS`, 8, in flight. A
  Spacewars ship holds at most two rounds, and each reload costs it battery
  energy.
- **Debris.** Scorched keeps at most `MAX_TERRAIN_FRAGMENTS`, 64, fallen
  pieces. Clock's wrecks break into 6 pieces a tank, and its fights keep at
  most `MAX_GRAINS`, 192, grains. Spacewars spawns no asteroid past
  `MAX_ASTEROIDS`, 1,000, and has no cap of its own on chips and breakup
  fragments, which no region binds.

What no game exercises yet: steady supports, wood, plant tissue, welds and
dividing a region run only in the engine's and Material Lab's tests. No host
repairs a region in place. A rebuild gives a body a new region: Spacewars'
rebuilt ship is whole, and Clock's sudden-death tank comes back with a
quarter of its armor.
[What is on the model](material-guide.md#what-is-on-the-model) gives the
issues that add the rest.

Of #332's consumers, only #294 has landed, and its host tests pass on the
pinned tree. No prerequisite of the model points back into an unfinished
consumer. On 2026-11-25 #238, #243, #297, #302, #311 and #322 gained notes
on what each must add to the hosts.
