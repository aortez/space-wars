# Execution plan for the Mario world model

How [the mission](smb-mission.md) gets built, in what order, and why that order
and not the one that looks like progress. Where the reader is today, including
every measurement quoted here, is in
[the world-model reader](nes-world-model-reader.md).

Unlike the mission, this document names files, flags and current gaps on purpose:
it is about the tree as it stands, and it is expected to be edited as the work
moves. Where it disagrees with the mission's ordering, it says so.

| Phase | What it produces | Why it sits here |
| --- | --- | --- |
| 0 seam | where the model lives, what a skill may ask | cheapest moment to decide; touches every later file |
| 1 solidity | L3, learned from the game's reactions | turns every later failure from mystery into a named hole |
| 2 gyms | the harness, not the gyms | turns every later iteration from minutes into seconds |
| 3 skills | the library, and a scripted bot that finishes a level | the unit everything else is measured in |
| 4 breadth | arbitrary-level start, 32-level inventory | generality is only testable once the model makes a claim |
| 5 search | A* over skills | needs a cost model and a baseline to be compared against |
| 6 fabrication | levels we author | should be aimed at holes that turned out to be real |

## The ordering principle

Order by what unlocks measurement. Two of the seven items above move nothing: they
are instruments. Building them last is the natural mistake, because they do not
make Mario go anywhere, and the cost of that mistake is that every later hour is
spent re-running whole levels to learn one fact and every later failure stays a
mystery because no layer owns it.

## Phase 0 - Decide the seam, not the API

The reader lives in `crates/engine-nes/tests/support/smb_world_model.rs` and the
examples reach it with `#[path]`. That is honest for scaffolding and fatal for a
library with several consumers. Decide one thing: which host-side module owns L1
through L4, and what a skill is allowed to ask it.

`scenario-nes` stays cartridge-agnostic and `engine-nes` keeps no game knowledge,
so the answer is a host-side profile-and-model module above both, following the
precedent of `scenarios/falling`, which decodes a versioned observation from
cartridge RAM. Decide the boundary; do not design the API. The API should grow out
of the pipe in Phase 1, and an interface designed before a bot has cleared anything
is a guess with types on it.

This is first because it is the only decision that touches every file that comes
after, and because it is cheap exactly now: there are two consumers, the bot and
the optional test, and both are small. After the skill library exists, moving the
reader is a refactor across everything.

**Discipline:** nothing is promoted out of `tests/support` until a second consumer
needs it. That rule is what keeps this from becoming a framework.

## Phase 1 - Solidity from the game's own reactions

The highest value per hour in the mission, because the core is already giving the
answer. The frame the game stops Mario it subtracts the difference from his
velocity and his position, so the reader does not infer solidity, it reads a
verdict: command a move, compare the motion asked for with the motion produced,
attribute the difference to the cell entered, at that body height, from that side.

It goes first because every later behaviour is unmeasurable while the model cannot
say what a wall is, and because the cost of not having it is already measured: the
reaction rule bolted on instead of it moves the run between absolute X 595, 899 and
2601 as one tuning constant changes. Tuning on top of a missing fact is a slot
machine.

Exit: the tile that stops the bot today is called a wall before he touches it, and
the reader-only policy climbs it. The reaction rule is deleted, not tuned.

This is not the only possible answer and the fallback is known, so the phase cannot
become a dead end. SMB's speeds are scaled eight-bit values and a blocked frame may
not be cleanly distinguishable from a slow one; if the delta turns out to be too
noisy, read the game's own collision result out of RAM instead. The oracle only has
to be correct at decision points, not every frame, which is what makes either route
tractable.

## Phase 2 - The gym harness

Deliberately earlier than the mission's order, which lists gyms after the skill
library. A snapshot gym is nearly free, an exact rewind point plus an objective
predicate plus a frame budget plus a verdict, and once the harness exists every
later iteration costs seconds instead of a ninety-second level run. Instruments get
built before the things they measure.

A gym is a start state, an objective, a budget and a verdict, and it is cheap here
specifically because the core is deterministic and `checkpoint`/`restore` is exact
to the bit, which is pinned by an existing test.

**One opinionated call:** a committed gym stores a tape, not a savestate. A tape is
the cartridge's identity, an input prefix and a frame index, and the state is
re-derived when the gym runs. A committed savestate silently mismatches when the
core changes, which is precisely when a gym is worth having; a tape fails loudly
instead.

## Phase 3 - Skills, then a scripted bot that finishes a level

Skills are the shared unit: preconditions in affordance terms, a postcondition, and
an action envelope measured to the frame. Scripted before searching, on purpose. A
script is a specification of a behaviour and a fixed baseline; a search that
finishes a level tells us nothing about whether the skills are any good, and a
script that finishes one does.

Envelopes are measured because both of the failures in this project that looked
like model bugs were envelopes: a one-frame A press is a hop that clears nothing,
and setting a hold after the frame's buttons were built landed A one frame late and
changed the run by more than two thousand pixels.

Exit: a scripted bot finishes 1-1 end to end, and each skill has a measured height,
duration and run-up beside it.

## Phase 4 - Every level

An arbitrary-level start is a small patch job, and it unlocks two things at once:
the inventory, and underground gyms, which is how the load-bearing set's blindness
gets measured rather than argued.

The inventory walks right from each of the 32 levels and records what the reader saw
and the first place it stopped with an explanation it could not give. It sits here
rather than early because an inventory run today produces 32 copies of the same
blindness; it becomes interesting the moment the model has a claim to check. It is
also the cheapest source of gyms in the project, since every stop is a candidate.

Exit: a table of where each level stops a walk-right bot and what the model said
about the thing that stopped it.

## Phase 5 - Search over skills

A* or DFS over skills, or over macro-actions on an exact rewind, with DirtSim's
pruning rules and reward. It cannot usefully precede Phase 3 because it needs
measured envelopes for a cost model and a scripted bot for a baseline. Exact rewind
at several hundred frames per second is what makes it affordable; budget in nodes
and frames, never in wall-clock, because this machine is usually loaded.

The result worth having is the comparison the mission asks for: same gym, same start
state, script against search, and a written account of where search wins and what it
pays in frames. The expectation is that search wins on tight and ambiguous
situations and loses on long open ones, and that the crossover is worth recording
whether or not it matches.

## Phase 6 - Fabricated levels

Last, because it is the most expensive item and because its aim is only correct
after the holes are known: a fabricated level should contain the situation the model
cannot handle, not a situation we imagined. SMB's level data is compressed command
data, so authoring means emitting that format or splicing existing bytes, and that
is the spike.

Spike the mechanism early anyway, in an afternoon, and defer the investment. One
patched byte that moves a pipe proves the whole chain, cartridge patch to loaded
level to reader seeing it, and tells us the cost of the phase long before the phase
is scheduled.

## Two gaps to close on the way

- **Golden state hashes.** There are none for SMB, so a core change that quietly
  alters the game is invisible without booting a cartridge. A hash per gym, recorded
  and asserted, is what makes gyms durable across core work, and it is cheap now.
- **Clips with every claim.** `tools/nes-reference/film-smb-run.py` already turns a
  run into films, clips and an event sheet. A claim about what a bot did should
  arrive with footage of it doing it.

## What would invert this order

If solidity cannot be recovered from what the game did to the player, the plan
flips: the ROM's own collision data comes first, and the 32-level inventory moves
early, because characterising a table you cannot derive needs many levels rather
than one pipe.

## Standing disciplines

- A failure is recorded as a hole in a named layer, and the layer is named in the
  test that pins it.
- Nothing is abstracted until a second consumer needs it.
- Gyms store tapes, not savestates.
- Search budgets nodes and frames, not seconds.
- The core keeps no game knowledge, so everything from the profile up stays host
  side.
