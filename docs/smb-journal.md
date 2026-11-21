# Super Mario Bros. world-model decision journal

[The mission](smb-mission.md) holds the direction,
[the execution plan](smb-execution-plan.md) holds the order of work, and
[the world-model reader](nes-world-model-reader.md) holds the measurements and the
decode traps. This journal preserves the decisions themselves and why they were
made, so that a choice is not relitigated by someone who only sees its result.

Add entries chronologically with a stable ID, a date, the decision, the evidence
and the consequence. Later decisions may supersede earlier ones; record the
supersession rather than rewriting the original rationale. Routine edits, test
output and film runs belong in Git and in run artifacts, not as a second task list
here.

Every entry below was decided on 2026-11-21, the day this work went from an
investigation to a bot with films, so the entries are ordered by the order decided
rather than by date. Entries that adopt or correct a DirtSim decision say which, and every figure
quoted here was re-measured against this repository's cartridge.

## SMB-001 — 2026-11-21 — Read the game out of the core instead of changing the core

Build the world model above `engine-nes` using only channels the core already
exposes: RAM, nametables, attribute tables, OAM, palette, the scroll latch,
mirroring, frame timing, `checkpoint`/`restore` and `state_hash`. No emulator
change.

Rationale: every channel a semantic reader needs is already public, so an
emulator change would add game knowledge to a component that is deliberately
cartridge-agnostic and free of a filesystem, clock, thread and renderer. The
in-repo precedent is `scenarios/falling`, which decodes a versioned observation
from six RAM addresses.

Consequence: the reader is host-side and the core stays an emulator. A reader bug
can never be blamed on the core, and a core change that alters the game is the
reader's problem to notice.

## SMB-002 — 2026-11-21 — Identify a tile by its CHR pattern hash, never by its tile number

Tiles are named by an FNV-1a hash of their 16-byte pattern.

This adopts a DirtSim design, `NesTileFrame` and its tile tokenizer, and was
verified here rather than assumed to transfer.

Rationale: tile numbers are bank- and game-specific. SMB's blank-sky filler is
`$24` and not `$00`, so a reader that trusts numbers sees a wall of solid tiles
across the whole sky. A pattern hash makes emptiness a property of the pixels, and
it survives a CHR bank switch and a different ROM revision.

Consequence: the vocabulary is learned, so a port needs a frozen vocabulary or a
documented rebuild step. This is an open gap, not a solved one.

## SMB-003 — 2026-11-21 — Decode the scroll latch `t`, not the current address `v`

The tile origin comes from the PPU's temporary address register.

Evidence: `v`'s Y field increments on every visible scanline and is only reloaded
during vblank, so read after `run_frame` it reports a nametable one table away from
the one just rendered. Switching to `t` removed 57 shape mismatches, and the
reconstruction is then exact at a static camera: 51,136 compared pixels, none
wrong.

Consequence: DirtSim's `NesTileFrame` decodes `v` and carries the same error. It is
invisible in a tile vocabulary and a reward signal, which is why it survived there,
and it only appears when the decode is checked against rendered pixels. That is the
argument for pixel-level validation of a reader, not just behavioural validation.

## SMB-004 — 2026-11-21 — The player's feet are 32 pixels below his stored Y

`$00CE` is the top of a 32-pixel sprite bounding box, not the surface he stands on,
so the standing row is `$00CE + 32` minus the status-bar crop.

Evidence: two independent measurements agree. Sweeping every 8-pixel offset across
2583 grounded frames leaves `+24` after the crop as the only row solid under the
feet with the body's two rows clear, and the player's own OAM column ends on the row
the constant predicts on 1008 of 1008 frames where his sprite is identifiable.

Consequence: DirtSim's player-row helper subtracts only the crop and so sits four
tile rows too high. It never bites there because that value anchors a
player-relative window where a constant bias is absorbed by training, and it bites
hard the moment a bot asks whether there is ground under it. A constant that a
learning consumer can absorb is not the same as a constant a rule consumes.

## SMB-005 — 2026-11-21 — Learn the load-bearing set from grounded frames, and accept what that can answer

Terrain walkability is a learned set of nine tile ids, gathered by recording which
id sat under the player's feet on frames the game called grounded, against which ids
his body occupied.

Evidence: the split has no overlap across 2583 frames. `$26` was a body tile on 2413
of them and a feet tile on none, while the ground and pipe-top ids only ever held
his feet. Opaque is not solid: SMB paints hills and bushes with fully opaque tiles
and walks Mario through them.

Consequence: the set can only ever contain surfaces, because that is the only
experiment that produced it. See SMB-008 for what replaces it as the authority on
walls; this remains the authority on floors.

## SMB-006 — 2026-11-21 — The reader lives under `tests/support`, reached by `#[path]`

The reader is not promoted into `engine-nes/src`.

Rationale: the core holds no game knowledge, so the address map belongs on the host
side of that boundary, and the part with a contract is the test, not the examples.
The examples are exploration scaffolding and reach the reader with `#[path]`, which
is why the module carries one `#![allow(dead_code)]`.

Consequence: promotion is gated on a second consumer needing it, which is what keeps
this from becoming a framework before it has a second user.

## SMB-007 — 2026-11-21 — The cartridge test is optional and fails closed on a different revision

`tests/smb_world_model.rs` skips with a note when no cartridge is present, and
rejects a cartridge whose SHA-256 is not the image the map was verified against.

Rationale: `data/` is gitignored, so CI has nothing to boot and an optional test is
the only honest shape. A shipped profile must fail closed rather than return
plausible nonsense: `smb2.nes` reports that it is not the verified revision.

Consequence: the semantic reader has a regression net locally and none in CI, which
is the cost of keeping cartridges out of the repository and is accepted knowingly.

## SMB-008 — 2026-11-21 — Solidity will be measured from the game's reaction, not inferred from pictures

Decided, not yet built. The reader will command a move, compare the motion asked for
with the motion the core produced, and attribute the difference to the cell entered,
at that body height, from that side.

Evidence: the reader-only bot walks into a pipe at absolute X 899, is stopped by the
game, and stands there for 600 frames with `wall_ahead` false, because the pipe body
ids are neither load-bearing nor blank. The reaction rule bolted on instead of a
solidity model moves the run between absolute X 595, 723, 899 and 2601 as one tuning
constant changes, which is what tuning on top of a missing fact costs.

Consequence: this supersedes SMB-005 as the authority on walls. The reaction rule is
to be deleted rather than tuned when it lands. The known fallback, recorded so the
phase cannot become a dead end, is to read the game's own collision result out of RAM
if the velocity delta proves too noisy on eight-bit scaled speeds.

## SMB-009 — 2026-11-21 — An action is specified to the frame

A skill's action envelope is measured, and a press is recorded by the frame the
button reaches the game rather than by a hold counter.

Evidence: a one-frame A press is a hop that clears nothing; the same policy with A
held 20 frames went from absolute X 436 to 899 with no change to the reader.
Separately, a reflex rule that set its hold after the frame's buttons were built
landed A one frame late, and the earlier claim that the reflex cleared 1-1 at X 3266
had drawn one winning ticket with that bug in it.

Consequence: a decode gap and an action bug look identical from outside a run, so an
envelope is a measured property of a skill and not an implementation detail.

## SMB-010 — 2026-11-21 — Wait for the level to hand the player over, not for the mode byte

A policy starts after eight consecutive frames that are both in-control and
grounded.

Evidence: `$0770` flips to play on the first frame of a level load and the load takes
about 150 more frames to place the level, and through the first two of those the
player bytes still hold the previous screen's values. A bot started on the mode byte
pushes at a world that is not listening, and a stuck detector counted in frames
reports that as the bot giving up before it moved.

Consequence: measured handover on this cartridge is frame 287, against gameplay mode
at 120. Every run starts from the handover, and the stuck rule is only meaningful
after it.

## SMB-011 — 2026-11-21 — A film is evidence, so the tool decodes one back and recounts the overlay

`tools/nes-reference/film-smb-run.py` writes films, clips and an event sheet, then
decodes a frame out of the film it just wrote and counts the model's dots again.

Rationale: a one-pixel mark is exactly what a lossy encoder discards, and a film that
quietly lost the overlay still looks like a film. The tolerance is a tenth of the
count, tight because the NES palette is full of browns one channel away from the red
dot.

Consequence: reported encodes are checkable rather than asserted; a run over the
death at frames 990 to 1080 reports 29 load-bearing and 161 opaque dots in against
29 and 159 out. The films themselves live under `data/shots` and are not committed.

## SMB-012 — 2026-11-21 — The mission document carries no measurement

`smb-mission.md` contains no digits, no command-line flags and no code identifiers,
and a grep for them is part of its review.

Rationale: the first draft mixed direction with reporting, and reporting is what
rots. Frame numbers, flags and test counts are wrong within a week and none of them
say what is being built.

Consequence: measurements live in the reader investigation, which is the document
whose job is to defend them, and the mission changes only when the direction does.

## SMB-013 — 2026-11-21 — Build the gym harness before the skill library

The plan puts the harness at Phase 2, ahead of the skill library, against the
mission's own ordering.

Rationale: instruments get built before the things they measure. A snapshot gym is a
rewind point, an objective predicate, a budget and a verdict, and it is nearly free
because the rewind is exact to the bit. Without it every later fact costs a whole
level run to learn.

Consequence: the plan states the disagreement rather than quietly reordering the
mission, so the mission stays the direction and the plan stays the route.

## SMB-014 — 2026-11-21 — A committed gym stores a tape, not a savestate

A gym's durable form is a cartridge identity, an input prefix and a frame index, with
the state re-derived when the gym runs.

Rationale: a committed savestate silently mismatches when the core changes, which is
precisely the moment a gym earns its keep. A tape fails loudly instead, and it
documents itself.

Consequence: gyms double as golden hashes for the cartridge, which is the gap that
currently leaves a core change to SMB invisible without booting one.

## SMB-015 — 2026-11-21 — Decide the seam before the API

Phase 0 decides which host-side module owns the profile through the affordances, and
what a skill may ask it. It does not design the interface.

Rationale: an interface designed before a bot has cleared anything is a guess with
types on it, and the decision is cheap only while there are two small consumers, the
bot and the optional test. After the skill library exists, moving the reader is a
refactor across everything.

Consequence: the API grows out of the pipe in Phase 1, and nothing is promoted out of
`tests/support` until a second consumer needs it, which is SMB-006's rule applied
forward.

## SMB-016 — 2026-11-21 — The model and the players get separate crates above the core

Phase 0's answer: the profile through the affordances will live in `crates/smb-model`,
which depends on `engine-nes` and nothing else; the skills, the route, the gyms and the
search will live in `crates/smb-ai`. `scenario-nes` keeps the frame and action envelope
and names no cartridge. The reader itself stays where SMB-006 put it until the gym
harness asks for it.

Rationale: for this game the cartridge is the world, so the two crates are a model of
it and the players of it, which is the division this repository already draws between
`scenario-spacewars` and `spacewars-ai`. Neither belongs in `scenarios/`, because
neither is a scenario. The client is the third candidate and the wrong one: it cannot
become the model's owner because a test and an example cannot depend on it, and a
model that grows inside a renderer is a model that gets rewritten the first time the
renderer changes.

Consequence: a skill's whole question set is four questions about body, resistance,
affordance and postcondition, and a skill that asks for an address, a tile number, a
pattern hash or a register is out of contract. It also gives SMB-006's gate a named
destination and a named trigger, so the reader still sitting in `tests/support` is a
plan being followed rather than a smell, and it leaves the observation's profile
payload deferred until the client wants to draw the model.

## SMB-017 — 2026-11-21 — Solidity is learned from the game's refusals, keyed on the artwork that refused him

SMB-008's plan, built. A frame counts as a refusal when three conditions the game owns
are all true: a direction held without its opposite, the body gaining no ground to the
pixel, and the speed the buttons had built up coming back exactly zero. The verdict is
stored against the CHR pattern hash of the tile that refused him, the row of his body it
was pressed against, and the direction — not a tile number, and not a position.

Evidence: the signature is the game's own, so nothing needs a threshold. Pinned against
the first pipe his speed runs 0, 1, 2, 3, 0 and against the second 0, 1, 2, 3, 4, 0, so
one contact records one refusal rather than one per frame of pushing. Keying it on the
pattern is what lets one lesson generalize: refused at absolute X 594, the bot calls the
second pipe a wall 304 pixels later without ever being stopped by it. Two attributions
had to be measured rather than assumed. The box the game stops against the level is
`[x, x + 14)` from the sprite's left edge at `$0086`, fixed by two independent refusals
304 pixels apart, and not the 11 that had been read off the decoded grid; and the cell
beyond him has to be found from the world pixel rather than from the screen-indexed
grid, which is up to one column out on seven frames in eight. The A/B that separates
memory from reaction restores one checkpoint state twice, once with an empty ledger and
once with the refusals from an earlier pass: the amnesiac tail names its first wall on
frame 864 flush against it after being stopped, the remembering tail names it on frame
855, nine frames and seven pixels before contact, and is refused nothing at all. How far
ahead the ledger is trusted turned out not to be a knob — the run is identical at 0, 1,
2, 3 and 5 columns — where the rule it replaced swung between 595 and 2601 over the same
range.

Consequence: this closes SMB-008 and deletes its reaction rule and its tuning constant
rather than tuning them; the reader-only policy reaches absolute X 1553 with the rule
gone. Two things follow that are not handled yet. The ledger lives host-side, so a
checkpoint restores the machine and not the model, which means a gym's verdict depends
on whether the model it hands the run is warm or cold and SMB-014's tape does not carry
that. And a refusal only ever names a drawn cell, so the thing that now takes three
lives in a row at the camera limit, where he is falling rather than blocked and no
artwork is involved, sits outside the model entirely: that is the first gym Phase 2 is
asked to build.

## SMB-018 — 2026-11-21 — A gym is a recorded tape that replays itself, and it names what the model remembers

Phase 2's harness, and the three calls that make it an instrument rather than a
replay feature. A committed gym stores a tape — the cartridge's identity, the button
latch values held for a run of frames, and the frame id the gym starts on — and the
state is re-derived by replaying it. A tape is always produced by `Gym::record`, which
replays what it just recorded before handing it back, so the bytes that get committed
are the bytes that produced the verdict printed beside them. And a gym carries a
`LedgerSeed` saying whether the model starts having been refused or not, because that
is host-side state no checkpoint can hold.

Rationale: a savestate silently mismatches exactly when the core changes, which is the
moment a gym is most worth having; a tape fails loudly instead, and the failure is
named — `WrongCartridge`, `ShortTape` and `Drifted` each say which of the three things
that can be wrong is wrong, rather than surfacing as a strange result two hundred
frames on. Recording rather than authoring is the same discipline one level up: an
authored run-up is a guess about what the bot did, and a gym built on a guess measures
a situation the model may never meet, which is why Phase 6 defers invented levels for
the same reason. Pinning the frame id rather than deriving it from the run lengths is
what makes a carelessly truncated tape report `ShortTape` instead of quietly starting
somewhere else. `LedgerSeed` is SMB-017's loose end faced rather than papered over:
`restore` returns the machine and leaves `Resistance` wherever the caller left it, so a
verdict that does not say which ledger it ran with is not reproducible, and the honest
fix was to make the two start states two gyms.

Evidence: the first tape is 32 button runs to frame 1167, ending on state hash
`v3:a73cebb1ed214d50` with absolute X holding at 1544 and the game still holding the
controller, and it is the project's first golden hash. The pair cut from it measures
identically — `Stuck` after 120 frames at absolute X 1544 with zero refusals, cold and
warm alike — and the agreement is the result: the thing at the camera limit never
refused him, so there was never a lesson for a ledger to hold, which means a wall has to
be readable from the picture before contact and not only remembered after it.
`replaying_the_same_tape_twice_lands_on_the_same_bits` is DirtSim's
`ReplayFromRootIsDeterministic` transplanted, and it is the check that a bot's buttons
reach the game in the order they were decided: the core latches a controller on the
game's own strobe, so an input landing at the wrong moment inside a frame would show up
as two replays of one tape disagreeing.

Consequence: this fires SMB-006's gate and completes SMB-016's move — L1 through L4 are
now `crates/smb-model`, the policy and the harness are `crates/smb-ai`, the five
`world_model_*` examples and the optional test went with them, and the core still holds
no game knowledge. Two of the plan's standing disciplines are new since this entry:
tapes are recorded and never typed in, and a gym names its ledger. What the phase
promised and has not delivered is the speed, because nothing has asked the harness a
question except the one it was cut for; that claim gets tested in Phase 3, where a
skill is measured against a gym rather than against a whole level. The camera limit
stays unexplained, and this entry narrows why: answering *what* it is needs a reading of
cells the camera will not show, which is Phase 4's arbitrary-level start, not a gym.

## SMB-019 — 2026-11-21 — A gym is cut upstream of the failure, not at it

Phase 1's unnamed stop has a name, and it is not a wall: the level has a pit from
absolute X 1379 to 1427, and the run that looked pinned to the edge of a locked camera
is a death in it. The decision is about where a gym's tape ends. A gym is cut at the
last frame its objective is still winnable from, not at the frame it fails on, and a
jump is scored by what the game allows after the landing rather than by what the tile
map says about the cell he came down on.

Rationale: a start state inside the failure measures the absence of a skill and nothing
else. `camera-limit` is cut at frame 1167 and he left the ground on 1100, so the frame
the jump had to start on is sixty-seven frames behind the start state before a bot has
pressed anything; its verdict is `Stuck` for every policy that ever runs it, and a gym
whose verdict cannot change is a recording rather than an instrument. Cutting upstream
is also what makes an objective mean something: 1600 sat past the old stop and
discriminated nothing, and a pit fall clamps absolute X at 1549 against the screen edge,
so from the new cut 1600 is reachable only by crossing. Scoring by the game rather than
by the map is the same argument one level down. The jump that works comes down at 1414,
thirteen pixels inside a hole the map says ends at 1427, and a cell lookup calls that the
hole; the game calls it a landing, because his box is `[x, x + 14)` (SMB-017) and its
right edge reaches the solid cell at 1427, and he then walks on to 1429 still grounded. A
sweep scored off the picture fails the one press that works.

Evidence: read by world pixel from the last frame the game called grounded, the support
row stops and starts again — `--` in every row it looks at, so the cells draw nothing and
this is the level's hole rather than the load-bearing set's blindness — 48 pixels wide,
six cells, starting eight pixels ahead of him, the ground returning at 1427 wearing the
same `$b4`/`$b5` artwork. The game's own side agrees: walked off at 1376 in float state 2
on frame 1100, X clamped at 1549, state `$06` on 1379, a life gone on 1380, reset on 1382,
with both live enemy slots 101 and 125 pixels *behind* him the whole way down. The commit
rule then comes out exact: the far edge is 1427, the box reaches 14 pixels past his
position, so a landing at 1414 or further stands on it, and a jump taken at 1299 has to
carry 115 — which is what the envelope measures from that frame, 115 pixels in 46 frames
of air. Every sample the sweep reports lands where that arithmetic says it should: the
presses that carry 112 from a takeoff at or past 1302 cross, the ones that arrive at 1371
or 1379 fall, and the presses past 60 frames into a run-up cut at 1040 cause no jump at
all because A goes down while he is already in the air and the game never spends it. The
window is 28 to 58 frames in from that cut, thirty-one frames wide, and the same
run-up stopped at 1000 reports the same thirty-one frames at 68 to 98, which is the
instrument saying the cut moved and the level did not. From the committed 1167 no press
in the sweep leaves the ground at all.

Consequence: this supersedes the last claim of SMB-018. Answering *what* the camera limit
is did not need Phase 4's arbitrary-level start, because the pit is inside the screen the
camera is already drawing and reading it needed only the world-pixel lookup SMB-017 had
already forced out of the grid. Phase 1's stop is named and it is not a solidity problem:
nothing refused him, so the two camera-limit gyms' zero refusals were correct all along
and the ledger was never the gap. That moves the stop to Phase 3 as its first skill, and
the skill arrives specified by measurement rather than by design. `camera-limit` stays
committed and unwinnable on purpose — it is the record of what an overlay could see, and
the two cuts together are the difference between a start state that can be measured and
one that can only be reported. `has_fallen_off_screen` is deleted rather than fixed: the
byte it compared is the low byte of a position that keeps counting, so it wraps every 256
pixels of a fall and DirtSim's terminal test is true for about eight frames in every
sixty-four. `SmbState::is_dying` takes its place and is DirtSim's own decode — `$06` and
`$0b` both mean a life is going, and the life counter lags both.

## SMB-020 — 2026-11-21 — An instrument rewinds to the end of a tape, and a test pins that a rewind is a replay

The `limit` example keeps one machine and one checkpoint and rewinds to it between
samples instead of replaying its run-up per question, and `gyms` carries a test that
requires the two to land on the same bits.

Rationale: the commit sweep asks sixty-one questions of one run-up. Replaying the tape
per question was correct and slow enough that the sweep had been stepped by four frames
to fit inside a work session, and a sweep that steps by four can walk straight over a
window three frames wide — which is the exact thing it is looking for. A checkpoint is
the cheap fix and it is not free: `restore` returns what `checkpoint` captured and
nothing else, so a section that forgets to rewind measures the frame the previous section
stopped on, silently, with plausible numbers. That is the failure worth a test rather
than a comment, because it is invisible in the result. Frame zero is not the interesting
comparison — a restore always agrees with its own checkpoint there — so the test plays
forty frames past the end of the tape first, restores, requires the tape's own golden
hash, and only then compares sixty frames of the rewound machine against sixty frames of
a fresh replay, absolute X on every frame and the hash at the end.

Evidence: sixty-one samples now cost one run-up of replay rather than sixty-one of them,
which at the committed tape's 1040 frames is 63,440 frames of replay avoided for the
1,040 actually paid; the whole instrument — a bot run, a state timeline, a support map
and sixty-six jumps, each watched up to 420 frames — runs in four and a half minutes on
this machine with the cartridge, where stepping by four and replaying per sample did not
finish inside a sitting. The sweep found a commit window thirty-one frames wide, so the
step had to come down to two frames to resolve it at all, and it did once the run-up
stopped being paid per question. What the finer step bought is visible in the result: the
same level read from two cuts reports the same window, thirty-one frames wide, at 28 to
58 frames in from a run-up stopped at 1040 and at 68 to 98 from one stopped at 1000 —
which are the same frames, 1068 to 1098. A sample taken at absolute frame 1069 from the
1040 tape and the same sample taken from the 1000 tape agree on every column the
instrument prints: takeoff at 1299, apex y101, headroom 96, landing on frame 1115 at
1414, forty-six frames of air, one hundred and fifteen pixels of carry, across on frame
1121 at 1429. Two different tapes, two different checkpoints, one level. The rewind test
is DirtSim's `ReplayFromRootIsDeterministic` moved from a replay to a restore, and it is
the check that the checkpoint carries every timer and enemy slot the run-up touched
rather than the ones the core happened to notice.

Consequence: Phase 2's speed claim is met and, for the first time in this project,
measured — SMB-018 closed with "nothing has asked the harness a question except the one
it was cut for", and sixty-one questions is the answer to that. The standing discipline
is that a harness which rewinds has its rewind pinned by a test, because the alternative
failure mode is a plausible wrong number rather than a crash. It also changes how the
instrument is used: `limit [FRAME]` now answers the same two questions at any cut,
because the envelope is measured at the press the sweep finds rather than at a press
named by hand, and a named press is a press that only makes sense at the cut it was named
for — press 68 is the commit point of a run-up stopped at 1000 and is inside the pit of
one stopped at 1040, where the envelope section used to print five jumps that had never
happened.
