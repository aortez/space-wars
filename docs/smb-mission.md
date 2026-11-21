# See Mario, be Mario: mission

The [world-model reader](nes-world-model-reader.md) can say what is drawn and
where Mario is. It cannot yet say what will stop him, and that is the difference
between a bot that walks into a pipe at absolute X 899 for six hundred frames and
one that climbs it. This document sets the direction for closing that gap and for
what gets built on the answer: bots that share subroutines, practice worlds small
enough to perfect one behaviour at a time, and a search layer over both.

The mission in one line: **one bot, written against one model interface, plays
every level Super Mario Bros. has and every level we invent, and every behaviour
it owns was proven somewhere smaller than a level first.**

## The ladder

Each layer asks the layer below a fixed set of questions and is not allowed to
reach past it. A bot at L5 does not read RAM; a planner at L6 does not press
buttons. Breaking the model down further is what makes the holes nameable, and
the holes are the whole reason a bot stops somewhere.

| Layer | Promise | Status |
| --- | --- | --- |
| L0 core | NES hardware: RAM, PPU, controllers, checkpoint, restore | done, cartridge-agnostic |
| L1 profile | this ROM's address map and decoders, keyed on its hash | done for SMB1, fails closed on another revision |
| L2 frame | tile identity by CHR pattern hash, stitched into absolute X | done, one screen ahead of the camera |
| L3 ground truth | what the game resists, in every direction, from every side | **missing; this is the blocker** |
| L4 affordances | gap, wall, ledge, enemy-in-reach, pipe mouth, stair | three of them, all feet-row only |
| L5 skills | named macro-actions with preconditions and a measured envelope | one jump, inline in an example |
| L6 route | which skill, in what order, across a level | missing; this is where search lives |
| L7 game | 32 levels, warps, lives, continues | missing; the bot has never finished a level |

## The hole to fill first: solidity as an answer, not an inference

L2 records what is drawn. L3 has to record what resists, and the game already
answers that question out loud every frame it stops him: it takes the difference
out of his velocity and his position. So run the experiment instead of the
inference — command a move, compare the intended motion with the motion the core
actually produced, and attribute the difference to the cell he entered, at that
body height, from that side.

That single experiment is what a pipe needs, and it comes with three more for
free: a one-way platform is solid from above and not from below, a `?` block is a
wall that changes identity when hit, and a hill is opaque and never a wall, which
is traps 6 and 7 resolved by measurement rather than by a growing list of tile
ids. It is a statement about the core's physics rather than about 1-1, so it
survives into 1-2 and into a level we fabricate.

Deliverable: a solidity map keyed by absolute X, cell, direction and body height,
filled in as the camera moves, plus a test that the tile that stopped the bot at
899 is called a wall before he touches it. That test replaces the reflex rule
rather than joining it; the `--reflex-look` sweep, where one tuning constant moves
the run between 899 and 2601, is what an unmodelled wall costs.

## Bots are a library, not a program

Skills at L5 are the shared unit. Each declares preconditions in L4 terms, an
action envelope measured to the frame, and a postcondition. The envelope is not
detail: a one-frame A press is a hop that clears nothing, and setting a hold one
frame late changed a run by two thousand pixels.

Two kinds of consumer, one library:

- **Scripted bots** run a hand-written sequence of skills. Cheap, reproducible,
  and the regression net for everything else.
- **Searching bots** run A* or DFS over the same skills, or over macro-actions on
  `checkpoint`/`restore`, with DirtSim's pruning rules and reward.

The difference is only who picks the next skill, and that has to be true in the
code and not just in the prose. The check is the same library, the same start
state, the same gym, run by a script and by a search, with a written account of
where the search wins and what it paid in frames to win.

## Gyms

A gym is a start state, an objective, a frame budget and a pass or fail. Three
grades, cheapest first:

| Grade | How it is built | What it can hold | Cost |
| --- | --- | --- | --- |
| snapshot | checkpoint at a real frame, plus an objective | an obstacle the game really has | free; checkpoint and restore are already exact |
| RAM patch | write position, lives, power-up, enemy slots at a checkpoint | variants of a real obstacle | small, but the game spawns enemies itself |
| fabricated ROM | patch PRG bytes and re-parse the image | obstacles SMB1 does not contain | a spike; SMB level data is compressed |

The loop is the point. A bot fails somewhere in a level, so cut a snapshot gym at
that frame: the failure is now isolated, reproducible and two seconds long. Fix
or add a skill, promote the gym to a permanent test, re-measure the level run.
Every unsolved obstacle leaves a gym behind, and a level is then just a gym that
is long.

Fabricated levels are the analytical end of the same idea. When a situation is
too rare or too entangled to study inside a real level, build a level that is
only that situation: three pipes of different heights, a gap at three widths, a
goomba released on a one-tile ledge. A model that only works on Nintendo's
authoring is a model of 1-1.

## All the levels

A level inventory harness: for each of the 32 levels, start there, walk right, and
report what the reader saw — tile ids seen, gaps, pipes, stairs, enemies, and the
first place the bot stopped with an explanation the model could not give. That is
the first real measurement of whether the model is level-independent or 1-1-shaped;
today the load-bearing set is literally 1-1's vocabulary and an underground level
is drawn with ids it never saw.

This needs a way to start a run in an arbitrary level, which does not exist yet.
The inventory is then the acceptance test for L3: any level whose obstacles the
reader cannot classify before touching him is a hole with a name and a gym.

## Order

1. Solidity oracle and the pipe test. Done when the reader-only bot clears the
   first pipe and `--reflex` is deleted rather than tuned.
2. Arbitrary-level start and the 32-level inventory. Done when there is a table of
   where each level stops a walk-right bot and what the model said about it.
3. Skill library lifted out of the example, with measured envelopes. Done when a
   scripted bot finishes 1-1.
4. Snapshot gyms for every failure the first two produced. Done when each gym
   passes with its skill and fails without it.
5. A* over skills. Done when the same gyms pass under search, with a frame and
   node cost next to each one.
6. Fabricated levels. Done when a gym for an obstacle SMB1 never contained is
   cleared by an unchanged library.

## Non-goals

No policy network in this mission; a trained policy is a later consumer of the
same L4 and DirtSim already has the sense vector for it. No second cartridge
until SMB1 is cleared end to end, though the profile shape already anticipates
one. `engine-nes` keeps no game knowledge: everything above L1 stays host side.
Films and stills stay the reporting medium, so every claim above should arrive
with a clip of the run that made it.

## Where the floor is

Measured on `data/roms/smb.nes`, release build, repeatable to the frame: gameplay
mode at frame 120, the level hands the player over at 287, the reader-only policy
reaches absolute X 899 and dies never, the reflex policy reaches 2601 and dies
twice. Four optional tests pass with a cartridge present and skip without one.
