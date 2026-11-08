# The learned transfer leg confirmed again (LF-096)

**Status: done, 2026-11-01.** The retained stack as committed after LF-095,
flying every transfer leg, played whole matches on seeds 37 to 48 under the
3-second asteroid stream and loose dirt and on fresh seeds 61 to 84 under
the default settings. Over the 72 default-settings seeds its seats hand off
6.99 times per ten minutes against the native-only seats' 6.76, its
identical first legs are 51 faster, 61 within five percent and 31 slower of
143, a typical ratio of 0.98, and its median leg is 696 ticks against
748.5; 20 of its legs run past 2,000 ticks against 16, it makes 14 budget
replans against 6, and it wins 68 of 144 decided matches. The twelve-seed
blocks still vary widely, from 6.40 to 7.93 handoffs per ten minutes
against native-only rates of 6.36 to 7.32, above the native seats in four
blocks of six. Loose dirt is level or better; the 3-second stream is the
weakest condition, 6.1 handoffs against 7.0 and a median leg of 820 against
756, on bodies the network has never been fitted with. The transfer leg is
about as good as the native law's, and the next part of the flight is the
next step.

## Why a second round

[LF-092](flight-transfer-confirmation-v1.md) confirmed the transfer leg in
whole matches under the default settings, asteroid streams at mean
intervals of 8 and 3 seconds and loose dirt, with the LF-091 network and
the legs beside the sun and the planets withheld from it. Since then
[LF-094](flight-withheld-legs-v1.md) gave the stack every leg and
[LF-095](flight-sun-detour-v1.md) refit the network to a law that no longer
crawls beside the sun, judged on seeds 1 to 24, 49 to 60 and 37 to 48 under
the default settings and the 8-second stream alone, where fits of one
recipe varied between twelve-seed blocks by as much as the fits differed.
This round plays the retained stack as committed, with no environment
override, on what those steps did not: the 3-second stream and loose dirt
on seeds 37 to 48, and two fresh blocks, seeds 61 to 72 and 73 to 84, under
the default settings (`tools/confirm_v2.sh`). A preflight on seeds 1 to 12
reproduces the LF-095 selection run of the same network byte for byte.

## Whole matches

| Whole ten-minute matches | LF-092 stack, legs withheld, asteroids 3 s | Retained stack, every leg, asteroids 3 s | LF-092 stack, legs withheld, loose dirt | Retained stack, every leg, loose dirt | Retained stack, every leg, seeds 61 to 84 |
|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 4, 14, 6 of 24 | 10, 7, 6 of 23 | 5, 12, 6 of 23 | 10, 10, 3 of 23 | 19, 21, 8 of 48 |
| Typical first-leg ratio | 0.89 | 0.97 | 1.01 | 0.94 | 0.95 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 672, 776, 756 | 820, 743.5, 756 | 682.5, 766, 757.5 | 683, 909, 757.5 | 769, 681, 775.5 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 5, 2, 1 | 5, 4, 1 | 8, 3, 2 | 1, 2, 2 | 11, 8, 8 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 5.6, 6.3, 7.0 | 6.1, 7.1, 7.0 | 6.5, 6.0, 7.1 | 8.0, 5.3, 7.1 | 6.4, 7.8, 6.8 |
| Budget replans: learned seats, their native opponents, native-only seats | 3, 3, 4 | 3, 2, 4 | 3, 3, 5 | 3, 2, 5 | 8, 1, 3 |
| Recoveries: learned seats against their native opponents | 14 against 14 | 11 against 10 | 11 against 10 | 10 against 14 | 23 against 27 |
| Decided matches: won by the learned seat, by the native seat | 11, 10 | 12, 11 | 16, 8 | 16, 8 | 19, 29 |
| Leg ticks the stack flew | 50,025 | 112,059 | 58,718 | 89,399 | 212,526 |
| Published ticks by entry: ordinary, clear alternate, blocked | not recorded | 57608, 54028, 456 | not recorded | 50852, 37976, 572 | 122673, 89862, 0 |
| Leg ticks the native controls flew after a refusal | 0 of 50,025 | 33 of 112,092 | 7 of 58,725 | 1 of 89,400 | 9 of 212,535 |
| Flown ticks spinning over 1.5 radians per second | 31% | 31% | 29% | 30% | 29% |

Loose dirt costs the stack nothing: 8.0 handoffs per ten minutes against
the native-only seats' 7.1, one leg over 2,000 ticks against two, and 16
decided matches won to 8. The 3-second stream is the weakest condition: 6.1
handoffs against 7.0, a median leg of 820 ticks against 756 and 5 legs over
2,000 ticks against 1, with the first legs from identical starts still
faster, 10 against 6 of 23. The network's object record now keeps the
nearest asteroids and fragments instead of refusing, but no training family
has such bodies in it, so the stack flies them on what it learned of
planets; a family with an asteroid stream is the remedy when the transfer
leg is next polished. The fresh seeds 61 to 84 are the stack's weakest
default-settings blocks: 6.40 and 6.47 handoffs against 6.36 and 7.32, 11
legs over 2,000 ticks against 8, 8 budget replans against 3 and 19 decided
matches won to 29, where the native opponents in the same matches handed
off 7.08 and 8.49 times per ten minutes, so a slower learned seat gives its
opponent planets.

Pooled with LF-095's seeds 1 to 60 (`tools/lf093_compare.py` over the six
blocks), the retained stack's record over 72 seeds under the default
settings is 6.99 handoffs per ten minutes against the native-only seats'
6.76 and their native opponents' 7.16, 20 legs over 2,000 ticks against 16
and 18, 14 budget replans against 6 and 6, identical first legs 51 faster,
61 within five percent and 31 slower of 143 at a typical ratio of 0.98, a
median leg of 696 ticks against 748.5, and 68 decided matches won to 76. By
block the learned seats' rate runs from 6.40 to 7.93 and the native-only
seats' from 6.36 to 7.32; the learned seats are above the native seats in
four blocks of six. The leg is about as good as the native law's, with a
tail and a replan count a little heavier and a crowded-scene gap, and the
stack flies every tick of it but 9 to 33 refused for a control state
outside its envelope.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the preflight (`lf096-preflight-1..12`), the runs (`lf096-asteroids3-37..48`,
`lf096-dirt-round-37..48`, `lf096-full-61..72`, `lf096-full-73..84`), the
test binary with its source state (`bin/engine_client-lf095-head`) and the
scripts (`tools/confirm_v2.sh`, `tools/lf096_summary.py`,
`tools/lf096_tables.py`). `tools/lf096_summary.py` writes the
[summary](data/flight-transfer-confirmation-v2-summary.json) from those
runs, and `tools/lf096_tables.py` prints the table above from it.
