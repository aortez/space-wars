# See Mario, be Mario: mission

**One bot, written against one model interface, plays every level Super Mario
Bros. has and every level we invent, and every behaviour it owns was proven
somewhere smaller than a level first.**

That is the mission. What is true of the code today is not in this document: that
belongs to [the world-model reader](nes-world-model-reader.md), where a claim
about the reader gets measured and defended. This document changes only when the
direction does.

## Why this is the mission

The bot is the instrument, not the product. A bot that finishes a level having
memorised it has told us nothing; a bot that finishes a level it has never seen,
from a model that was never told the level's shape, has told us the model is
right. The deliverable is the model, and the bot is how the model gets measured,
the way a native scenario is how a physics engine gets measured here.

Three consequences, which are the culture of this work rather than preferences:

- **A failure is a hole in a named layer**, never "the bot is dumb". Naming the
  layer is what makes a fix findable and a regression writable.
- **A behaviour is unfinished until it has a practice world smaller than a level**
  that it passes and fails in on demand.
- **Generality is the point**, so a level we fabricate is worth more than one
  Nintendo authored: the first tests the model, the second tests how well we read
  a cartridge.

## The ladder

Each layer asks the layer below a fixed set of questions and may not reach past
it: a bot does not read RAM, a planner does not press buttons. That rule is what
makes "a hole in a named layer" an actionable sentence.

| Layer | Promise |
| --- | --- |
| L0 core | NES hardware: memory, picture, controllers, and an exact rewind |
| L1 profile | one cartridge's address map and decoders, keyed on the cartridge's own identity |
| L2 frame | tile identity that survives a bank switch or a re-release, stitched into a position that survives a scroll |
| L3 ground truth | what the game resists: which cell, which direction, which body height, from which side |
| L4 affordances | the situations a body can act on: gap, wall, ledge, enemy in reach, pipe mouth, stair, item |
| L5 skills | named macro-actions, each with preconditions, a postcondition, and an action envelope measured to the frame |
| L6 route | which skill next, and in what order, across a level |
| L7 game | the whole cartridge: every level, every way between them, lives and continues |

L3 is the load-bearing gap. Everything above it is either missing or resting on a
guess, and a guess about what is solid is the guess that leaves a bot standing
still in front of something it can see.

## Solidity is an answer, not an inference

L2 records what is drawn; L3 has to record what resists. The game says that out
loud every frame it stops him, by taking the difference out of his velocity and
his position, so the direction is to run the experiment instead of making the
inference: command a move, compare the motion asked for with the motion the core
produced, and attribute the difference to the cell he entered.

That is the first item because it pays for several holes at once. A one-way
platform is solid from above and not from below; a block is a wall that changes
identity when hit; a hill is opaque and never a wall; a pipe is a surface in one
direction and a wall in another. None of those is a property of a tile picture, so
a list of tile ids learned from one level can never contain them. A measurement of
what the game did to the player is a statement about the core's physics, which is
why it carries into a level the bot has never seen and into one we invented.

## Bots are a library, not a program

Skills are the shared unit: preconditions in L4 terms, a postcondition, and an
action envelope measured to the frame rather than assumed, because a skill whose
envelope is guessed fails in a way that looks like a broken model. Scripted and
searching bots are then two consumers of one library, a hand-written sequence of
skills and a search over the same skills or over macro-actions on an exact rewind.
Only the chooser differs, and that has to be true in the code rather than in the
prose, because the interesting question is a comparison: same start state, same
objective, where does search beat a script and what does it pay in frames to win.

## Every obstacle leaves a gym behind

A gym is a start state, an objective, a frame budget and a verdict. It is cheap
here specifically because the core is deterministic and the rewind is exact: a gym
is a state plus a predicate, not a recording.

| Grade | How it is built | What it can hold |
| --- | --- | --- |
| snapshot | an exact rewind point at a real frame, plus an objective | an obstacle the game really has |
| RAM patch | position, lives, power-up and enemy slots written at a rewind point | variants of a real obstacle |
| fabricated ROM | patched program data, re-read as a cartridge | obstacles the game does not contain |

The loop is this mission's metabolism. A bot fails somewhere in a level, so cut a
gym at that frame: the failure is isolated, reproducible and seconds long. Fix or
add a skill, promote the gym to a permanent test, re-measure the level. Every
unsolved obstacle leaves a gym behind, and a level turns out to be a gym that is
long.

Fabricated levels are the same idea at its analytical extreme: a level that is
only the situation being studied, pipes at three heights, a gap at three widths, an
enemy released on a one-tile ledge. They are also how a hole in the model gets
found early, since a fabricated level can be authored to contain exactly the thing
the reader cannot classify.

## The model is only real at 32 levels

An inventory harness: for every level on the cartridge, start there, walk right,
and record what the reader saw and the first place it stopped with an explanation
it could not give. Until that table exists, every claim about generality is an
assumption. The inventory is L3's acceptance test, and every level it cannot
classify owes a gym.

## Order of attack

1. **Solidity from the game's own reactions.** Done when the bot climbs the first
   thing it currently walks into and the stopgap reaction rule is deleted rather
   than tuned.
2. **Start a run in an arbitrary level, then inventory all of them.** Done when
   there is a table of where each level stops a walk-right bot and what the model
   said about the thing that stopped it.
3. **A skill library with measured envelopes.** Done when a scripted bot finishes
   a level end to end.
4. **A gym for every failure the first two steps produced.** Done when each gym
   passes with its skill and fails without it.
5. **Search over skills.** Done when the same gyms pass under search with a frame
   and node cost recorded beside each one.
6. **Fabricated levels.** Done when a gym for an obstacle the cartridge never
   contained is cleared by an unchanged library.

## Boundaries

- No policy network in this mission. A trained policy is a later consumer of the
  same L4, and the sense vector for it already exists in DirtSim.
- No second cartridge until the first is cleared end to end, though the profile
  layer exists so that adding one is a table rather than a rewrite.
- The core keeps no game knowledge. Everything from L1 up lives above it, so the
  emulator stays an emulator.
- Films and stills are how a run gets reported. A claim about what a bot did should
  arrive with a clip of it doing it.
