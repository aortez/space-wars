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

## SMB-021 — 2026-11-21 — The picture ends two pixels short of a long jump's landing

A skill is a named behaviour with a precondition in affordance terms, a postcondition,
and a button program measured to the frame; `clear-gap` and `hop-obstacle` are the first
two, and `pit-1379` is met under the script that strings them together.

Rationale: Phase 3's first skill was specified by measurement before it was written, and
the measurement turned out to be about the reading rather than about the jump. The
envelope was re-measured at one-frame resolution from the committed tape and the carry is
a range: the same twenty frames of A carry 118 pixels from the frame the envelope is
pinned to, 117 one frame earlier, and a flat 115 from every frame from thirty on, so the
115 the plan quoted — measured from the takeoff at 1299 — and the 118 the envelope carries
are the same jump three pixels and one frame apart. The near edge came out the same way:
press 26, whose last grounded frame is 1291, falls in, and press 27 at 1294 comes down at
1411 and stands, which puts the game's edge between them and the rule's edge, read at the
last pixel of the box, at 1293. Judging a landing by the position byte instead of by the
box refuses every jump the game allows, since the shortest crossing the sweep scored left
him at 1414 over a hole whose ground returns at 1424 and it stood. Then the thing the
design had no answer for: at running speed the camera pins him at screen column 126 or 127
and the reader reaches thirty-two columns, so the picture stops 128 or 129 pixels past his
position and the long jump's box comes down at 131. The direct probe — is the last pixel
of the box over something load-bearing — has no answer there, and the first version of
this library asked nothing else, refused every long jump it was offered, and walked into
the pit. So the landing is asked two ways: the pixel first, and where the picture has run
out, the hole's own far edge, which is the same reading taken from a pixel inside the
window rather than two outside it. Probing at 129 instead alternates between an answer and
none as the fine scroll moves, which is the edge itself flickering and not a tile the
reader lost. Two smaller rules came out of running it rather than designing it. A long
jump that lands with one foot on the lip leaves him standing at 1414 inside a hole whose
ground returns at 1424 — standing to the game, in the hole to the map — and the hole
reading from there is a nine-pixel hole still ahead, so the script asked for a second long
jump from inside the hole it had just crossed; the postcondition is now asked before the
precondition, and a claim the world has already granted cancels the start. And the
arbitration is order plus `Wait`, where `Wait` passes the frame on rather than holding it,
because a commit window has a near edge and a skill that cannot say "not yet" is the base
policy.

Evidence: `pit-1379` is `Met` on frame 151 at absolute X 1599 with no death under
`skills::SCRIPT` and `Stuck` after 248 frames at 1544 under the reflex, one field of the
gym apart, and `gym` re-measures both against the verdicts the gyms carry. The script
commits at absolute X 1296, inside the window the game scored at 1294 to 1371, and 1296 is
the first frame the far lip is inside the camera's window at all. The hop's envelope
measures 65 pixels of carry, 25 of air and 39 of rise from fifteen frames in, and its
headroom reads 32 while its rise is 39 — he takes it under the brick row and is twenty
pixels past the row's end by the top — which is why headroom is recorded as the context a
number was taken in and not as a requirement. The ablation is the part that changed a
design assumption: with `hop-obstacle` removed the pit is still crossed, from the first
frame the far lip is readable, and the run stops dead at 1526 on a pair of koopas 286
pixels past the gym's name; with it restored the hop fires twice, at frame 1055 on a
koomba twenty-two pixels ahead where the run did not need it and at 1147 at forty-three
where it did, and the gym is met. Twelve tests hold it, seven of which need no cartridge
at all because a `Situation` can be written by hand.

Consequence: Phase 3 is started rather than next, and its exit grew a clause — a skill is
not in the library until a gym fails without it, because a skill can be measured, correct,
and load-bearing for a situation its own gym is not named after. The hop's real gym is
enemies on flat ground and does not exist yet. The bigger consequence is a standing limit
on the model: at running speed this reader can never *see* a long jump's landing, only
infer it from the hole being crossed, so a level whose far lip is not the ground return —
a ledge over a pit, a platform standing in a hole — is not a wider look away, it is the
level memory Phase 6 is about. One pixel of the landing rule is still open, too: a landing
at 1410 has never been pressed, and the reader's cell-aligned far edge at 1424 and the
instrument's pixel-exact lip at 1427 disagree by three, with the measured window as the
only arbiter between them.

## SMB-022 — 2026-11-21 — The game's own input byte is where a button claim gets checked

`$074A` joins the RAM map and a test compares it, frame by frame, against the buttons each
frame was handed.

Rationale: every envelope in this project is a claim about what a button program did, and
the claim is only worth the frame the button arrived on. The buttons are a host-side
argument to `run_frame` that nothing keeps, so nothing in the tree could see that
assumption — the plan already records a hold built after the frame's buttons had been
assembled landing A one frame late and moving a run by more than two thousand pixels, with
every decoded state along the way looking reasonable, because the state is the game's and
the buttons were the caller's. The question arrived from the other side first:
`engine-client`'s training test flaked on a shared runner and was reported as possible
non-deterministic keyboard input, and it was not — the waits around the keyboard dispatch
counted down from a fixed number of seconds while a busy runner legitimately runs the same
job on one worker, so runner load reached the verdict first. That fix bounds the waits on
the job's own progress counters. The NES side deserved the same look rather than the same
conclusion, because there the input is the measurement.

Evidence: the game keeps its own copy of player 1's buttons at `$074A`, written inside the
frame from the port the frame was handed, in the order the controller shifts them out — A
in the high bit down to Right in the low one, the reverse of `ControllerButtons`, so the
decode reverses it; asking A and Right reads `$81`, asking Left reads `$02`, asking Select
and Right reads `$21`, and reversing reproduces all three. Measured against a pattern that
touches all eight buttons and both edges of each, from the handover: before a frame runs
the byte still holds the previous frame's ask, and after the frame runs it holds that
frame's ask exactly, on every frame, held or released, for sixty frames. The alignment
every envelope above assumes is therefore pinned rather than assumed.

Consequence: the input path has a regression net, and the byte is where a response probe
can compare an ask against a take before it starts timing a jump, which is what DirtSim's
probe does (`gameInputCopiedFrameId` in `NesSuperMarioBrosResponseProbe.cpp`) and the shape
Phase 5's search will want when it asks whether a macro-action did anything. It also
settles what "the keyboard is non-deterministic" was worth: on the client side, nothing —
it was a wall-clock deadline; on the emulator side, nothing measurable, and now a test
would say so if that ever stops being true.

## SMB-023 — 2026-11-21 — An affordance that cannot name its cell cannot be postconditioned

`wall_ahead` reports a pixel and a height rather than a bit, `clear-wall` joins the
script, and world 1-1 is cleared by a library rather than by a reflex.

Rationale: the reflex has never got past the pit, so the script had never been asked to
play a level either, and the first thing worth knowing about a skill library is what it
does on the ninety per cent of a level its first gym is not about. It stopped at the first
pipe with the hop's precondition satisfied on every frame of it. The cause was the
postcondition SMB-021 had just installed in front of the precondition, working as designed
on a claim that had no cell in it: `hop-obstacle`'s `past` was `x + enemy_pixels.unwrap_or(0)`,
so with a pipe ahead and no enemy in front of that, the pixel the skill was about was `x`
itself — which a body fourteen pixels wide is always past — and the skill cancelled its own
start. Phase 0's rule already had the answer and had not been applied to walls: an
affordance names the cell it is about. `wall_within_reach` keeps the bit the reflex needs
and `wall_ahead` answers with `Wall { x, rows }`, the near edge of the refusing column and
how many rows above his feet that column draws.

The height is what decides the envelope, and that is why the wall got its own skill rather
than a bigger hold inside the hop: an envelope is a measured program, and four frames of A
reach their top long after they reach the pipe in front of them. So the hop is enemies and
nothing else — measured over a koomba, never measured over a wall — and `clear-wall` is the
long jump, which is the program this project has filmed clearing that pipe.

The second stop was the stairs at absolute X 2162, and it was the landing gate refusing a
two-row step. At a wall the long jump's box comes down two pixels past the last column the
PPU draws, so the direct question has no answer, and the fallback that saves a long jump
over a hole is a hole whose far lip the picture showed — and an untried staircase is drawn
with artwork the load-bearing set has never held a body on, so the hole reading returns
sixty-four pixels of "nothing to stand on" with no far side at all. A gate that cannot be
answered is a refusal, so the skill stood in front of sixteen pixels of step, against a
rise of seventy-five, for six hundred frames. The decision is that at a wall the wall is
the landing: `wall_ahead` reports a column only where the load-bearing set has held a body
on it or the ledger was refused by it, which is already a claim about solidity, and a cell
that stops a body sideways is the same collision that holds one up. What that gives up is
the question *behind* the wall, which is the same two pixels the reflex has always been
jumping into blind.

The height cap was then lifted from eight rows to twelve and measured, and the measurement
said to put it back. Twelve rows is ninety-six pixels, so the height term became able to
fire — and the run stopped fourteen pixels short of 1-1's exit, against the flagpole: a
column the model reads correctly as solid and correctly counts as twelve rows of drawing,
that no jump in the library gets over, and that the level ends by being jumped *into*. A
gate on height alone strands the bot there with nothing else in the script to take the
frame. The cap is therefore back at eight, which is sixty-four pixels and under the long
jump's rise, so the term is knowingly a gate that does not gate; the alternative was a
number chosen to let this run pass, and the situation the cap hides is a missing skill
rather than a wrong constant.

Evidence: `bot 2620 --script` clears world 0.0 at absolute X 3266 with no death, 376 frames
with A held and 26 named behaviours, of which eleven are `clear-wall`, six `hop-obstacle`
and two `clear-gap` — the pit crossed on frame 1018 from absolute X 1324, the same hole the
gym is named after, taken as one behaviour among eleven. The same instrument under the
reflex reaches 1553 and spends two lives there. `bot 2400 --script` before the wall could
name itself reached 436; after the pixel and before the gate came down, 596. Four tests
hold the rules, and none of them needs a cartridge: the wall names a pixel the body is not
already past, the hop leaves the pipe to the skill that can get over it, `clear-wall` starts
where the landing gate cannot be answered, and a wall taller than the rise is nobody's
frame. The films are `script-1-1.mp4` and three clips — the stairs, the flagpole, and the
walk to the castle that the level ends on — plus `script-1-1-events.png`, all twenty-nine
event stills in order. The clear still that sheet ends on is the reason `bot` now writes
`event-clear-*`: the module doc had promised that name since the first run and nothing had
ever reached the frame that writes it.

Consequence: the flagpole is the next gym, and it is a strange one — the obstacle is the
level's exit, and the behaviour that fails on it is the one that succeeds everywhere else.
A wall too tall to clear needs a skill whose claim is not "past it", because pressing into
such a wall costs the air it takes and nothing else, and 1-1 proves a bot that never does
that never leaves the level. Two more things are now visible and unfixed. `carries_weight_at`
has two answers where the picture has three: a cell drawn with artwork the model has no
evidence about is reported as "nothing to stand on", which is why the staircase read as a
sixty-four-pixel hole, and the ledger that already knows those cells refuses to be asked.
And a height counted by what draws is inflated by any background artwork stacked above a
real wall, which is the cost of lifting that cap and is not yet priced by anything.

## SMB-024 — 2026-11-21 — A wall gets its gym, and the layer that fails is not the model

`pipe-608` joins the committed gyms, `bot` takes `--trace N` so that a cut frame is a
measurement rather than a guess, and Phase 3's exit closes with every skill in the library
ablated against something named.

Rationale: the standing rule says a skill is not in the library until a gym fails without it,
and two of the three skills satisfied that rule on the strength of one gym that is about the
hole. `clear-wall` carries eleven of the twenty-six behaviours that finish 1-1 and had no
situation named after it, so "the level needed this skill" had never been tested. The level
run cannot test it: there the skill is never alone, it has a ledger warmed by everything
before it, an enemy standing in front of half its walls, and 1-1's own ordering of obstacles.
A gym is the only place in this project where one entry in one library can be the only
difference between two runs, so the wall needed a start state of its own.

Cutting one turned out to be a question about where the player is on a given frame, and the
instrument that answers it printed a decoded line every 600 frames. The reflex is refused at
the second pipe on frame 642 and is at absolute X 908 on 886, so "cut it sixty pixels short
of the pipe at running speed" was being read off two prints that bracketed the entire
approach. `--trace N` prints every N frames instead, and the cut came from what it said:
grounded and running from 608 (absolute X 531) to 636 (593), stuck at 594 from 638, refused
on 642 against artwork `$68`, reading `Wall { x: 608 }` from 644. Frame 616 is X 546 — inside
the run, before the refusal, and far enough back that the pipe is met under the gym's own
driver rather than on momentum the tape had already spent.

Evidence: `pipe-608` carries the reflex's recorded run-up to 616, golden hash
`v3:0xdcc8e4285344c231`, a cold ledger, objective absolute X 700, budget 600, stall 120.
Under `skills::SCRIPTED` it is `Met` after 108 frames at furthest absolute X 698 with **one**
refusal. Under the same tape with `clear-wall` deleted from the script it is `Stuck` after
142 frames at absolute X 595 with **eighteen**. Same tape, same objective, same budget, same
stall rule, one entry of one library apart. `the_pipe_gym_fails_without_the_skill_that_acts_on_a_wall`
holds the pair; the suite is twenty-nine tests, eleven of which need no cartridge.

The two verdicts are worth less than what is inside them. **The layer that fails is not the
model.** By the second refusal the ledger holds the pipe's pattern and `wall_ahead` names the
column at 608 on the frames he stands in front of it, and the run still never gets its box
past that column: knowing a wall and clearing one are the two layers the whole design rests
on, and until this gym the case for splitting them was prose. What the ablated library does
instead is the part that explains a stop rather than describing one — its transcript is two
`hop-obstacle`s, both from absolute X 594, both about pixel 642, because a koomba
forty-eight pixels ahead is the only affordance in reach that a remaining skill can act on.
The hop carries sixty-five pixels and rises thirty-nine into a four-row column. A wall with
an enemy in front of it looks, to a library with no wall skill, like an enemy.

The `Met` verdict has an ordering inside it that the gym was cut to produce and that is now
measured rather than hoped for: the first behaviour is `clear-wall` on frame 643, from X 594,
about pixel 608 — the frame *after* the game refused him there. The pipe's artwork is not in
the load-bearing set and this gym's ledger starts cold, so nothing in the picture calls that
column solid until the game has said so; the run earns one refusal and spends it. That makes
the verdict a measurement of refusal-then-action, which is the question
`camera-limit-remembering` asked of memory and could not answer, because nothing at the
camera limit had ever been refused.

Consequence: three things, cheapest first. Phase 3's exit is met, and what is left of the
phase is the flagpole — a twelve-row column that is also the level's exit, whose gym
therefore has to start downstream of the pit. `Gym::record` cannot cut that tape because it
hard-codes the reflex as the run-up's player, so recording a run-up with the gym's own driver
is the harness change that the next skill is waiting on. Second, a takeoff that fails its own
postcondition leaves no trace: run past its own objective on this tape and the script starts
`clear-wall` from absolute X 722 about pixel 736 on frames 740, 783 and 826 — three takeoffs
at one pixel — where the level run, on frame 673, cleared that same wall from that same
pixel in one jump. The mechanism behind the difference is unmeasured; what is decided here is that no layer
records that a behaviour was just spent there and did not land, so an envelope can be spent
twice with nothing new known. That is a named hole rather than a flaky run, and it is now a
command line rather than a claim: `transcript` prints a gym's behaviour list past the
objective that stops its verdict, and takes `--without SKILL` so the ablated run can be read
as well as asserted. Third, three of the four gyms in the tree are still about the hole; the
balance is better and not good, and Phase 4's inventory is where that gets fixed by
measurement rather than by intent.

## SMB-025 — 2026-11-21 — A gym's run-up is cut by the driver it will hand the machine to

`Gym::record` runs the gym's own driver through the run-up instead of always the base
policy, and `flagpole-3168` is the gym that needed it: 1-1's exit, the first start state
here that sits downstream of a problem the reflex never solved — and a verdict that says the
thing the plan sent it to measure is not the missing thing after all.

Rationale: the harness hard-coded the reflex as the run-up's player, and the reason written
beside it was sound at the time — a tape cut by the player being tested starts past the
frames that player is supposed to solve, so a gym whose start state already contains its own
answer measures nothing. What that reasoning quietly assumed is that every gym sits where the
reflex stops. The flagpole is the counter-example the plan named: the level's exit is four
hundred pixels past the pit, the reflex spends two lives at the pit, so a reflex-cut run-up
cannot reach the exit at all and the only tape that can is one the script makes. The general
form is the sentence in `Gym::record` now: **a tape the driver cannot run itself describes a
start state its own driver will never be handed.** Phase 2's rule still decides *where* a gym
is cut — upstream of the failure, never at it — and this decides only *who makes the frames
up to there*. Every committed tape stays reflex-cut, because each of them sits exactly where
the reflex stops; the change adds a case rather than replacing one.

Evidence: `flagpole-3168` carries the script's recorded run-up to frame 1900 — fifty-four
button runs, two refusals, furthest absolute X 3005, golden hash `v3:0x97f2d3ee150f8352` — a
cold ledger, `Objective::ClearLevel`, budget 900, stall 600. Under `skills::SCRIPTED` it is
`Met` after 736 frames at absolute X 3267 with one refusal and exactly one behaviour:
`clear-wall` on frame 1971 from absolute X 3154 about pixel 3168. Under the reflex the same
tape is `Met` after 757 frames at 3266 — the first gym in this repository the reflex wins.
Delete `clear-wall` and it is `Stuck` after 666 frames at absolute X 3155 with **ninety-two**
refusals; delete either of the other two skills and the verdict is unchanged to the frame.
`the_level_exit_needs_the_wall_skill_and_not_a_taller_one` holds all of it, and the suite is
thirty tests, eleven of which need no cartridge.

`film flagpole-3168 data/shots/flagpole-3168 760` cuts the run outside the repository, and
`film` marks the frame the gym's own objective first holds as an `event-clear-*` still — the
one `bot --shots` has written since the level run and the gym films did not have — so the
sheet is the jump at 1971 and the ending at 2634 beside each other, which is the finding of
the gym in two pictures.

Two numbers in that set are the finding, and neither is the one the plan expected.

**The flagpole is not a wall to the game.** The script's one jump is taken at 3154 about
3168, absolute X then holds at 3161 for sixty-four frames while the game takes the grab, and
the level ends at 3266 — so the postcondition `clear-wall` claims, "standing past 3168", is
satisfied by a jump that never gets over the top of anything. The reflex arrives the same way
twenty-one frames later on a rule that has no idea what a flagpole is. The twelve-row column
the plan sent this gym to find is real in the picture and irrelevant to the run, because the
height gate that would have refused it is `MAX_WALL_ROWS`, and that is set to eight.

**The cap and the deleted skill are the same measurement.** Setting `MAX_WALL_ROWS` to the
flagpole's honest twelve moves this gym to `Stuck` after 666 frames at absolute X 3155 with
ninety-two refusals — frame count, position and refusal count identical to the ablation
above — and moves none of the other four gyms. A height gate that fires on the only wall in a
gym *is* a deleted skill, and the equivalence is now measured rather than argued, which is
the strongest available reason for the constant's comment: the cap is not hiding a bug, it is
standing in for a skill, and `flagpole-3168` is the pin that says so.

A third number arrived by surprise and belongs to the harness. After the slide he walks to the
castle door, and the game then holds absolute X at 3266 for **four hundred and eighty-eight
frames** before the world and level bytes change. The house stall window of 120 frames calls
that `Stuck` two hundred and sixty-eight frames before the level says it is finished, so this
gym's stall is 600 and its budget 900 — sized by the game's ending and not by the obstacle.
Every `ClearLevel` gym from here has the same requirement, and none of them can discover it
from a bot trace, because in a trace the run looks like it is winning the whole time.

Consequence: Phase 3's flagpole item is closed as a measurement and stays open as a skill.
The gym the tall-wall skill will be scored on exists, it fails today in exactly the shape
that skill has to fix, and the shape is now specific: whatever it postconditions, it must
hold at a column the body reaches and does not clear, on a tape whose objective is the level's
ending. `MAX_WALL_ROWS` stays at eight, with the equivalence above in its comment instead of
a paragraph of prediction. The stall-window rule goes into the plan's standing disciplines,
because it will bite on every level in Phase 4 and cannot be inferred from a bot's behaviour.
And the reflex winning a gym for the first time is worth recording for what it says about the
other four: the script's advantage in this project has never been about walls.
