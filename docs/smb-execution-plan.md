# Execution plan for the Mario world model

How [the mission](smb-mission.md) gets built, in what order, and why that order
and not the one that looks like progress. Where the reader is today, including
every measurement quoted here, is in
[the world-model reader](nes-world-model-reader.md).

Unlike the mission, this document names files, flags and current gaps on purpose:
it is about the tree as it stands, and it is expected to be edited as the work
moves. Where it disagrees with the mission's ordering, it says so.

| Phase | What it produces | Why it sits here | Where it stands |
| --- | --- | --- | --- |
| 0 seam | where the model lives, what a skill may ask | cheapest moment to decide; touches every later file | done: both crates exist |
| 1 solidity | L3, learned from the game's reactions | turns every later failure from mystery into a named hole | met; the stop it could not name turned out not to be a wall |
| 2 gyms | the harness, not the gyms | turns every later iteration from minutes into seconds | met: three gyms, one of them winnable |
| 3 skills | the library, and a scripted bot that finishes a level | the unit everything else is measured in | started: two measured skills, and the script wins `pit-1379` |
| 4 breadth | arbitrary-level start, 32-level inventory | generality is only testable once the model makes a claim | not started |
| 5 search | A* over skills | needs a cost model and a baseline to be compared against | not started |
| 6 fabrication | levels we author | should be aimed at holes that turned out to be real | not started |

The two instrument phases were the only ones with anything to show for themselves
for most of this, which is what the ordering asked for: Phase 1 moved him from 899 to
1553 by deleting a rule, and Phase 2 moved him nowhere at all and cost six replays to
find that out. What Phase 2's instrument bought was an explanation rather than
distance — the stop every run ends at is a pit, the reader had been able to see it the
whole time, and there is a measured window for the jump that clears it — and Phase 3
has since spent that explanation: a two-skill script crosses the pit and reaches
absolute X 1599 on the tape where the reflex stops at 1544, which is the first
distance in this project that a library rather than a deleted rule produced.

## The ordering principle

Order by what unlocks measurement. Two of the seven items above move nothing: they
are instruments. Building them last is the natural mistake, because they do not
make Mario go anywhere, and the cost of that mistake is that every later hour is
spent re-running whole levels to learn one fact and every later failure stays a
mystery because no layer owns it.

## Phase 0 - Decide the seam, not the API

Decided, and now carried out: the gate SMB-006 set fired when the Phase 2 harness
arrived, and the reader left `engine-nes/tests/support`. L1 through L4 are
`crates/smb-model`, L5 and the harness are `crates/smb-ai`, and the five
`world_model_*` examples and the optional test went with them, renamed to
`probe`, `tiles`, `rollout`, `bot`, `solidity` and `gym`. SMB-018 records the move.

That is the split this repository already uses for its own game: the world in
`scenario-spacewars`, the players in `spacewars-ai`. Here the cartridge is the
world, so the two new crates are a model of it and the players of it. Neither is a
scenario, and neither joins `scenarios/`.

| Layer | Owner | Why there |
| --- | --- | --- |
| core | `engine-nes` | hardware only; already true, and it stays true |
| profile, frame, ground truth, affordances | `smb-model` | one cartridge's knowledge: above the core, below the bots |
| skills, route, game, gyms, search | `smb-ai` | the consumer that decides whether the model is any good |
| frame and action envelope, observation | `scenario-nes` | cartridge-agnostic: it may host a model, never name one |

`smb-model` depends on `engine-nes` and nothing else, which is what lets the
examples, the optional test, the client and the bots all ask one reader rather than
three. It borrows the machine and never writes it; the only code allowed to patch
cartridge RAM is Phase 4's arbitrary-level start and Phase 6's fabricated level,
and both are named for what they patch. Both crates join the workspace members and
the NES CI job's package list, which is where the reader's checks already run.

**What a skill may ask.** Skills speak affordance vocabulary only, so a skill's
entire question set is four questions:

- where my body is, how fast it is moving, which way it faces, whether it is
  standing, and whether the player is in control of it at all;
- whether a named cell resists, at a body height, from a side, and what is drawn
  there;
- which affordance holds right now, each naming the cell it is about: gap, wall,
  ledge, obstruction in reach, pipe mouth, stair, item;
- whether the world has changed in a way that ends this skill's postcondition.

A skill may not ask for an address, a tile number, a pattern hash or a PPU
register. Those are profile and frame vocabulary, and a skill that reaches for them
has stopped being portable to a fabricated level or a second cartridge. The
interface itself stays undesigned: those four questions are the boundary, and the
signatures should come out of the pipe in Phase 1.

**Identity.** A profile binds to the core's own `CartridgeIdentity`, a length and a
hash over the image, because that is what a running machine can answer cheaply, and
it refuses to answer at all for a cartridge the profile was not written for. The
SHA-256 stays what documents and tests quote, because that is what a person can
check against a file.

**Deferred, with its trigger named.** `scenario-nes`'s observation grows an
optional profile payload when the client wants to draw the model. Nothing on the
host consumes an observation today, so the payload would be a shape with no user.

This is first because it is the only decision that touches every file that comes
after, and because it is cheap exactly now: there are two consumers, the bot and
the optional test, and both are small. After the skill library exists, moving the
reader is a refactor across everything.

**Discipline:** nothing is promoted out of `tests/support` until a second consumer
needs it. That rule is what keeps this from becoming a framework, and it is the
reason the move happened at Phase 2 rather than at Phase 0, when promoting the
reader would have been a framework with one user.

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

**Met, and the exit turned out to be the easy half.** The refusal is recorded against
the CHR pattern that made it, so the pipe that stopped the reader-only policy at
absolute X 899 is called a wall nine frames and seven pixels before contact on the next
pass, the bot climbs it, and the run reaches 1553. The reaction rule is deleted along
with its tuning constant, and the distance the ledger is trusted ahead is not a knob:
the run is identical from 0 to 5 columns. What the exit did not anticipate is a stop
with no artwork attached to it. Three lives in a row now end at the camera limit near
absolute X 1553, where he is falling rather than blocked, so nothing can be attributed
to a cell and the model has nothing to say. That is the first gym.

It is also, it turns out, not a solidity problem at all. Phase 2's instrument named the
stop — a pit 48 pixels wide at world pixel 1379 — and nothing was ever refused there
because nothing resisted him. The ledger was never the gap; a policy that spends a jump
was.

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

Phase 1 adds a requirement this ordering did not anticipate: a start state is now two
things, the machine's frame and the model's memory of what it has been refused, and the
tape has to say which of the two it carries. Restoring the frame and handing over a cold
ledger is a different gym from restoring the frame and handing over the lessons an
earlier run learned, and the difference between them is the experiment worth running.

**Met, and the harness is library code rather than an example.** `Gym::run` replays a
tape and reports a verdict; `Gym::record` cuts a new one and replays it before handing
it back, so the bytes about to be committed are the bytes that produced the verdict
printed beside them. That is why a tape is recorded and never authored: an authored
run-up is a guess about what the bot did, and a gym whose run-up is a guess measures a
situation nobody met. A gym names its `LedgerSeed`, so the two start states Phase 1
split are two `const`s with one tape between them rather than one gym with a hidden
input, and the first pair measured the finding described in
[the reader](nes-world-model-reader.md): at the camera limit the ledger changes nothing,
because nothing there was ever refused.

What the phase promised is the *speed*, and it has now been measured, though not on the
question the plan predicted. Sweeping one decision across sixty-one frames of
opportunity costs one run-up plus one window per sample rather than sixty-one run-ups,
because the instrument rewinds to the end of the tape with `checkpoint`/`restore` instead
of replaying it: the 1000-frame run-up is paid once. The claim holds for an instrument.
Phase 3 is where a *skill* gets the same test.

The phase's other lesson arrived with the second and third gyms, and it is the one the
plan did not have: **a gym is cut upstream of the failure, not at it.** `camera-limit`
was cut inside the fall it was built to ask about, so its verdict could only ever be
`Stuck` no matter who played it, and the pair cut from it measured a difference that
could not have mattered. `pit-1379` is the same run stopped 153 pixels short of the hole
and is the first start state in this repository a bot can win from.

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

The first skill was specified by measurement before it was written, and the
measurement turned out to be about the reading rather than about the jump. The pit at
absolute X 1379 is 48 pixels wide and the game's box reaches 14 pixels past his
position, so a landing at 1414 or further stands on the far lip; from a run-up cut at
frame 1040 the commit window is thirty-one frames wide, and the jump that clears it
carries 118 pixels measured from the last frame he stands on anything, not the 115
measured from the takeoff — three pixels and one frame apart, which is most of the
margin. `pit-1379` is that situation with a verdict attached, and the script now wins
it: `Met` on frame 151 at absolute X 1599, against the same gym's `Stuck` at 1544 under
the reflex, one field of the gym different.

What that is not yet is a library. Two skills exist, one of them load-bearing for a
reason nobody predicted — the ablation crosses the pit without the hop and dies on the
koopas 286 pixels past it — and the exit below still asks for a bot that finishes 1-1,
which needs a skill for every other thing 1-1 does.

Exit: a scripted bot finishes 1-1 end to end, and each skill has a measured height,
duration and run-up beside it. Add to that what the ablation taught: each skill also
needs a gym that fails without it, because a skill can be measured, correct, and
load-bearing for a situation its own gym is not named after.

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

- **Golden state hashes.** Three now exist, one per committed tape, each asserted by the
  gym tests that replay it, which is what makes those gyms durable across core work on a
  machine that has the cartridge. They are still all hashes of states only a cartridge
  can produce, so a core change that quietly alters SMB stays invisible on a machine that
  does not. The cheap remaining half is a boot-to-gameplay hash taken from a built-in
  image rather than `data/roms/smb.nes`.
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
- Gyms store tapes, not savestates, and a tape is recorded and replayed before it is
  committed, never typed in.
- A gym names the ledger it starts with. A verdict that depends on unrecorded host-side
  state is not a measurement.
- A gym is cut upstream of the failure, not at it. If no input from the start state can
  reach the objective, the gym reports the same verdict whoever plays it and measures
  nothing.
- An instrument that rewinds has its rewind pinned by a test. A restore that quietly
  loses a timer moves every sample it takes and still prints plausible numbers.
- An envelope is measured per frame, not per button. The same hold rises three different
  heights a few pixels apart, so one number is wrong about most of the level.
- A button claim is a frame claim. The buttons handed to a frame are the caller's and
  nothing keeps them; `$074A` is the game's copy, and the test that compares the two is
  what makes an envelope's carry mean anything.
- A skill is not in the library until a gym fails without it. Measuring an envelope says
  what a behaviour does; only an ablation says what the level needed it for.
- Search budgets nodes and frames, not seconds.
- A test wait bounds progress, not wall clock. A shared runner legitimately runs the
  same job on one worker when it is busy, and a deadline reads that throttle as a
  hang, so load decides the verdict. `engine-client`'s training test flaked this way
  and was mistaken for non-deterministic keyboard input; a wait on a gym sweep will
  flake the same way for the same reason.
- The core keeps no game knowledge, so everything from the profile up stays host
  side.
