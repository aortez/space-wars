# NES world-model reader

This document records the investigation into reading semantic game state out of
the Rust NES core so a host can build bots that play a cartridge, using Super
Mario Bros. (`data/roms/smb.nes`) as the target. It is an investigation, not a
feature contract: nothing here has been promoted into `engine-nes` or
`scenario-nes` yet.

The direction from here — the abstraction ladder, a shared skill library,
practice gyms and a search layer over both — is in
[See Mario, be Mario](smb-mission.md).

The conclusion is that **no emulator change is required**. The core already
exposes every channel a world-model reader needs, and the address map, tile
decode, rollout and search design already exist in DirtSim's C++ NES
subsystem. The work is a port plus a place to put it, not new hardware
emulation.

The reader built from it now drives a bot, and that is where the interesting part
is: the same sense data walks Mario into a pipe at absolute X 899 and stops there,
forever, with nothing wrong in the reading. Bolting a reaction rule onto that gets
to 2601 and dies twice, and which of those two it is moves with one tuning
constant. What a reader knows, what it cannot know from the picture, and what that
costs are in "Driving a bot off the reader".

## Read surface the core already provides

| Reader need | Existing API |
| --- | --- |
| CPU RAM | `NesBus::ram`, `NesBus::peek` (side-effect free), `ram_mut` |
| PRG/CHR RAM | `Cartridge::prg_ram`, `Cartridge::chr_ram` |
| Pattern tables | `NesBus::ppu_memory_peek` over `$0000-$1fff` |
| Nametables and attribute tables | `Ppu::nametables` (4 KiB, attributes at `+$3c0`) |
| Sprites | `Ppu::oam` |
| Palette | `Ppu::palette_ram` |
| Scroll latch and control | `Ppu::registers` (`vram_address`, `temporary_address`, `fine_x`, `control`, `mask`) |
| Nametable mirroring | `Cartridge::mirroring`, `Mirroring::map_nametable_address` |
| Frame timing | `Ppu::timing` (`frame_id`, `scanline`, `dot`, `clocks`) |
| Input and stepping | `NesMachine::run_frame`, `run_frame_with_input` |
| Branching rollouts | `NesMachine::checkpoint`, `restore` (allocation-free after construction) |
| Durable rollouts | `NesMachine::save_state`, `load_state` |
| Determinism oracle | `NesMachine::state_hash` |
| Rendered pixels | `Ppu::framebuffer`, `write_rgb888`, `NES_PALETTE_RGB565` |

`scenario-nes` deliberately stays cartridge-agnostic: its `NesObservation` is
frame id, state hash and controller state only. The in-repo precedent for a
semantic observation is `scenarios/falling`, which decodes a versioned
`FallingObservation` from six hardcoded RAM addresses. A world-model reader is
that pattern generalised to arbitrary cartridges.

## Prior art in DirtSim

DirtSim drives the vendored `smolnes` core from
`apps/src/core/scenarios/nes/` and a DFS planner from `apps/src/server/search/`.
Its `smb.nes` test asset is byte-identical to this repository's
`data/roms/smb.nes` (both SHA-256
`f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de`), so its
addresses transfer without retuning.

| DirtSim source | What it is |
| --- | --- |
| `NesSuperMarioBrosRamExtractor.cpp` | The authoritative SMB RAM map and its decoders |
| `NesSuperMarioBrosSetupPolicy.cpp` | Boot contract: one Start press on frame 120 |
| `NesSuperMarioBrosEvaluator.{h,cpp}` | State enum, distance and level-clear reward, termination |
| `NesSuperMarioBrosSpecialSenses.cpp` | 22 normalised senses fed to the policy network |
| `NesPpuSnapshot.h`, `NesTileFrame.{h,cpp}` | Scroll decode, 32x28 tile grid, CHR pattern hashing |
| `NesTileTokenizer.*`, `NesTileTokenFrame.h`, `NesPlayerRelativeTileFrame.*` | Tile-token vocabulary and a player-anchored 63x55 window |
| `NesSuperMarioBrosTilePosition.cpp` | Player position inside the 512-pixel nametable period |
| `NesPaletteClusterer.*`, `NesPaletteFrame.h` | Palette channel kept separate from tile identity |
| `NesRamProbe.{h,cpp}` | Labeled RAM address trace with CSV export |
| `SmbSearchCore.*`, `SmbDfsSearch.*`, `SmbPlanExecution.*` | Macro-action DFS over savestates |
| `Super Mario Bros._RAM map - Data Crystal.txt` | The upstream map this all came from |

DirtSim also already ports two further cartridges with the same shape
(`NesFlappyParatroopaRamExtractor.cpp`, `NesSuperTiltBroGameAdapter.cpp`), which
is the argument for a profile table rather than one SMB-specific reader.

## Super Mario Bros. RAM map

Valid for `Super Mario Bros. (JU) (PRG0) [!]`. All addresses are CPU RAM.

| Address | Meaning |
| --- | --- |
| `$0770` | Game mode: 0 attract demo, 1 normal play, 2 end-world, 3 end-game |
| `$075F`, `$0760` | World and level, both zero-based (`0-0` is 1-1) |
| `$075A`, `$0756` | Lives, power-up (0 small, 1 big, 2+ fire) |
| `$000E` | Player mode: 8 normal, 9 growing, 0x0A shrinking, 0x0B dying, 6 player-dies, 0 leftmost-of-screen, 1 vine, 2/3 pipe, 4/5 autowalk, 7 entering area |
| `$001D` | Float state: 0 grounded, 1 jumping, 2 walked off a ledge, 3 flagpole |
| `$0057`, `$009F` | Horizontal speed (int8, scale 40) and vertical speed (int8, scale 128) |
| `$0033`, `$0045` | Facing and movement direction (1 right, 2 left) |
| `$006D`, `$0086`, `$00CE` | Player X page, player X on screen, player Y on screen. Absolute X is the page shifted left eight bits OR-ed with the screen byte |
| `$000F-$0013` | Five enemy-slot active flags |
| `$0016-$001A` | Five enemy-slot type ids |
| `$006E-$0072`, `$0087-$008B`, `$00CF-$00D3` | Enemy X page, X on screen, Y on screen |
| `$074A` | Buttons the game believes player 1 is holding, in controller shift order (A in bit 7 down to Right in bit 0) |

DirtSim reports only the two nearest enemies by squared distance, which keeps
the sense vector fixed-width.

## Boot contract

The reader has to get into gameplay before any of the map above means anything.
DirtSim's policy is a scripted single Start press on frame 120, gameplay
detected when `$0770` becomes 1, the setup script ends at frame 300 and a run
that has not reached gameplay by frame 500 is a setup failure. The Rust
examples use the same policy and reach gameplay on frame 120.

## Tile-frame reader and ten decode traps

Terrain comes from the PPU, not RAM. The port reads the scroll latch, walks the
nametables through the cartridge's own mirroring function, and identifies each
tile by an FNV-1a hash of its 16-byte CHR pattern rather than by its tile
number. Tile numbers are bank- and game-specific; pattern hashes survive a CHR
bank switch and a different ROM revision. DirtSim's tokenizer then maps hashes
to a fixed-size vocabulary of tokens.

The decode was validated by reconstructing the background pixel by pixel from
the nametables, the mirroring and the CHR pattern, and comparing the resulting
transparency mask against the framebuffer the core actually rendered, with every
cell a sprite touches set aside. At a static camera the reconstruction is exact:
51,136 compared pixels, none wrong. Ten traps came out of it: four from that pixel comparison, two from joining the
terrain to the player, one from driving a bot off the result, one from starting a bot
a frame too early, and two from asking which cell a world pixel falls in — once
through the wrong grid, and once past the edge of the screen.

1. **SMB's blank-sky filler is tile `$24`, not `$00`.** A reader that treats
   only `$00` as empty sees a wall of solid tiles across the whole sky. Hashing
   patterns sidesteps this: `$24` hashes to an all-zero pattern, so "empty"
   becomes a property of the pixels rather than a magic number.
2. **Use the scroll latch `t`, not the current address `v`.** `v`'s Y field is
   incremented on every visible scanline and only reloaded from `t` during
   vblank, so `v` read after `run_frame` returns reports a Y nametable one table
   away from the one that was just rendered. DirtSim's `NesTileFrame` decodes
   `v`, and because the two tables mostly share sky and ground the error is
   invisible in a tile vocabulary and a reward signal; it only shows up against
   rendered pixels. Switching to `t` removed 57 shape mismatches.
3. **SMB scrolls at pixel granularity, not tile granularity.** Observed scroll
   X values include 70, 120 and 170, so a 32x28 tile grid is a sampling of a
   pixel-exact field rather than the field itself. DirtSim keeps both channels
   for this reason: an exact per-pixel pattern field, and a tile-id grid sampled
   at cell centres that drifts up to seven pixels while the camera moves. A
   reader that wants collision-grade terrain has to use the pixel field or
   resample on tile boundaries.
4. **The sprite channel has to be paired with the frame it drew.** At busy
   frames 1-2% of pixels still disagree, spread evenly across columns rather
   than clustered at the newly exposed right edge, which rules out a nametable
   write skew. OAM read after a frame is the sprite list the next frame will
   draw, so sprites that moved or despawned are not masked out. The background
   decode is not at fault, but a reader that consumes sprites has to take OAM at
   the same instant as the framebuffer.

5. **The player's feet are 32 pixels below his stored Y, not at it.** `$00CE` is
   the top of a 32-pixel sprite bounding box, not the surface he stands on. SMB
   lays the player's sprites out as a column of 8-pixel parts starting at OAM Y =
   `$00CE` and leaves the unused parts as the blank `$fc` pattern, so small Mario
   fills the bottom two parts, big Mario all four, and in both cases the surface
   starts at `$00CE + 32`. Reading the stored Y as the standing row puts the
   player four tile rows too high. DirtSim's
   `makeNesSuperMarioBrosPlayerTileScreenY` subtracts only the 8-pixel status-bar
   crop, so it carries the same four-row error; it never bites there because that
   value anchors a player-relative window where a constant bias is absorbed by
   training, and it bites hard the moment a bot asks "is there ground under me".
   Two independent measurements agree: sweeping every 8-pixel offset across 2583
   grounded frames leaves `+24`, after the crop, as the only row that is solid
   under the feet with the body's two rows clear, and the player's own OAM column
   ends on the row the constant predicts on 1008 of 1008 frames where his sprite
   is identifiable.
6. **Opaque is not solid.** The nametable records what is drawn; the game
   collides with a far smaller set of ids. SMB paints its hills and bushes with
   fully opaque tiles and walks Mario straight through them, so `is_blank_tile`
   reports scenery as terrain. Recording which id sat under his feet on frames
   the game called grounded, against which ids his body occupied, split the two
   with no overlap across 2583 frames: `$26` was a body tile on 2413 of them and
   a feet tile on none, while the `$b4`/`$b5` ground, the `$60`-`$63` pipe tops,
   and the `$45`, `$57` and `$58` solid blocks (each drawn as a lit top edge over a
   filled body) only ever held his feet. A reader that derives walkability from
   pixel coverage parks a bot inside a hill.
7. **Standing on something and being stopped by it are different observations, and
   a feet-row survey can only make the first.** `LOAD_BEARING_TILES` is learned by
   recording which id sat under the player's feet on grounded frames, so it can
   only ever contain surfaces. A pipe is `$60`-`$63` on its top row, which is in
   the set, over `$64`-`$6a` for its body, which is not, so every row that
   actually stops him reads as scenery. The bot built on this reader walks into a
   pipe body at absolute X 899, is stopped by the game, and stands there pushing
   right with `wall_ahead` false for 600 frames. Solidity is a relation over tile,
   direction, player state and time, not a property of a tile id: a one-way
   platform is solid from above and not from below, a `?` block changes id when
   hit, and breakable and unbreakable bricks share a picture. Learning wall ids
   needs a different experiment, walking into things, or the ROM's own collision
   table. Walking into things settled it, without reading the table: see
   *Solidity from a refusal* under the bot section.

8. **`$0770` says play before the game has handed him over.** The mode byte flips
   to `$01` on the first frame of a level load, and the load takes about 150 more
   frames to place the level and put the player in it. Through the first two of
   those the player bytes still hold the values from the screen that was showing
   before, so one can read "state `$08`, grounded, absolute X 40, tile row 25" for
   a level that has not loaded yet; the level then starts him at absolute X 0 in
   state `$00`. A policy started on the mode byte spends its first two seconds
   pushing at a world that is not listening, and a stuck detector counted in frames
   reports that as the bot giving up before it moved. `SmbState::is_in_control`
   waits for the state to hold for eight consecutive frames rather than for it to
   flicker once.

9. **The decoded grid is indexed by screen column; the level is laid out in world
   pixels.** `tile_grid` samples the picture at cell centres, so its column 14 is
   whatever is under screen pixel 116, and screen pixel 116 is world pixel
   `scroll + 116` for whatever the fine scroll happens to be. The level's cells sit
   on world multiples of eight. The two agree only when the fine scroll is zero, so
   asking the grid "which cell is this pixel in" is up to one column out on seven
   frames in eight. That is not a rounding complaint: it is why a pipe stopped the
   bot with `wall_ahead` false *after* the reader had been refused by that same
   artwork. The refusal was attributed to the right pattern and the look-ahead was
   measured against the wrong grid. A cell at a world pixel is read through
   `tile_at_world_pixel`, which follows the PPU's own mirroring from the world
   coordinate instead of through a column number.

10. **A world-pixel lookup past the camera's window folds back onto the screen.**
    `tile_at_world_pixel` follows the PPU's own mirroring from a world coordinate, and
    the mirroring is a modulo over a nametable 32 columns wide, so a pixel 256 or more
    past the scroll origin names a column the camera is already showing near its *left*
    edge. The level's cells out there are real, but they are not in the picture the PPU
    is holding, and asking the PPU a wider question does not fetch them: it answers a
    different one. Reading the support row ahead of the pit at world pixel 1379 first
    reported a second hole 256 pixels wide that does not exist; the lookup now returns
    `None` past `origin + 256` and the walk stops there and says where it stopped. The
    cells past the screen are Phase 4's arbitrary-level start, not a wider loop.

The attribute table is a separate channel again: 20 of 884 sprite-free cells in
the sample frame render the same pattern under different palette indices. Tile
identity must not absorb palette, which is why DirtSim clusters palettes
separately.

## What the reader actually says the world is

Decoding 1-1 through the reader, at scroll `(0,0)` with Mario standing at tile
`(5,25)`, gives a picture worth stating plainly, because much of it is not what
a reader that trusts its own eyes would expect.

The visible 32x28 grid is: sky everywhere above row 20; a hill of `$31`/`$32` at
its apex in columns 4-5 of row 20, widening through `$30`/`$26`/`$34`/`$33` until
it fills columns 0-9 of row 24; three bushes, each a `$36`/`$37` pair in row 23 at
columns 24, 26 and 28, sitting on a `$35` then six `$25` then `$38` across row 24;
and three rows of ground across the full width at rows 25-27: `$b4`/`$b5`
alternating on the surface row 25, `$b6`/`$b7` on the subsoil row 26, and
`$b4`/`$b5` again on row 27. Thirty-six distinct tile patterns appear on this one
screen.

Three things follow that a reader has to get right:

- **Terrain is not solidity.** Rows 23 and 24 above Mario's feet are opaque hill
  tiles he is standing inside, and the game does not care (trap 6). The
  load-bearing set is nine ids wide; the drawn set is thirty-six.
- **The nametable is a sliding ring buffer, not a level map, and a bounded one.**
  SMB rewrites nametable columns as the camera scrolls and the pair of tables
  wraps every 512 pixels, so the leftmost columns of page 0 end up holding content
  from X = 512 and beyond. `smb-model`'s `rollout` watches every slot the camera
  passes over: over 320 playing frames to absolute X 705, 1,652 of 1,792 slots
  (92%) kept one tile id the whole time, and of the 144 changes seen in a slot the
  player could still see, 140 were at column 31, the edge the camera is exposing,
  which is new terrain arriving rather than old terrain edited. Only four landed in
  a column he was looking at, which is what a hit `?` block looks like. So a reader
  can trust terrain it has already seen, and it cannot see further than about one
  screen ahead: "what is over the next hill" needs the camera moved to find out. Because the period is 512 pixels and that run reached
  705, a slot is not a stable address for a level cell, and a map has to be keyed
  by absolute X and stitched from windows.
- **`$0086` is position within the 256-pixel page, not relative to the camera.**
  On-screen position needs `wrapped_delta(absolute_x, scroll_x)`; subtracting the
  scroll from the screen byte mixes two different origins.

The dynamic half is thinner than the terrain half. The game keeps five enemy
slots live at once, so the reader sees at most five enemies, and only the ones
the game has spawned, which is a screen or so ahead of the camera. In a probe run
that reads as a goomba (type 6) 110 pixels ahead, then 10, then gone.

## Driving a bot off the reader

`crates/smb-ai/examples/bot.rs` is the reader's first consumer: a policy that decides
every frame from the reader alone. It senses the tile grid, its own cell from the feet
offset, and the enemy slots, and knows nothing about 1-1's shape beyond the
load-bearing ids the reader learned by watching. The policy is the simplest one that
model supports: run right, and jump a hole, an enemy or a cell the model can call a
wall. Since Phase 1 the last of those includes cells the game has already refused to
let him through, which the reader cannot see at all. It lives in `smb-ai`'s `policy`
module rather than in the example, so that the example, the optional test and the gym
harness cannot drift apart on what "acting on the model" means.

Measured on 1-1, release build, repeatable to the frame:

| Policy | Furthest absolute X | How it ends |
| --- | --- | --- |
| reader only | 899 | pushes right against a pipe for 600 frames, no deaths |
| reader plus a blocked-means-run-at-it reflex, since deleted | 2601 | two deaths, then the game ends |
| reader plus a ledger of the game's refusals | 1553 | two deaths at a screen lock, then the game ends |

The middle row is kept as history, because it is the measurement that justified
deleting it. The reflex was the cheapest thing that stood in for the missing wall ids:
if the controller says right and the world will not move, back off for 24 frames, run
back at it, and jump whatever drawn thing appears within reach in the rows the body
occupies. That signal is not "a wall", because opaque is not solid (trap 6); it is
"something is there and the model cannot say what", which is the most an honest reading
of the nametable offers. What it bought was distance, and how much distance depended on
one tuning constant:

| `--reflex-look` | 3 | 4 | 5 | 6 | 7 | 8 | 9 and above |
| --- | --- | --- | --- | --- | --- | --- | --- |
| furthest absolute X | 595 | 723 | 899 | 899 | 2601 | 899 | 2601 |

From 7 columns the answer stops changing, because the window is then wider than
anything the level puts between the player and the obstruction. Nothing about the
reader differs across that table. A reaction rule bolted onto a model with a hole in it
is a lottery, and an earlier draft of this document, which reported the reflex clearing
1-1 at absolute X 3266, had drawn one winning ticket: that build pressed A one frame
later than this one does, and what it won at the third pipe it lost at the first. The
rule is deleted, not tuned, and `--reflex-look` went with it.

### Solidity from a refusal

The core already knows whether it stopped him. When SMB blocks a body it takes the
motion back out of the speed, so the difference between the motion the buttons asked
for and the motion that came out is a verdict from the game rather than an inference
from the picture, and it can be attributed to the cell the body is pressed against.
`Resistance` in the reader records that verdict and nothing else, on three conditions
that are all the game's own rather than a threshold: the direction was held without its
opposite, the speed the buttons had built up came back exactly zero, and the body sits
where a cell could have left it.

The third condition is the one that took a measurement to get right, and it used to read
"the body gained no ground to the pixel". That is what pushing looks like *from the
ground*, and it is not what a refusal is. On the frame a body is stopped the game clamps
it **to** the wall, so the position still moves; on the frame after, the speed the
detector would have asked about is already zero. A refusal lasts one frame and the old
test looked for it on neither of them. Survivable while a body walks into a wall, fatal
when it arrives in the air: measured over six levels under the reflex, 1806 frames carry
the zeroed-speed signature, and in 2-2 and 6-2 — the two levels where both players end up
pressed against a pipe body — **not one of them is grounded on both sides of the frame**,
because a body that reaches a wall by jumping is never standing on the frame the game says
no on. The ledger recorded nothing in either level, so `wall_ahead` went on calling those
pipes scenery six hundred frames later and the script hopped at them until its budget ran
out.

What replaces it is where the game leaves a body it has stopped: flush against a cell
boundary, with the cell that refused it on the far side of that boundary. The two
refusals that fixed the body reach above are the first two samples — held at 594 against
a pipe edge at 608, at 898 against an edge at 912 — and both leading pixels land on a
multiple of eight because the box reach is the remainder. Over twelve runs of six levels
with both players, 5098 of the 5123 frames that carry the zeroed-speed signature with
something drawn at the leading edge are flush. The 25 that are not split in two, and the
split is the whole reason the condition is not just the flush test:

| where the leading edge sits | frames | what it is |
| --- | --- | --- |
| flush on a boundary | 5098 | the game, having clamped the body to the cell |
| 1–2 px past a boundary, in the air or on the ground | 16 | the frame a body first touches a pipe or a block, before the game pulls it back |
| 5 px past a boundary, at one scroll limit in 5-1 | 9 | the screen edge, with ground drawn beside it |

A touch frame counts only while the body is in the air, which is a completeness argument
and not a tolerance. On the ground the game holds a body at the boundary for as long as
the direction is held, so the flush frame follows and the touch is a second report of a
cell already reported; in the air the speed comes out, the body falls away, and the
boundary is never reached. The two cells the reflex learns in 1-1 about the pipe body at
absolute X 912, and the one it learns in 6-2 about the solid block at 816, are named by
airborne touch frames and by nothing else. The bound on how far past a boundary such a
frame may sit is the distance the body covered that frame — the most an entering body can
get past an edge it has not been pushed back from — and it is what rejects the clamps: a
body pinned to the edge of a locked screen has covered nothing, so it can claim no cell.
That is 4 frames of the camera-limit gym, six pixels past a boundary, and 87 at the 5-1
scroll limit, five past one, which the old position test caught only by accident and the
blank-cell rule does not catch at all, because those frames have ground drawn beside them.

What the signature actually looks like is a sawtooth. Pinned against the first pipe his
speed runs `0, 1, 2, 3, 0` while his position does not move at all, and against the
second `0, 1, 2, 3, 4, 0`. Only the frame that resets to zero satisfies the speed
condition, so one contact records one refusal rather than one per frame of pushing.

The verdict is stored against the CHR pattern hash of the tile that refused him, the
row of his body it was pressed against, and the direction: not a tile number, and not a
position. The pattern is what makes it generalize. The game refused him at absolute X
594, and every cell drawn like that became a wall, which is what the second pipe, 304
pixels further along, is drawn like.

Two measurements had to be right for the attribution to mean anything, and both were
wrong at first.

- **How far the body reaches.** `$0086` is the left edge of the sprite, and the box the
  game stops against the level is not the sprite. Two independent refusals fix the
  number: the game holds him at absolute X 594 against a pipe whose left edge is at
  608, and at 898 against a pipe whose left edge is at 912. Both leave exactly 14
  pixels, so the box is `[x, x + 14)`, the last pixel he occupies is `x + 13`, and the
  cell that can refuse him is the one containing `x + 14`. The number in the reader
  before this was 11, read off the decoded grid rather than off the level.
- **Which cell that pixel is in.** Asking the screen-indexed grid is trap 9, and the
  answer is up to one column out on seven frames in eight. The refusal was being
  attributed to the right artwork while the look-ahead was measured against the wrong
  grid, which is how a bot can be stopped by a pipe it has already learned to call a
  wall. Cells beyond the body are read through `tile_at_world_pixel` now, which follows
  the PPU's own mirroring from the world coordinate instead of through a column number.

Whether that counts as memory or only as reaction needs an A/B whose single variable is
the memory, and building one needed a construction the level does not supply by accident.
`smb-ai`'s `solidity` runs four passes. Pass 1 runs cold and keeps every refusal the game
makes; the lesson it teaches is the first refusal whose *artwork* turns up again at least
180 frames later, because the claim is about recognising a drawing and the level has to
draw it twice for that to be a claim — 1-1's first two refusals are airborne block bumps
at absolute X 306 and 338 whose patterns never come back. What does come back is the pipe
body at tile `$68`, refused on frame 642 at absolute X 594 and drawn again on frame 856 at
899. Pass 2 runs cold again and takes its checkpoint sixty frames *before* that second
meeting, on frame 796 at absolute X 690, so the lesson is behind the start state and the
obstacle is in front of it. Passes 3 and 4 restore that one checkpoint and run the same
691-frame tail, one with an empty ledger and one holding every refusal the taught tile
made. Both tails start from the same bits, and the thing they differ on happened before
either of them began.

| One 691-frame tail from the same frame 796 state | First cell the ledger calls a wall that the tile ids do not | Refusals in the tail | Furthest absolute X |
| --- | --- | --- | --- |
| nothing remembered | frame 865 at absolute X 898, flush against it, after being stopped | 46 | 1192 |
| the taught tile's refusals | frame 853 at absolute X 890, three frames and nine pixels before contact | 2 | 1544 |

The memory is the difference between clearing the second pipe and spending the tail
against it: three hundred and fifty-two pixels further on, and two refusals instead of
forty-six. It is not immunity — the remembering tail is refused twice, because a jump
started three frames from contact still touches the pipe on its way up — so what the
ledger buys is the run-up and not a reprieve. What it is not is a reaction to what is in
front of him, because the wall it names is a different pipe than the one that taught it.
`remembering_a_refusal_names_a_wall_the_tile_ids_do_not` keeps that comparison in CI,
including the exactness of the restore it is built on and the half of it that says the
remembering tail gets further.

The same run drawn on the picture is the shorter argument. At frame 641 he is pressed
against the first pipe with no orange dots anywhere in the frame: the model has nothing
to say about what is stopping him. Frame 642 is the refusal. Frame 643 has two orange
cells and a jump. At the second pipe, frames 820 to 860 carry orange dots with no
refusal event between them at all. At the screen lock that kills him at absolute X 1544
there are no orange dots either, so the model is exactly as silent about that as it was
about the pipe, which is the next hole named. Those dot counts are read back out of the
PNGs rather than looked at.

How far ahead the ledger is trusted turned out not to be a knob, which is the whole
difference between it and the rule it replaced. `WALL_LOOK_COLUMNS` is 1, because a body
flush against a wall has no run-up left and a jump started flush clears a two-cell pipe
only just, and the bot's outcome is identical at 0, 1, 2, 3 and 5: 1553 and two deaths
every time. The deleted `--reflex-look` swung from 595 to 2601 over the same range. A
model that names artwork can be trusted one cell ahead because it says what a cell is
before he reaches it; a rule that only knows something is there cannot.

Three findings belong to the bot rather than the reader, and all three were invisible
from outside the run.

- **A one-frame A press is a hop.** SMB's jump is variable-height. The first version
  pressed A for a single frame, cleared nothing, and read as a model failure; the same
  policy with A held for 20 frames went from absolute X 436 to 899 with no change to
  the reader at all.
- **The frame a press lands in is part of the action.** The reflex set its jump hold
  after the frame's buttons had been built, so A reached the game one frame late. That
  one frame is the difference between the two numbers in the table above, which is a
  more useful result than either. An action space has to be specified to the frame and
  measured, not assumed, and a decode gap and an action bug look identical from
  outside.
- **A bot pinned flat against a wall has no run-up.** Jumping straight from blocked
  clears nothing, which is why the reflex had to leave the wall before it could get
  over it. The ledger does not need the escape, because it stops him arriving flush in
  the first place: the measured jump starts seven pixels out.

Two tests keep the bot's half of this honest, one on each side of the memory.
`the_tile_that_stops_the_player_is_not_one_the_reader_calls_solid` plays the policy with
an empty ledger, waits for 120 frames that cover no new ground, reads the first cell
ahead that draws anything in the rows his body occupies, and asserts that the game
stopped him on a tile the reader does not call load-bearing while `wall_ahead` said
nothing was there; if the load-bearing set ever grows a wall, that test says so and
points at trap 7. The other test is the same cell one memory later.

What the reader cannot see fits in one picture. `--shots DIR --overlay` writes a PNG of
every frame with the model's own opinion dotted on: green where a cell is
load-bearing, orange where the game has refused to let him through a cell of that
artwork, red where a cell draws something the model calls neither, blue on the cell he
is standing in. A red dot against a Mario who will not move is trap 7 in one image, and
watching that dot turn orange on the next pass is what learning looks like in the same
picture. Frames are named for what was happening (`event-jump-*`, `event-refusal-*`,
`event-death-*`, `event-game-over-*`), and `tools/nes-reference/film-smb-run.py` turns a
shots directory into two films that stay in step (plain, and the same frames with the
dots on), short overlay clips over a frame range, and one contact sheet of the event
stills in the order they happened. The plain series is thinned to every Nth frame by
`--every`, so its film runs at 60/N to stay in real time while the overlay series keeps
all 60. The tool then decodes frames back out of the film it just wrote and counts the
dots again, one frame for each kind of dot the run ever drew: a one-pixel mark is
exactly what a lossy encoder discards, and it does. At x264's default quality a frame
with 36 load-bearing dots and 115 opaque ones came back with 16 and 102 of them, so the
overlay films are capped at crf 12, where every dot comes back. A film that quietly
lost the overlay still looks exactly like a film.

An earlier draft reported this bot reaching world 1-2 and going blind there. The
current code dies before the flagpole, so the blindness is an argument again rather
than a measurement: the load-bearing set is 1-1's vocabulary, gathered by watching
what one level holds the player up on, and an underground area is drawn with ids it
never saw. Measuring it needs a way to start a run in 1-2, which the reader does not
have yet.

### The first gym, and what it measured

Phase 2's harness is what turns that stop into a question that costs seconds: a tape
from power-on to frame 1167, an objective, a frame budget and a verdict. The tape is 32
button runs, cut by `cargo run -p smb-ai --example gym -- --record 1167`, which
replays what it recorded before printing it, so the run-up committed is the run-up that
produced the verdict printed beside it. It ends on state hash `v3:a73cebb1ed214d50`
with absolute X holding at 1544 and the game still holding the controller. That hash is
this project's first golden hash: a core change that moves SMB out from under the tape
now fails as `tape replayed to … where it was recorded at …: the core moved, not the
bot`, in those words, rather than as a strange result two hundred frames later.

Phase 1's loose end is in the type: a start state is the machine's frame *and* what the
model remembers, and only one of them survives a checkpoint, so a gym names which of
the two it carries (`LedgerSeed`) and the two become two gyms rather than one gym with
a hidden input.

| Gym | Model memory it starts with | Verdict |
| --- | --- | --- |
| `camera-limit` | nothing: the run-up's refusals are wiped | `Stuck` after 120 frames, absolute X 1544, 0 refusals |
| `camera-limit-remembering` | the run-up's one refusal, rebuilt by watching the replay | `Stuck` after 120 frames, absolute X 1544, 0 refusals |
| `pit-1379` | nothing, and the stop is 153 pixels ahead of where it starts | `Stuck` after 248 frames, absolute X 1544, 0 refusals |

Same tape, same buttons, same budget; the only difference is what the model arrives
knowing. Measured, that difference is nothing, and that is the finding rather than a
null result: the thing at the camera limit never refused him, so there was never a
lesson for a ledger to hold. A model whose only source of walls is the game's own
refusals cannot be warm about a stop it was never given, which puts the next hole in
sharper terms than the overlay did: a wall has to be readable from the picture *before*
contact, not only remembered after it.

The third row is the same stop cut somewhere else. `camera-limit` is cut at frame
1167, which turns out to be inside the fall it asks about, and the section after this
one is what that discovery cost and bought. `pit-1379` is the same run stopped at frame
1040 — absolute X 1226, at full running speed, mid-jump, 153 pixels short of the hole,
coming down on frame 1060 at 1276 — and it is the first gym here a bot can win from. The
base policy still loses from it, `Stuck` after 248 frames at absolute X 1544, so it is an
open problem rather than a formality; what it takes is measured below.

`replaying_the_same_tape_twice_lands_on_the_same_bits` is DirtSim's
`ReplayFromRootIsDeterministic` transplanted: replay one tape twice, run 60 frames of a
single button from each landing, and require the same absolute X on every frame and the
same final hash. It is here because a gym that drifts by a frame measures nothing, and
because it is the check that a bot's buttons reach the game in the order they were
decided: the core latches a controller on the game's own strobe, so an input that
arrived at the wrong moment inside a frame would surface as two replays of one tape
disagreeing.

`rewinding_to_the_end_of_a_tape_lands_on_the_same_bits_as_replaying_it` is the same
argument applied to `checkpoint`/`restore`, and it exists because the instrument below
rewinds to the end of a tape between samples instead of replaying it. A checkpoint that
quietly forgets one timer or one enemy slot does not fail: it moves the run by a frame,
every sample inherits the move, and the commit window the sweep reports belongs to a
game nobody is playing. So the test plays forty frames past the end of the tape,
restores, requires the tape's own golden hash, and then plays sixty frames from the
restored machine and sixty from a fresh replay and requires the same absolute X on every
frame and the same hash at the end. Frame zero is not the interesting comparison — a
restore always agrees there.

### The pit at 1379, and what reading it took

`camera-limit` is a name read off an overlay, and an overlay cannot tell a wall from a
death. Absolute X holding, the player's cell pinned at the right edge of a screen that
never scrolls, and a picture that does not change by a dot: that is what a locked camera
looks like from inside the overlay, and it is also what falling off the end of the world
looks like. The two need opposite skills, so the difference is worth an instrument. The
instrument is `smb-ai`'s `limit` example — it cuts a run-up to any frame with the base
policy, rewinds to the end of it between samples rather than replaying it, and asks the
same question of the game and of the picture.

Read by world pixel rather than by screen column, the support row under him stops and
starts again:

    #  .  .  .  .  .  .  #  #  #  #  #  #  #  #  #  #
    b5 -- -- -- -- -- -- b4 b5 b4 b5 b4 b5 b4 b5 b4 b5

from the last frame the game called grounded — absolute X 1371, feet row 25, screen
column 15 — sixteen columns of eight pixels. `#` carries his weight and `.` does not;
the hex under each column is the first cell in rows 25..27 that draws anything, and
`--` where none does. The gap is 48 pixels wide, six cells, and it starts eight pixels
ahead of him; the ground returns at 1427 wearing the same `$b4`/`$b5` artwork it wore
before. The `--` is the finding. A gap whose cells hold an id the load-bearing set has
never been shown is a hole in the model, which is trap 6's failure and wants a wider
set. A gap whose cells draw nothing is a hole in the level, and wants a bot that jumps.

The game's own side of it, under one fixed input of right and B: grounded at full
running speed (speed byte 40, two and a half pixels a frame) until frame 1100; walked
off at absolute X 1376 in float state 2, which is the game's own "walked off an edge"
and not "jumped"; X still advancing while he falls until the game clamps it at 1549
against the screen edge, camera origin 1280 and screen X 13; player state `$06` on frame
1379, a life gone on 1380, reset on 1382. The two live enemy slots sit 101 and 125
pixels *behind* him the whole way down, so nothing touches him.

Five traps came out of measuring the jump rather than the hole.

- **The game takes its time about a pit, and the delay is itself a signal.** He leaves
  the ground on frame 1100 and the state leaves `$08` on 1379 — 279 frames. A state
  change ten frames after a landing is a different event, and neither one shows up in a
  position. So a caller that wants the game's verdict has to wait for it, which is why
  the sweep watches each sample for 420 frames and not 200: at 200 a death in the hole
  and a jump still in the air print the same line, and the first version of this table
  could not tell them apart.
- **There is no `has_fallen_off_screen`, and DirtSim's terminal test does not
  transfer.** The Y byte is the low byte of a position that keeps counting, so it wraps
  every 256 pixels of a fall — the timeline above walks it twice on the way down. A
  comparison against 224 is true for about eight frames in every sixty-four and false
  for the rest. `SmbState::is_dying` is the test that does transfer, and it is DirtSim's
  own decode: `$06` and `$0b` both mean a life is going, and the life counter lags both.
- **Track the apex only while the game says he is rising.** Same wrap: a jump that ends
  in the pit otherwise reports its apex off the third lap of the fall.
- **A landing with one foot on the far lip is standing to the game and in the hole to
  the map.** The longest jump the envelope allows comes down at 1414, thirteen pixels
  short of the far edge at 1427, and a cell lookup calls that the hole. The game calls it
  standing, because his box is `[x, x + 14)` (SMB-017) and its right edge reaches the
  solid cell at 1427 — and he then walks on to 1429 still grounded. A sweep scored off
  the picture fails the one press that works, so every sample is scored by what the game
  allows *after* the landing.
- **A is only a cause if he left the ground on the frame A went down.** He is already
  airborne on frame 1100, so a press after that is a jump requested during a fall. The
  one sample whose A goes down while he is still in the air — press 20, a few frames
  before he comes down — gets no jump at all: the game takes the press and does not
  spend it when he lands. The sweep refuses any sample where the ground left first.

Then the envelope, from a full-speed run, A held for N frames:

    hold  takeoff         apex  head  landing          air  span  rose
       4  f1069@1299      y138    96  f1094@1361        25    62    38
      20  f1069@1299      y101    96  f1115@1414        46   115    75

Held 4 frames the jump carries 62 pixels, he lands at 1361 and walks off at 1374
anyway. Held 20 it rises 75 pixels, stays airborne 46 frames and carries 115. Held 8,
12 and 16 he never lands at all: the game takes his life at the bottom of the fall, on
frames 1376, 1381 and 1386, with the two enemies still behind him.

The hole is not 48 pixels of jump away, though. The far edge is at 1427 and the box the
game stops him against is `[x, x + 14)` (SMB-017), so the last pixel he occupies is 13
past his position and a landing at 1414 already stands on it — thirteen pixels short of
the edge the picture draws. A jump taken at 1299 therefore has to carry 115, and the
envelope gives 115. That is the whole margin, and it is why the commit window starts on
the frame it starts on rather than somewhere comfortably before.

Then the sweep, which asks the other half: not how far a jump carries, but which frame is
worth spending one on. A goes down on frame 1000 + N and is held for the twenty frames the
envelope says is enough, two frames at a time from 20 to 140, every sample scored by the
game over the next 420 frames:

    press  takeoff        apex  head  landing          air  span  rose
       22  f1023@1184     y90     96  f1080@1326        57   142    86
       32  f1033@1209    y144     96  f1053@1259        20    50    32
       52  f1053@1259    y101     32  f1098@1371        45   112    75
       66  f1067@1294    y101     96       -            -     -    75
       68  f1069@1299    y101     96  f1115@1414        46   115    75
       98  f1099@1374    y101     96  f1144@1486        45   112    75

A on presses 68 to 98 inclusive gets him across, thirty-one frames wide, and every other
press in the sweep is the game saying no. The sweep steps by two, so the true edges are
within one frame of those numbers. Thirty-one frames at two and a half pixels a frame is
seventy-eight pixels of approach — wide enough to be a skill rather than a reflex, narrow
enough that a bot which jumps on sight of the hole is still jumping too early.

Below the window the presses fall into three kinds. From 22 to 52 the jump lands short and
he walks off the near lip on frame 1099 at 1374 — the same frame, to the frame, that the
fixed input reached it. From 54 to 66 it is the same jump that crosses, apex y101 and a
hundred and fifteen pixels of carry, taken fifteen pixels earlier, so it arrives at 1379:
the hole's own near edge, and he comes down in it, which the game only admits 279 frames
later. From 100 on A goes down while he is already in the air and the game never spends
the press. Presses 24 to 28 are the sweep's accident: they land him on a raised surface at
y112 where two enemies stand at +7 and +46 pixels, and that run ends for a reason with
nothing to do with the pit.

The `head` column is the picture's answer to the same question, read off the nametable —
clear air above his head on the frame he leaves the ground, capped at the twelve rows the
lookup reaches. It is there because the button is not the variable. Held the same twenty
frames, the jump rises 86 pixels from a takeoff at 1184, 32 from one at 1209 and 75 from
one at 1259. The rise is cut across the same stretch of level the headroom reads 32
across, the two columns offset by the twenty-odd pixels he covers while rising: the
headroom is read where he is when the button goes down, and the rise is decided where he
is when he reaches the top. Either way the conclusion is Phase 3's, not Phase 4's — an envelope measured at one
frame is a statement about that frame and not about the button, and a skill that carries a
single jump height is wrong about most of the level it has to cross.

The same sweep from the committed cut at 1167 has nothing to sweep: sixty-one presses, and
none of them leaves the ground, because he has been off it for sixty-seven frames before
the first sample is taken. That is SMB-019's rule in one line of output.

That makes the stop the first thing in this project that is a skill's problem rather
than the model's. Nothing refused him at the pit, so there was never a lesson for a
ledger to hold; the model has been able to say "no floor for 48 pixels, eight ahead"
since trap 9 landed, and says it correctly. What was missing was a bot that spends a
jump on the right frame, and a gym cut where the frame is still in front of it.

### The first skill, and what measuring it took

Phase 3's first entry exists, and it wins its gym. `crates/smb-ai/src/skills.rs` holds
two named behaviours — `clear-gap` and `hop-obstacle` — over two measured envelopes, and
`pit-1379` now carries the script's verdict: `Met` on frame 151 at absolute X 1599 with
no death, where the same gym under the base reflex is `Stuck` after 248 frames at 1544.
One field of the gym differs between those two runs — its driver. The tape, the
objective, the ledger seed and the budget are the same bits.

| Envelope | A held | Air | Carry | Rise | Headroom | Measured from |
| --- | --- | --- | --- | --- | --- | --- |
| `long-jump` | 20 frames | 46 | 118 px | 75 px | 96 px | `pit-1379`, 28 frames in |
| `hop` | 4 frames | 25 | 65 px | 39 px | 32 px | `pit-1379`, 15 frames in |

`skills::measure` produced every number in that table by replaying the tape, running a
run-up, pressing, and watching where he left the ground and where he stood on something
again. It is the instrument a future skill gets measured with, which is why it is code
rather than a table someone typed.

Four things came out of the measuring that the design did not predict.

**A carry is a range, so an envelope names the frame it was measured from.** The same
twenty frames of A carry 118 pixels from the frame `long-jump` is pinned to, 117 one
frame earlier, and a flat 115 from every frame from thirty on: one frame of flight
either way, which is three pixels of landing. The 115 this document quoted for the pit
is the same jump measured from the *takeoff* rather than from the last frame he stood on
anything, and the three pixels between the two numbers are the frame in between. A skill
reads its envelope against the position it is standing on, so which frame a number was
taken from is part of the number.

**A landing is judged by the last pixel of the box, and one pixel of that is still open.**
The shortest crossing the commit sweep scored came down with him at absolute X 1414 over a
hole whose ground returns at 1424, and it stood, so a landing judged on the position byte
alone refuses every jump the game allows. The sweep re-run at one-frame resolution put the
game's own edge between a landing at 1409, which falls, and one at 1411, which stands. The
box reaches fourteen pixels past his position, so the pixel that can be inside a cell and
counted is `x + 13`, and that answers both the way the cartridge does: 1411 + 13 = 1424,
which is the ground, and 1409 + 13 = 1422, which is the hole. A landing at 1410 has never
been pressed, so one pixel of ambiguity is open here and is recorded rather than argued.
It sits beside a second, larger one: the reader's cell-aligned far edge is 1424 and the
instrument that walks pixels puts the lip at 1427. The measured window is the arbiter
between them, and the last-pixel convention reproduces both of its bracketed points.

**The picture ends two pixels short of a long jump's landing.** At running speed the
camera pins him at screen column 126 or 127 and the reader reaches thirty-two columns, so
the picture stops one hundred and twenty-eight or one hundred and twenty-nine pixels past
his position. The long jump's box comes down at one hundred and thirty-one. The direct
question — is the last pixel of the box over something load-bearing — has no answer there,
and the first version of the library asked nothing else and refused every long jump it was
offered and walked into the pit. The fallback is the hole's own reading: a hole whose far
edge the picture showed has already said where the ground comes back, and a box that
reaches past that pixel is on ground. Probing one pixel earlier instead, at the last
readable column, alternates between an answer and none as the fine scroll moves, which is
the edge itself flickering. What that costs is a standing limit, not a workaround: at
running speed this model can never *see* a long jump's landing, only infer it from the
hole it is crossing, so a level whose far lip is not the ground return — a ledge over a
pit, a platform standing in a hole — needs the level memory Phase 6 is about rather than
a wider look.

**A skill has to ask whether the world has already granted its postcondition.** A long
jump that comes down with one foot on the far lip leaves him standing at absolute X 1414
over a hole whose ground returns at 1424 — standing to the game and in the hole to the
map — and the hole reading from there is a nine-pixel hole still ahead of him. Without
that check the script asked for a second long jump on the frame it landed, from inside the
hole it had just crossed, and survived only because the jump happened to carry.

**And the ablation says the hop is load-bearing somewhere other than where the gym is
named.** Take `hop-obstacle` out of the script and the pit is still crossed, from the
first frame the far lip is readable at absolute X 1296; the run then stops dead at 1526,
two hundred and eighty-six pixels past the pit, on a pair of koopas. Put the hop back and
it fires twice — frame 1055 at a koomba twenty-two pixels ahead, where this run did not
need it, and frame 1147 at forty-three, where it did — and the gym is met at 1600. The
enemy the base policy flinched at, the one the pit tape's jump was spent on, is not the
hop's reason; the koopas past the lip are. Phase 3's exit asks each skill for a measured
envelope beside it, and what this adds is that it should also ask for a gym that fails
without it, which for the hop means enemies on flat ground and is not in the tree.

The script is not allowed to be lucky: `tests/skills.rs` requires it to commit inside the
window the game scored, 1294 to 1371, and it commits at 1296 — the first frame the far lip
is inside the camera's window at all, which is the first frame anything reading the screen
could have made this decision.

### The library plays a whole level, and two walls teach it to name them

The reflex has never got past the pit, so the script had never been asked to play a level
either. `bot 2620 --script` now clears world 1-1: absolute X 3266, no death, 376 frames with
A held, two refusals, and twenty-six named behaviours. The same instrument under the reflex
reaches 1553 and spends two lives there, which is the pair the whole Phase 3 argument needs
and it is now a pair about a level rather than about a hole in it.

The `bot` example prints the transcript, because a library with no record of what it chose
has no evidence about itself:

```text
frame   406: hop-obstacle  from absolute X   282 to get past 328
frame   477: clear-wall    from absolute X   434 to get past 448
frame   569: clear-wall    from absolute X   586 to get past 608
frame   632: hop-obstacle  from absolute X   676 to get past 701
frame   673: clear-wall    from absolute X   722 to get past 736
frame   779: clear-wall    from absolute X   891 to get past 913
frame   858: clear-gap     from absolute X  1006 to get past 1136
frame   908: clear-wall    from absolute X  1123 to get past 1137
frame   968: hop-obstacle  from absolute X  1199 to get past 1244
frame   992: hop-obstacle  from absolute X  1259 to get past 1281
frame  1018: clear-gap     from absolute X  1324 to get past 1424
frame  1085: hop-obstacle  from absolute X  1491 to get past 1539
frame  1146: hop-obstacle  from absolute X  1644 to get past 1691
frame  1195: hop-obstacle  from absolute X  1766 to get past 1814
frame  1259: hop-obstacle  from absolute X  1926 to get past 1974
frame  1346: clear-wall    from absolute X  2130 to get past 2144
frame  1383: clear-wall    from absolute X  2154 to get past 2176
frame  1436: clear-wall    from absolute X  2226 to get past 2240
frame  1523: clear-wall    from absolute X  2346 to get past 2368
frame  1572: clear-wall    from absolute X  2467 to get past 2481
frame  1653: clear-wall    from absolute X  2586 to get past 2608
frame  1736: hop-obstacle  from absolute X  2724 to get past 2772
frame  1783: clear-wall    from absolute X  2842 to get past 2864
frame  1837: clear-wall    from absolute X  2906 to get past 2928
frame  1867: clear-wall    from absolute X  2978 to get past 2992
frame  1961: clear-wall    from absolute X  3146 to get past 3168
```

Eleven of those are `clear-wall`, six are `hop-obstacle`, and the pit — the hole the whole
gym is named after — is one long jump on frame 1018 out of eleven. That is what the level
actually asks for, and it is not what the library was built for.

**A wall had to become a pixel before anything could be past it.** The first run stopped at
absolute X 436, at the first pipe, with the hop's precondition satisfied on every frame of
it. `Skill::call` asks the postcondition before the precondition — SMB-021's rule, a claim
the world has already granted is not worth pressing — and `hop-obstacle`'s `past` was
`x + enemy_pixels.unwrap_or(0)`. With a pipe in front and no enemy in front of the pipe,
that pixel was `x` itself, which a body fourteen pixels wide is always past, so the skill
cancelled its own start. Phase 0's rule had simply never been applied to walls: an
affordance names the cell it is about. `wall_within_reach` keeps the bit the reflex reads,
and `wall_ahead` now answers `Wall { x, rows }` — the near edge of the refusing column, one
past the last pixel the body occupies, and how many rows above his feet that column draws.

**The height decides the envelope, so the wall is its own skill.** Four frames of A reach
their top long after they reach the pipe in front of them: the hop is measured over a
koomba and has never been measured over a wall, and the long jump is the program this
project has filmed clearing that pipe. So the hop lost its wall term entirely and
`clear-wall` is the long jump gated on `rows * 8 <= rise`.

**At a wall, the wall is the landing.** The run then stopped at the stairs at absolute X
2162, in front of a two-row step, for six hundred frames, and the reason was the landing
gate. At a wall the long jump's box comes down two pixels past the last column the PPU
draws, so the direct question has no answer; the fallback that saves a long jump over a
hole is a hole whose far lip the picture showed, and an untried staircase is drawn with
artwork the load-bearing set has never held a body on, so the hole reading returns
sixty-four pixels of "nothing to stand on" with no far side at all. A gate that cannot be
answered is a refusal, so the skill refused sixteen pixels of step against a rise of
seventy-five. `clear-wall` does not ask the gate: `wall_ahead` reports a column only where
the load-bearing set has held a body on it or the ledger was refused by it, which is already
a claim about solidity, and a cell that stops a body sideways is the same collision that
holds one up. The residual is the question *behind* the wall — a hole just past the far side
sits in the two pixels the picture does not have — and that is the risk the reflex has
always taken, filmed.

**The height cap was lifted and put back.** Twelve rows is ninety-six pixels, above the long
jump's rise, so the height term became able to fire; and the run stopped fourteen pixels
short of the exit, against the flagpole. The model reads that column correctly — it is
solid, it refused him, it draws twelve rows — and no jump in the library gets over it,
because the level ends by being jumped *into*. A gate on height alone strands the bot there
with nothing else in the script to take the frame, so the cap is back at eight rows, which
is sixty-four pixels and under the rise: a gate that does not gate, and the honest name for
what is missing is a skill for a wall too tall to clear rather than a constant chosen to let
this run pass.

Two things this run made visible and did not fix. `carries_weight_at` answers two ways where
the picture has three states: a cell drawn with artwork the model has no evidence about is
reported as *nothing to stand on*, which is the false sixty-four-pixel hole above, and the
ledger that already knows those cells is not asked. And a height counted by what *draws* is
inflated by background artwork stacked over a real wall, which is the price of lifting that
cap and has no gym attached to it yet.

### The wall gets its gym, and the layer that fails is not the model

Phase 3's exit asks every skill for a gym that fails without it. Two skills had one and
both of those gyms were about the hole, so `clear-wall` — the skill that carries eleven of
the twenty-six behaviours that finish 1-1 — was in the library on the strength of the level
run alone. `pipe-608` is the wall's gym, and the answer it gave is worth more than the
box it was cut to tick.

The start state is the reflex's own run-up to frame 616: absolute X 546, on the ground, at
running speed — the two and a half pixels a frame the pit measurement already records —
sixty-two pixels short of the second pipe in 1-1, with a cold ledger and no refusal anywhere
in the tape. The pipe is the one the reflex is refused at on frame 642, and the objective
is absolute X 700 — past the column, on ground the reflex itself lands on, and short of the
next wall at 736.

```text
pipe-608        reach absolute X 700   player script
  with    clear-wall   Met   after 108 frames, furthest absolute X 698, 2 refusals
  without clear-wall   Stuck after 142 frames, furthest absolute X 595, 18 refusals
```

One field of one gym apart — the same tape, the same objective, the same budget, the same
stall rule, and a library with or without one entry.

**The layer that fails is not the model.** Eighteen refusals is the ablated run being
told, eighteen times, that the column at 608 is solid, by a ledger that has held that cell
since the first of them, and `wall_ahead` names the column at 608 on the frames he stands
in front of it. The model knows the pipe and the library still has nothing to do about it,
which is the two-layer split the whole design rests on, arriving as a verdict rather than
as an argument. Before this gym the evidence for that split was prose.

What the ablated library does instead is the detail that makes the pair readable. Its
transcript is two `hop-obstacle`s, both from absolute X 594, both about pixel 642 — an
enemy forty-eight pixels ahead, which is the only affordance in reach that a remaining
skill knows how to act on. The hop carries sixty-five pixels and rises thirty-nine, and a
four-row pipe wants more than that, so he hops into the column, is refused, and hops again
three hundred frames later. A wall with an enemy in front of it looks, to a library with no
wall skill, like an enemy.

**The `Met` verdict has an ordering inside it that the gym was cut to produce.** The first
behaviour is `clear-wall` on frame 643, from absolute X 594, about pixel 608 — the frame
*after* the game refused him there. The pipe's artwork is not in the load-bearing set and
this gym's ledger starts empty, so nothing in the picture calls that column solid until the
game has said so. The run earns two refusals and spends both: the pipe body, tile `$68`, on
frame 642 with the body flush at 594, and the cell above it, tile `$64`, on frame 650,
recorded by a jump that had already left the ground. That second one is a refusal the
detector committed before this change could not see at all, because it asked for a grounded
pair on both sides of the frame, and it is the same reason 6-2 recorded nothing. Two
refusals bought by one behaviour is still a verdict measured as refusal-then-action rather
than as action alone. `camera-limit-remembering` is the same idea asked of memory and got
`nothing changes` because nothing at the camera limit was ever refused; this is the case
where the lesson exists to be used.

**A skill can start three times at one wall and nothing knows it.** Run long enough past
its own objective, the script on this tape meets the next pipe, at 736, and starts
`clear-wall` from absolute X 722 on frames 740, 783 and 826 — the same pixel, three times,
each attempt ending back where it started — where the level run from the handover cleared
that same wall from that same pixel in one jump on frame 673. The two runs differ in what
they were refused and when, and the mechanism behind the difference has not been measured.
What is worth stating is the hole it exposes: a takeoff that fails its own postcondition
leaves no trace, so the library will spend the same envelope again at the same pixel with
no memory that it just did. That is a different missing thing from the flagpole, and it is
now a command line rather than a hypothesis — `transcript pipe-608` prints the behaviour
list a verdict stops short of, and `transcript --without clear-wall` prints the ablated
one, which is the other half of the pair above and eighty-seven refusals by the time its own
budget runs out.

Choosing that cut frame is why `bot` takes `--trace N`. A gym's start frame is a question
about where the player is on a given frame, and the example printed a decoded line every
600 frames: the reflex is refused at the pipe on 642 and is at absolute X 908 on 886, and
nothing in between was ever printed, so "cut it sixty pixels short of the pipe" was a guess
read off two prints that bracketed the whole approach. Traced at two frames the same run
says it is grounded and running from 608 (absolute X 531) to 636 (593), stuck at 594 from
638, refused on 642 against artwork `$68`, and reads `Wall { x: 608 }` from 644 — so 616, at
X 546, is inside the run, before the refusal, and far enough back that the pipe is met under
the gym's own driver rather than on momentum the tape already spent.

### The level's exit, cut by the player that reaches it

The last gym Phase 3 named and did not have is 1-1's own ending, and it needed a harness
change to exist. `Gym::record` cut every run-up with the base policy, on the reasoning that a
tape cut by the player being tested starts past the frames that player is supposed to solve.
That holds for the four tapes committed before this one, because each sits exactly where the
reflex stops and the frames the driver is asked about are still ahead of it. It does not hold for the level's exit, which is four hundred pixels past the pit
the reflex dies at: a tape the driver cannot run itself describes a start state its own driver
will never be handed. `record` now runs the gym's own driver from the handover to the cut
frame, and Phase 2's rule — upstream of the failure, never at it — still decides where.

`flagpole-3168` is what that buys. The script's run-up to frame 1900: fifty-four button runs,
two refusals, absolute X 3005 on the ground at running speed, one hundred and sixty-three
pixels short of the flagpole, golden hash `v3:0x97f2d3ee150f8352`. The objective is
`Objective::ClearLevel` — the level byte changing, not a pixel — because the pixel is the
thing under dispute: absolute X holds at 3161 for sixty-four frames while the game takes the
grab, so a `ReachX` past the pole could be met by a body that never understood it.

```text
flagpole-3168   finish the level   tape to frame 1900 with a Cold ledger
  script                    Met   after 736 frames, furthest absolute X 3267, 1 refusal
  reflex                    Met   after 757 frames, furthest absolute X 3266, 1 refusal
  script minus clear-wall   Stuck after 666 frames, furthest absolute X 3155, 92 refusals
  script minus clear-gap    Met   after 736 frames, furthest absolute X 3267, 1 refusal
  script minus hop-obstacle Met   after 736 frames, furthest absolute X 3267, 1 refusal
```

**The flagpole is not a wall to the game.** The scripted run starts exactly one behaviour:
`clear-wall` on frame 1971 from absolute X 3154 about pixel 3168. That jump does not get over
the top of a twelve-row column — nothing in the library gets over it — it puts a body into the
column, and the game takes the grab, slides him down, and walks him to the castle. The
skill's own postcondition, "standing past 3168", is satisfied by a jump that cleared nothing.
The reflex wins the same gym twenty-one frames later on a rule that has no notion of a
flagpole at all: jump what the reader reports and what the game has refused. This is the first
gym here the reflex wins, and it is worth sitting with, because the script's advantage in
every other gym in this repository is about holes it can see a landing for and walls it can
name — and at the level's exit neither of those is what the game asks for.

**The exit is load-bearing on `clear-wall` anyway.** Delete that one entry and the same tape,
the same objective, the same model comes to `Stuck` at absolute X 3155 with ninety-two
refusals, short of the pole at 3168 and short of the level; delete either of the other two and
the verdict is unchanged to the frame. `the_level_exit_needs_the_wall_skill_and_not_a_taller_one`
holds the five-way comparison, and it is the only place a gym is attributed to one skill by
deleting the other two as well.

**The height cap and the deleted skill are the same measurement.** `MAX_WALL_ROWS` exists to
cap a reported wall at eight rows, and its comment says raising it to the flagpole's honest
twelve stopped a level run fourteen pixels short of the exit. Measured against this gym, the
twelve-row cap gives `Stuck` after 666 frames at absolute X 3155 with ninety-two refusals —
identical to the ablation above in frame count, position and refusal count — and moves none
of the other five gyms, `plant-wall-304` included. The experiment is worth its own sentence
for what it did not move: the 6-2 column that gym is cut from reports 64 px under this
constant and 64 px under twelve, so that number is eight cells of drawing, while 2-2's column
at pixel 176 moves to 96 px and turns out to have been the ceiling wearing a height. A height
gate that fires on the only wall in a gym *is* a deleted skill.
That is the constant's justification rewritten from a prediction into a number, and it is why
the tall-wall skill is still owed rather than still wanted: `flagpole-3168` fails today in
exactly the shape that skill has to fix, and the shape is now specific — a postcondition that
holds at a column the body reaches and does not clear, on a tape whose objective is the
level's ending.

**A stall window has to be measured against the game's ending, not the bot.** After the slide
he walks to the castle door, and the game then holds absolute X at 3266 for four hundred and
eighty-eight frames before the world and level bytes change. The house stall of 120 frames
calls that `Stuck` two hundred and sixty-eight frames before the level says it is finished, so
this gym carries a stall of 600 and a budget of 900. Nothing in a bot trace shows this — the
run looks like it is winning throughout, because it is — so every `ClearLevel` gym from here
inherits the requirement and cannot discover it from the run it is cut from.

### The frame a button arrives on

`engine-client`'s training test flaked on a shared runner and was reported as possible
non-deterministic keyboard input. It was not. The waits around the keyboard dispatch
counted down from a fixed number of seconds, and a busy runner legitimately runs the same
job on one worker, so runner load decided the verdict before the keyboard was reached; the
waits now watch the job's own progress counters.

The class of bug is real on the NES side, though, and nothing in this tree could see it.
The buttons are a host-side argument to `run_frame` that nothing keeps, and the plan
already records a hold built after the frame's buttons had been assembled landing A one
frame late and moving a run by more than two thousand pixels, with every decoded state
along the way looking reasonable — because the state is the game's and the buttons were
the caller's. The comparison needs the game's own copy, and it keeps one at `$074A`,
written inside the frame from the port the frame was handed, in the order the controller
shifts buttons out: A in the high bit down to Right in the low one, the reverse of
`ControllerButtons`, so the decode reverses it.

Measured against a press pattern that touches all eight buttons and both edges of each,
from the handover: before a frame runs the byte still holds the previous frame's ask, and
after the frame runs it holds that frame's ask exactly, on every frame, held or released,
for sixty frames. So the alignment every envelope above assumes is pinned by a test
(`the_game_takes_the_input_a_frame_was_handed_on_that_frame`) rather than assumed, and the
byte is where a response probe can compare an ask against a take the way DirtSim's does
before it starts timing a jump.

### Starting the game at any level, and what the level byte indexes

`boot_to_level(rom, world, level)` puts him at any of the cartridge's thirty-two levels and
returns once the game has handed him over. `boot_to_level_load` stops one step earlier, at the
mode byte, for anything that wants to watch a load happen. `world` is the byte and `level` is
the number the game's own status bar prints, and why those two are not the same arithmetic in
every world is what this section is about. SMB-026 is the measurement; what is here is the
contract that came out of it.

**`$0760` is a slot, not a number.** In worlds 1, 2, 4 and 7 the game spends slot `$01` on the
pipe travel between the first and second levels, so from the pipe level onward every level
sits one higher than its number. The other four worlds are the plain `level - 1`.

| Worlds | `$00` | `$01` | `$02` | `$03` | `$04` |
| --- | --- | --- | --- | --- | --- |
| 1, 2, 4, 7 | 1st level | travel, delivers 2nd | 2nd level | 3rd level | 4th level |
| 3, 5, 6, 8 | 1st level | 2nd level | 3rd level | 4th level | not a level |

`level_slot` and `level_at_slot` are the only places that table may live, and
`SmbState::level_number()` is what a bot reads. Reading the byte plus one is right in half the
cartridge and one ahead in the other half: a bot that does it believes it is in 1-3 after
clearing 1-1, while the label the game drew during the travel says 1-2. `None` is an honest
answer as well — a byte one past a world's last slot still hands the player over, on an area
that is not one of that world's four.

**The two bytes go in one frame before the Start press**, on `SETUP_PATCH_FRAME`, because the
game reads them while it builds the level and it builds the level as part of starting. Written
on the press frame itself the patch is in place, reads back exactly what was asked for, and
changes nothing: thirty-two asks give eight distinct screens, one per world, each that world's
own first level. A byte can be present, correct and unread.

**A tape can carry those two bytes.** `boot_to_level_slot` is `boot_to_level` with the slot
byte already chosen rather than derived, and it exists so that a gym about a level the setup
script never visits can be recorded and replayed at all. `Tape::start` is that write kept as
the two bytes it puts in RAM — the world byte, where `0` is world 1, and the [`LEVEL`] slot
rather than the number the status bar prints — and `Gym::record` boots through it when a gym
carries one and copies it into the tape it prints. `plant-wall-304` is the first tape outside
world 1: `start: Some((5, 0x01))`, world 6's second slot, the plant column that ends the
script's run there. Bytes and not labels is deliberate. The mapping from a level number to a
slot is a second thing that could move underneath a committed verdict, and a tape that
reproduces a verdict has to name what was written rather than what it meant. A tape that
names a start and is cut at or before `SETUP_PATCH_FRAME` is a contradiction — the frame it
wants to start on is before the frame that decides where it starts — so it fails as
`GymError::Setup` instead of booting into world 1 and reporting a verdict about the wrong
area.

**A load has three lengths**: 167 frames for a plain level in any world, 217 or 231 for the
second level of a world entered by pipe, and 577 or 591 for the travel slot itself. The count
runs from the frame the mode byte is first seen on to the frame the handover is confirmed, and
that starting anchor is the Start press itself — the mode byte flips to play on the frame the
press runs on — and it includes the eight standing-still frames the confirmation wants, which
is why a plain level's 167 is longer than the 157 frames the game really spends loading it.
`LEVEL_LOAD_FRAMES` is the largest of the three because the wait cannot tell them apart from
behind.

The travel slot is worth its own row of numbers, because it is three loads wearing one: 157
frames of load, then player state `$07` for 334 frames while he walks over the *previous*
level's screen under his own power with his buttons ignored, the level byte reaching `$02` two
frames before that walk ends, 24 more frames of load, 17 more of `$07`, and his own area from
there. `$07` is not a travel-only marker — a plain load passes through it for a single frame
and a pipe world's second level for seventeen — so what a caller reads is `is_in_control` plus
a budget, not a state.

**The game's own label is readable, and stale.** In this font the tile id *is* the digit —
`$00` through `$09`, `$28` the dash — and the WORLD field is nametable row two, columns 19–21.
The field is drawn when a level changes and not again after, so a patched start shows the label
from before it and a respawn shows the one before that; it is printed beside the byte, never
instead of it. The backdrop palette is a worse oracle still — 5-2 and 8-2 draw the overworld's
blue and world 7's castle-shaped slot draws the water's — so the geometry columns below are
what the area type is read from.

The instrument is `crates/smb-ai/examples/levels.rs`, and this is its answer to "what is each
level", asked without believing a byte: `slot` is what was written, `byte` and `level` are what
the machine reads afterwards, `hud` is the label the game drew, `idle` is the frames from the
mode byte to the handover, and the last five columns are the playfield: absolute X, `ceil`
for how many columns of the top row draw something, `wall` for how many rows of the left
column do, `full` for the first row that is nearly all one thing — the ground surface at 25
in an overworld and an underground, the brick ceiling at 3 in a castle — and a hash over both
nametables below the status bar.

```text
asked  slot  byte  level  hud    idle    x     ceil wall full   screen
1.1  0x00  0x00      1  1-1      167     40     0     0   25  4566b2ebe3dc7581
1.2  0x02  0x02      2  1-1      217     40    20    16   25  4f646cbfb458b1af
1.3  0x03  0x03      3  1-1      167     40     0     2   25  2d0f3f0ec60f2a44
1.4  0x04  0x04      4  1-1      167     40    20    12    3  f55318e041692be3
2.1  0x00  0x00      1  2-1      167     40     0    14   25  0960ba7e47b81aac
2.2  0x02  0x02      2  2-1      231     40    20    16    3  c94c3146e1162052
2.3  0x03  0x03      3  2-1      167     40     0     2   25  e0a2482c117b491b
2.4  0x04  0x04      4  2-1      167     40    20    12    3  3d2bf5b11661a2c6
3.1  0x00  0x00      1  3-1      167     40     0    14   25  0b7c79b15e9ad9b7
3.2  0x01  0x01      2  3-1      167     40     0     2   25  dc9018b2d128c928
3.3  0x02  0x02      3  3-1      167     40     0     2   25  752437e722053abc
3.4  0x03  0x03      4  3-1      167     40    20    12    3  6ebb6350fc90a495
4.1  0x00  0x00      1  4-1      167     40     0    14   25  f7f96b7e2c100fd6
4.2  0x02  0x02      2  4-1      217     40    20    16    0  a7fbe3776b4b2643
4.3  0x03  0x03      3  4-1      167     40     0     2   25  0cda9f384e5946f6
4.4  0x04  0x04      4  4-1      167     40    20    12    3  0f6ab4eb6fe7699d
5.1  0x00  0x00      1  5-1      167     40     0     2   25  f11a2930754e73a2
5.2  0x01  0x01      2  5-1      167     40     0     2   25  baf4046fd75e5e43
5.3  0x02  0x02      3  5-1      167     40     0     2   25  41c78045d1dd81d8
5.4  0x03  0x03      4  5-1      167     40    20    12    3  296ce7b8d7eceed7
6.1  0x00  0x00      1  6-1      167     40     0    14   25  68fe8ca685253554
6.2  0x01  0x01      2  6-1      167     40     0     2   25  2a68a582e3c2fc08
6.3  0x02  0x02      3  6-1      167     40     0     2   25  035a161c28608c5a
6.4  0x03  0x03      4  6-1      167     40    20    12    3  1e903e74281e711e
7.1  0x00  0x00      1  7-1      167     40     0    14   25  605d18478eb9f8b7
7.2  0x02  0x02      2  7-1      231     40    20    16    3  84e4386ec4d3b6b5
7.3  0x03  0x03      3  7-1      167     40     0     2   25  f2085b784e2cac30
7.4  0x04  0x04      4  7-1      167     40    20    12    3  3ee165ad148f8944
8.1  0x00  0x00      1  8-1      167     40     0    14   25  3b5a234a1e19d229
8.2  0x01  0x01      2  8-1      167     40     0     2   25  4f241566a66b5db4
8.3  0x02  0x02      3  8-1      167     40     0     2   25  0d702d6fdf524c11
8.4  0x03  0x03      4  8-1      167     40    20     8    3  a5a4c2ae6b9ab086
```

A `full` row of 3 instead of 25 is the castle — its top row is brick across the whole width —
and it lands on `$04` in the four pipe worlds and `$03` in the other four, once per world and
nowhere else, which is what the mapping predicts. World 1's `$03` has no ceiling at all, so it
is not that world's last level, and that one row is what rules out the naive reading. 4-2's
`full` of 0 is the water: no row across the screen is nearly full. 8-4's eight-row wall is the
one castle whose left wall differs from the rest. Thirty-two asks, thirty-two distinct screens,
every one of them handing the player over in control and standing on ground.

**The patched start and the played start are the same run, pixel for pixel.** `drive 1 1 3000
--shots DIR` reaches 1-1 by writing two bytes into RAM; `bot 2620 --script --shots
data/shots/script-1-1` reaches it by pressing through the title screen and touching no byte at
all. Both write one PNG per frame, named for the frame id and the absolute X, and 2,327 of those
names are shared — every one of them the same file, 0 differing pixels out of 2,327, over
thirty-eight seconds of HUD, enemies, deaths and a level ending. The patched series carries one
frame the played one does not, `frame-002614-x03266.png`, and the played series names its clear
still `event-clear-f2614-x3266.png`: two writers that share no code, agreeing on the frame the
level ends on as well as on every pixel up to it. The patch's justification used to be that the
game names the level back and that the arrived screen hashes to the table's own hash for it.
This is the same claim at the highest resolution available here, and it is what licenses reading
every row of the inventory below as being about the level it names.

`drive-all` runs *both* of the tree's players from all thirty-two, and that pair is Phase 4's
inventory. `skills::SCRIPTED` and `gym::REFLEX` are two `Driver` values over the same `BaseBot`,
the same empty ledger, the same start, the same frame budget and the same stuck detector, so the
policy above the ledger is the only thing that varies — which is the whole reason the run is
doubled. A single driver cannot tell a hard level from a weak player; two of them can, and the
verdict line under each pair is the answer. A run ends where its level ends, at the first level
byte change, because a row is a claim about the level it started in. The excerpt is two of the
thirty-two pairs and the whole summary; the sixty-four rows between them are the inventory.

```text
start driver outcome      frames furthest deaths refusals      feet  ctrl  model at the stop
1.1   script left it        2327     3266      0       22  (21, 25)  game  nothing standing against him
1.1   reflex stuck          2046     1553      2        8  (29, 10)  his   nothing standing against him
  only script left it: the other stops at absolute X 1553, so the level asks for something one player has and the other has not
2.2   script stuck           736      162      0       77   (9, 23)  his   a wall 64 px tall starting at pixel 176
2.2   reflex stuck           702      162      0       75  (13, 22)  his   a wall 64 px tall starting at pixel 176
  both stop at absolute X 162 within 0 px: the stop is the level's, not the driver's, so it is a gym candidate

== what two players on the same thirty-two starts say ==
both players clear the level:
one player clears it and the other stops: 1.1
both stop on the same pixel — the level's own obstacles: 1.4 2.2 3.2 4.1 4.4 5.2 6.4 7.2 8.4
one player gets a screen further — a stop about the player: 1.2 1.3 2.1 2.3 2.4 3.1 3.3 3.4 4.2 4.3 5.1 5.3 5.4 6.1 6.2 6.3 7.1 7.3 7.4 8.1 8.2 8.3
…of those, stops the model has no name for: 1.4 3.2 4.1 4.4 5.2 6.4 8.4
the same level under 2 names: 1.3 5.3 — both players' runs match to the frame
the same level under 2 names: 1.4 6.4 — both players' runs match to the frame
the same level under 2 names: 2.2 7.2 — both players' runs match to the frame
the same level under 2 names: 2.4 5.4 — both players' runs match to the frame
```

**One of the thirty-two reaches its ending, and it is the same one as ever**: 1-1 at
absolute X 3266 with no death under the script, and the reflex dying twice and stopping at
1553. **The two players disagree on twenty-three starts, and the reflex is further ahead on
fourteen of them**; the script's own advantage is 1-2, 2-3, 3-4, 7-1, 7-3, 7-4, 8-1 and
8-3, on top of the one level it leaves. The scripted player is not the stronger player on
the cartridge, it is the stronger player on nine starts, and that is only sayable now that
something else has been run there. **Nine stops belong to the level**: 1-4, 2-2, 3-2, 4-1,
4-4, 5-2, 6-4, 7-2 and 8-4 stop both players within one cell of each other, which is the
only test here that says *level* rather than *player*. Two of the nine name the same wall —
64 px tall at pixel 176, `MAX_WALL_ROWS` times eight, the height gate's own ceiling, in two
different worlds — and seven name nothing at all, which is Phase 6's silence arriving
early. That ceiling is measured and not inferred: raise `MAX_WALL_ROWS` to twelve and both
of those rows report 96 px instead of 64, which is the one place in the inventory where the
number the bot gates on is the count's limit rather than the level's. **6-2 is the
disagreement the ledger now has a name for, and it is not that ceiling.** The reader reports
the first pipe column there as a wall 64 px tall at pixel 304 — eight cells of drawing, and
the cap experiment leaves the number where it is — so the height gate is not what stops the
script, because sixty-four is inside the long jump's measured seventy-five pixels of rise
and the gate lets the attempt start. What stops it is the approach: `hop-obstacle` fires at
the plant 48 pixels ahead, its four-frame hold stops him at absolute X 290, `clear-wall`
wins the next choice from a pixel with no run-up left, is refused, and lands back on that
pixel. Eight of those — five against the column at 304 and three against the next one at
448, each burst at forty-three-frame intervals — until one attempt lands him on top of the
column and he walks off its far side, which is what standing on a pipe looks like to a
reader whose load-bearing set has no pipe top in it, and then a death at 576. The reflex is
refused forty-two times by the same columns
and reaches 883, because its jump hold is twenty frames where the hop's is four and it is
airborne before the wall is flush. `plant-wall-304` is that first column cut at absolute X
242 with both players asked of the same bits: `Stuck` after 141 frames and 15 refusals for
the script, `Met` after 68 and one refusal for the reflex — a wall the library is tall
enough for, lost to the run-up the library spent getting to it. **Four pairs of starts run
identically under both players** — 1-3 with 5-3, 1-4 with 6-4, 2-2 with 7-2, 2-4 with 5-4, same
frames, same pixel, same deaths — so the cartridge ships fewer than thirty-two levels
under thirty-two names, and since the ledger is keyed on pattern hash and those pairs draw
their shared geometry with different artwork, a bot taught 1-3 arrives at 5-3 having been
taught nothing. 2-3 and 7-3 draw an identical first screen and still run differently, so
the twins are found from runs and not from pictures.

## Rollout and search

`checkpoint` plus `restore` is enough for planning: DirtSim's DFS stores one
savestate and one memory snapshot per node and explores 11 macro-actions
(neutral, right, right-run, right-jump, right-jump-run, left-run,
left-jump-run, duck, duck-jump, duck-right-jump-run, duck-left-jump-run) one
frame at a time, pruning on death, on 120 frames without progress, on stuck
velocity, on falling below the screen, and on transposition. Its transposition
key is 63 specific RAM bytes covering hidden movement, input, collision, scroll
and timer state (`kClosedLossTranspositionRamByteAddrs` in `SmbDfsSearch.h`),
with a 5,000-node budget.

Reward and termination transfer unchanged: 0.5 per unit of new absolute X, 1000
for a level clear, and a no-progress timeout at 1800 gameplay frames. The
below-screen terminal test does not: it compares the player's Y byte against 224, and
that byte is the low byte of a position that keeps counting, so read this way it wraps
every 256 pixels of a fall — above 224 for about eight frames in every sixty-four
while he is falling, below it for the rest. DirtSim can afford that because a reward
model averages over thousands of frames; a bot deciding whether to jump cannot. The
test that transfers is the game's own, the player state leaving `$08`, and it is slow:
measured on the pit at 1379 he leaves the ground on frame 1100 and the state only goes
on 1379.

## Proposed shape here

Nothing below is implemented; it is the shape the investigation points at.

1. A cartridge-profile module that owns a versioned RAM address table plus
   decoders, so SMB, Falling and later cartridges each declare a map instead of
   hardcoding addresses in a scenario. Key it on canonical cartridge identity,
   not filename, and fail closed on an unknown ROM.
2. A versioned semantic observation for `scenario-nes` that carries an optional
   profile payload alongside the existing frame id, state hash and controllers,
   following `FallingObservation`.
3. A tile-frame reader over `nametables`, `ppu_memory_peek`, `registers` and
   `mirroring`, with the pattern-hash vocabulary. Keep it borrow-only and
   allocation-free in the core; the vocabulary can live host-side.
4. A rollout harness on `checkpoint`/`restore`, and only then the 11-action DFS
   with DirtSim's pruning rules.

`engine-nes` should keep no game knowledge: profiles belong above the core, in
`scenario-nes` or a sibling crate, so the core stays cartridge-agnostic and
free of a filesystem, clock, thread and renderer.

## Gaps

- DirtSim's map is pinned to one ROM revision. The optional test rejects a
  different checksum outright, but a shipped profile has to do the same: fail
  closed on an unrecognised cartridge rather than silently misread it.
- The tile vocabulary is learned, so a port needs either a committed frozen
  vocabulary or a documented rebuild step.
- DirtSim's savestate format is `smolnes`-specific; only the design transfers,
  not the bytes.
- No SMB2, SMB3, Contra or Legend of Kage map exists yet. Their headers put
  them inside the documented mapper envelope, but each needs its own map.
- The DFS in DirtSim runs against a runtime with wall-clock pacing; the Rust
  host must drive it from the synchronous frame call instead.
- Solidity is learned, not known. The ledger holds only artwork this bot has
  already been refused by, so a level is passable on the second walk and not the
  first, and nothing measured yet says whether a refusal recorded against one body
  row holds at a height it was never refused at.
- The model's memory is not part of a checkpoint. `Resistance` lives host-side, so
  `restore` returns the machine to a frame and leaves the model wherever the caller
  left it, so the A/B above has to carry the ledger across the restore itself. The gym
  harness names the consequence instead of hiding it: `LedgerSeed` is part of a gym, so
  the cold and warm starts are two gyms with one tape, and a verdict cannot quietly
  depend on which. What is still missing is a way to *persist* a ledger, so a lesson
  learned in one run is available to the next one at all.
- The stop that ended every run of 1-1 is named and crossed, and so is the rest of the
  level: `bot --script` clears world 1-1 at absolute X 3266 with no death. See *The first
  skill* and *The library plays a whole level* above. What is still missing is everything
  past the exit. The flagpole is a twelve-row solid column and the run only gets past it
  because `MAX_WALL_ROWS` caps the reported height at eight, so the level's own ending is the
  one wall in 1-1 with no skill that owns it — and `flagpole-3168` now measures the debt: the
  cap raised to the pole's honest twelve and `clear-wall` deleted from the library give the
  same verdict to the frame, so the level ends today on a number chosen so that nothing asks
  the question. The base policy asks `gap_within_reach` only while it is grounded and
  only over five columns, and the script that crosses asks `gap_ahead` over twenty-four,
  which at running speed is past the picture's own edge — so the crossing works because the
  hole reports its far lip, and it would stop working on a hole whose far lip is not the
  ground return. Three skills is also three skills: one level, one cartridge, one start, and
  the only tapes ever run under them are the level's own beginning and six gyms cut out of
  runs that stopped — three of them about the hole, and the other three, the pipe, the
  level's exit and the plant column in 6-2, the only ones that ask the wall skill anything.
- A takeoff that fails its own postcondition leaves no trace, and the library will spend the
  same envelope at the same pixel again with nothing new known. On the pipe tape the script
  starts `clear-wall` from absolute X 722 about pixel 736 on frames 740, 783 and 826 — three
  takeoffs, one column — where the level run cleared that same wall from that same pixel in
  one jump. What differs between the two runs has not been measured; what is missing has:
  no layer records that a behaviour was just started there and did not land.

How far that leaves this from a player, measured rather than judged: the reflex has
never finished a level and still stops at the pit, and the script has finished exactly
one — 1-1, which is the only level whose tiles this reader can read and the only place it
can start. It has three macro-actions written inline in an
example where DirtSim has eleven, no reward or termination wiring, no item or block
state, five enemy slots because that is all the game keeps live, a sideways
solidity model that knows only what one bot has already walked into, three skills that
between them clear one level and cannot name the thing that ends it, and six golden hashes —
the frames six tapes start on — so a core change that quietly alters SMB is caught where a
gym happens to stand and is still invisible everywhere else.

## What is in the tree now

One model in four layers, one policy, a three-entry skill library that clears 1-1, one
harness, six gyms, ten drivers, thirty-four tests — twenty-three of them skipped when the
cartridge is absent, the eleven that hold the skill rules needing no game at all — and one
film cutter.

| File | Layer | Role |
| --- | --- | --- |
| `crates/smb-model/src/profile.rs` | L1 | the RAM map, its decoders, the setup script, the handover rule |
| `crates/smb-model/src/frame.rs` | L2 | PPU snapshot, tile grid, pattern hashes, the feet offset, background reconstruction |
| `crates/smb-model/src/resistance.rs` | L3 | the refusal signature and the ledger keyed on artwork |
| `crates/smb-model/src/affordances.rs` | L4 | gap, wall, enemy in reach: what a body can act on right now, each naming the cell it is about |
| `crates/smb-model/src/cartridge.rs` | - | the pinned image, the loader, `SPACEWARS_SMB_ROM` |
| `crates/smb-ai/src/policy.rs` | L5 | the base policy, the `Bot` seam every player shares, and the frame-exact envelope it acts through |
| `crates/smb-ai/src/skills.rs` | L5.5 | two measured envelopes, the three skills that spend them, the script, and the instrument that measures an envelope from a tape |
| `crates/smb-ai/src/gym.rs` | instrument | tape, replay, objective, frame budget, verdict |
| `crates/smb-ai/src/gyms.rs` | instrument | the gyms this project has actually needed |
| `crates/smb-model/examples/{probe,tiles,rollout}.rs` | | read-side exploration: decoded state, the tile grid and the pixel comparison, branch and rewind |
| `crates/smb-ai/examples/{bot,solidity,gym,transcript,film,limit}.rs` | | the bot, its overlay films and the transcript of behaviours it started, the A/B that tells memory from reaction, the tape recorder, the behaviours behind a gym's verdict and the library entries that produce them, the films a gym's own driver was measured under, the instrument that asks a stop what it actually is |
| `crates/smb-ai/examples/levels.rs` | | the whole cartridge: the thirty-two-start table, a level's playfield as a map, a load watched frame by frame from a level or from a raw byte, either player loose on any start with the run printed one frame per line, and both of them on all thirty-two |
| `crates/smb-model/tests/reader.rs` | | the optional cartridge test: the RAM map, the tile frame, a frame-exact restore, and the frame a button arrives on |
| `crates/smb-ai/tests/{gyms,levels,solidity,skills}.rs` | | a gym's verdict is what its driver does, the two players disagreeing about a level start in both directions, the A/B that tells memory from reaction, and the skill rules and their ablation |
| `tools/nes-reference/film-smb-run.py` | | cuts films, clips and a contact sheet from a `--shots` directory, and checks the overlay survived the encoder |

The reader left `engine-nes/tests/support` when its second consumer arrived, which is
the gate SMB-006 set and SMB-016 named a destination for: `smb-model` depends on
`engine-nes` and nothing else, `smb-ai` holds the players, and the core still holds no
game knowledge. The address map moved to the host side of that boundary rather than
into the emulator, so a reader bug is still nobody's excuse for a core change.

Every driver takes an optional cartridge path and otherwise reads `cartridge::path`,
which is `SPACEWARS_SMB_ROM` or `data/roms/smb.nes`, so one command line works on a
machine whose image lives elsewhere.

```sh
cargo run --release --locked -p smb-model --example probe -- data/roms/smb.nes 700
cargo run --release --locked -p smb-model --example tiles -- data/roms/smb.nes 900 30
cargo run --release --locked -p smb-model --example rollout
cargo run --release --locked -p smb-ai --example bot -- data/roms/smb.nes 9000

# The library, on the whole level: 1-1 cleared, no death, and a line for every behaviour
# it started. Drop `--script` to watch the reflex spend two lives at the pit.
cargo run --release --locked -p smb-ai --example bot -- data/roms/smb.nes 2620 --script

# Where a gym's cut frame comes from: a decoded line every N frames instead of every 600.
cargo run --release --locked -p smb-ai --example bot -- data/roms/smb.nes 900 --trace 2
cargo run --release --locked -p smb-ai --example solidity
cargo run --release --locked -p smb-ai --example gym
cargo run --release --locked -p smb-ai --example bot -- data/roms/smb.nes 2400 --shots /tmp/smb-film --overlay --every 2
python3 tools/nes-reference/film-smb-run.py /tmp/smb-film --clip 600-700 --clip 830-920

# The behaviours a gym's verdict stops short of, and the same run with one skill deleted.
cargo run --release --locked -p smb-ai --example transcript -- pipe-608
cargo run --release --locked -p smb-ai --example transcript -- pipe-608 --without clear-wall

# A gym film: the tape, the ledger and the player all come off the gym.
cargo run --release --locked -p smb-ai --example film -- pit-1379 data/shots/pit-cross 200
python3 tools/nes-reference/film-smb-run.py data/shots/pit-cross

# Any level in the cartridge: the table, one level's map, one load watched, one start
# driven, and both players loose on all thirty-two. `W` is 1-8 and `L` is 1-4 as the game
# numbers them, not as the byte indexes them. `slot` is the one mode that writes a byte
# instead of naming a level, which is how the pipe travel gets watched and filmed, and
# `--player script|reflex` picks which of the two plays a single start.
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes map 1 4
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes watch 1 2 900
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes natural
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes drive-all
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes drive 8 3 2400 --shots data/shots/level-8-3
# The same run one frame per line: position, speed, grounded, control, what the reader
# reports and what the player decided. This is how a stop gets read instead of inferred.
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes drive 6 2 1200 --player script --trace
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes drive 6 2 1200 --shots data/shots/6-2-script --player script
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes drive 6 2 1200 --shots data/shots/6-2-reflex --player reflex
cargo run --release --locked -p smb-ai --example levels -- data/roms/smb.nes slot 1 01 700 --shots data/shots/travel-1
python3 tools/nes-reference/film-smb-run.py data/shots/level-8-3 --every 1
python3 tools/nes-reference/film-smb-run.py data/shots/travel-1 --every 1
python3 tools/nes-reference/film-smb-run.py data/shots/6-2-script --every 1
python3 tools/nes-reference/film-smb-run.py data/shots/6-2-reflex --every 1
# The two players on one start, side by side and time-aligned on the same emulator frame.
ffmpeg -y -i data/shots/6-2-script/6-2-script.mp4 -i data/shots/6-2-reflex/6-2-reflex.mp4 \
  -filter_complex "[0:v][1:v]hstack=inputs=2" data/shots/6-2-compare/6-2-script-left-reflex-right.mp4
```

The `film` line writes a PNG per frame, with the reader's green, orange, red and blue dots
in a second copy of each, which is the fastest way to see what the model believes and, on
the next pass, what it has learned. The two `levels --shots` lines write the plain frames
only, so the cutter gets one series and no overlay; the travel film is worth watching
precisely because nothing is drawn over it — you can see the game walking him rightward
over a level he is not in yet, at absolute X 40 to 160, before the load takes the screen
away and puts him back at 40 in his own.

Observed on x86-64 Linux with a release build:

- `probe` reaches gameplay on frame 120, reads world `0-0` (1-1), walks absolute X
  from 47 to 295, sees a goomba (type 6) 110 pixels ahead and then 10, and drops lives
  from 2 to 1 when Mario dies in player state `0x0B`.
- `tiles` decodes a grounded frame's 32x28 grid, puts Mario on ground tile `$b5` at
  tile `(5,25)` with hill decoration in the two rows his body occupies, confirms the
  mid-air tile is `$24` with an all-zero pattern, and reconstructs the background from
  the nametables and CHR with `0` wrong pixels out of 51,136 compared. Passing a third
  argument samples later frames, where the camera is moving and the residual described
  above appears.
- `rollout` branches four 120-frame rollouts from one checkpoint and gets four
  different outcomes (idle, run into a pit, run and jump, jump in place) and reproduces
  the root state hash exactly on every `restore`. Its frames-per-second figure swings
  with machine load, so treat it as an order of magnitude: several hundred frames per
  second including checkpoint restore, comfortably above the 60 fps the kiosk needs.
  Its lookahead section reports the slot-stability numbers quoted above.
- `bot` runs the reader-only policy plus the refusal ledger to absolute X 1553, losing
  three lives at the same place — the pit the camera limit turned out to be — before the
  game ends the run. It waits for the level load to hand the player over (trap 8) before
  it starts, and says which of "stuck", "out of lives" or "cleared the level" ended the
  run. The one
  refusal it learns, at absolute X 594, teaches two cells and carries it past the pipe
  that stopped the same policy with an empty ledger at 899.
- `solidity` runs the four passes behind the A/B table above and prints, for both
  tails, the first cell the ledger calls a wall that the tile ids do not, the refusals
  the tail itself collected and the furthest absolute X, all from one checkpoint state
  captured 60 frames before the taught artwork appears a second time. The lesson sits
  behind the start state and the obstacle in front of it, which is the only arrangement
  where remembering and reacting are distinguishable by anything but the prose.
- `gym` replays every committed gym and says whether each still measures the verdict it
  carries; all six do. `gym --record FRAME [NAME]` cuts a tape instead: it runs the gym's
  own driver through the run-up, replays what it recorded before printing it, and prints the
  run-up, the start bytes, the golden hash and the verdict measured from it — which is how
  `pit-1379`, `flagpole-3168` and `plant-wall-304` got their bytes, and why none of the three
  has hand-typed ones. The recorder prints `start:` beside `runs:` because a tape cut in a
  level the setup script never visits is only reproducible with the two bytes it wrote.
- `levels` boots all thirty-two starts on eight threads and prints the table above, then,
  with `drive-all`, runs both players from every one of them — sixty-four runs in sixteen
  seconds of wall clock, because a run now ends at its own level byte: one start is left by
  anyone, nine stop both players on the same pixel, twenty-three stop the two players in
  different places and fourteen of those favour the reflex, and four pairs of starts match
  frame for frame under both players. `drive W L FRAMES --player P --trace` prints one of
  those runs a frame per line — position, speed, grounded, control, what the reader reports
  and what the player decided — which is how the 6-2 account earlier in this document is a
  reading of a run rather than an inference from a verdict. `slot 1 01 700 --shots DIR` is
  the same watch from a raw byte, and it is how the travel gets filmed: the screen it
  arrives on is `4f646cbfb458b1af`, the table's own hash for 1-2, which is the travel slot
  and a patched start agreeing without anyone asserting it. `natural` is the control the
  whole table leans on — clearing 1-1 without writing a byte and reading back what the game
  says it landed on, byte `$02`, status bar "1-2". `--shots` writes the PNG series the
  cutter expects, so a start anywhere in the cartridge is filmable.
- `limit [FRAME]` is the instrument behind the pit section: it cuts a run-up to any frame
  with the base policy, checkpoints the end of it, and asks the same question of the game
  and of the picture — what the policy knew as it approached, what the state bytes say the
  game was doing, what the support row under and ahead of him is drawn from, how far a
  jump carries from each frame it could start on, and what the game allowed afterwards.
  Sixty-six jumps per run, each rewound to the end of the tape rather than replayed from
  power-on, and each watched for 420 frames because the game takes its time about a pit.

## Where the films live

Run artifacts stay out of the repository with the rest of `data/`, so the films
this document reports are on the owner's workstation under `data/shots` and this
paragraph is their record. A `--shots` directory holds the raw series and the film
tool writes its output into that directory unless `--out` says otherwise:

| Name | What it is |
| --- | --- |
| `data/shots/<run>/` | the raw PNG series, `frame-*` plain and `model-*` with the dots on |
| `data/shots/<run>/event-<what>-f<frame>-x<x>.png` | a still at a moment: `jump`, `refusal`, `death`, `stuck`, `game-over`, `clear` |
| `<run>.mp4` | the plain series at 60/N, so it still runs in real time |
| `<run>-model.mp4` | the same frames at 60 with the model's own beliefs dotted on |
| `<run>-clip-<lo>-<hi>.mp4` | an overlay clip over emulator frames `lo` to `hi` |
| `<run>-events.png` | every event still in the order they happened, legend printed |

Ten runs and one composite are worth keeping. `reader-only` is the honest one, 2,283 frames, stopped
against the pipe at absolute X 899. `reflex` is the reaction rule that was deleted,
8,168 frames, reaching 2601 and dying twice; it stays because it is the evidence for
SMB-008, not a current build. `ledger` is the current one, 2,380 frames, absolute X
1553 and two deaths, and it is the only series whose overlay includes orange.
`pit-cross` is 200 frames of the `pit-1379` tape under the script, from the tape's own
frame 1040, and it is the first film here of a bot crossing the pit on purpose: four
`event-jump` stills, the hop at 1266, the crossing jump at 1331, the hop at the koopas at
1494, and one more past the objective at 1646. `script-1-1` is the first run of anything
here that finishes a level: 2,327 frames, the script from the handover to absolute X 3266,
no death, twenty-six `event-jump` stills — one per behaviour it started, which is the
transcript printed as pictures — two refusal stills, the first pipe at 434 and the first
stair at 2130, and one `event-clear` still on frame 2614. That last one is the only frame
any run here has ever reached, and the PPU has already moved on to the next area by it, so
what it holds is the ending rather than the jump that caused it; the jump is the
`event-jump` still at frame 1961 and the film is what joins them. It is cut by
`bot 2620 --script --shots data/shots/script-1-1` with no overlay, so the series is the
plain one and the film is `script-1-1.mp4`.
`pit-cross` and `pipe-608` have no `model-*` series and no overlay clip because they are gym
films rather than `bot` films: `film` replays a gym's tape and hands the machine to the driver
that gym names, so the run on screen is the run the verdict was measured from, and it writes
one PNG a frame instead of two. Re-cutting either reproduces every frame and every still byte
for byte.

The clips that carry the argument are `reader-only-clip-1600-1802`, the pipe, where
the row under his feet is green and the three rows ahead are red, which is trap 7
without any prose; `ledger-clip-600-700`, the refusal at the first pipe and the jump
one frame later; `ledger-clip-830-920`, the second pipe jumped without ever
being stopped by it, with no refusal event anywhere near it, which is the memory
rather than the collision;
and `ledger-clip-1330-1400`, the death at the camera limit that the model says nothing
about at all. Three clips from the new run carry the current argument:
`script-1-1-clip-1340-1470` is the stairs at 2130 to 2240, three `clear-wall` jumps up a
step each, which is the frame range the landing gate used to stand in front of for six
hundred frames; `script-1-1-clip-1940-2300` is the flagpole — the jump into the twelve-row
column at 3168 and the slide down a column no jump in the library gets over; and
`script-1-1-clip-2300-2613` is the walk to the castle and the level ending, which is what
the run's last three hundred frames turn out to be. `script-1-1-events.png` is all
twenty-nine stills in the order they happened, the transcript as one picture.

The sixth is `pipe-608`: 200 frames of the wall gym under its own driver, from the tape's
own frame 616, cut by `film pipe-608 data/shots/pipe-608 200`. It is the film of a refusal
being spent — the `event-jump` still at frame 643 is one frame after the game stopped him at
the pipe, which is the ordering the gym was cut to produce, and the two stills at absolute X
722, frames 740 and 783, are two of the three takeoffs at the next pipe, the same pixel asked
twice. `pipe-608-events.png` is those four in order.

The seventh is `flagpole-3168`: 760 frames from the tape's own frame 1900, cut by
`film flagpole-3168 data/shots/flagpole-3168 760`, and the first film here of a run that
finishes a level from the middle of one. Its sheet is two stills, and they are the gym's whole
finding in one picture: `event-jump-f1971-x03154` is the only behaviour the run starts, a jump
taken *into* the flagpole, and `event-clear-f2634-x03266` is the frame the objective held, six
hundred and sixty-three frames later with the castle already on screen. Between them absolute
X holds at 3161 for sixty-four frames while the game takes the grab and slides him down. That
second still is new to `film` and is the one `bot --shots` has written since the level
run: the frame comes from the same `Objective::held` the verdict is scored with, so a film of
a gym shows the frame its verdict stops on. `flagpole-3168.mp4` is 25.3 s — the tool thins to
every second frame by default and a gym film writes every frame, so it plays at half speed —
and `flagpole-3168-events.png` is those two in order. Re-cutting either reproduces every
frame and every still byte for byte.

The last four are the two players on one start. `1-1-script` is `script-1-1` reached the other
way — `drive 1 1 3000 --shots data/shots/1-1-script` — and it exists because the comparison
above needs both series on disk; re-cutting it and diffing the shared frame names is the check.
`6-2-script` and `6-2-reflex` are the same 6-2 start under the two players, 724 and 1,153 frames,
written by the two `drive 6 2 1200 --player` lines above and cut by `film-smb-run.py`.
`6-2-compare/6-2-script-left-reflex-right.mp4` is those two films side by side, script on the
left and reflex on the right, both starting on the same emulator frame, from one `ffmpeg
-filter_complex hstack`. It is the most legible thing this project has filmed: the left half
stops dead at absolute X 291 and stands there for the remaining ten seconds while the right half
walks past the same pixel, reaches 883, loses the life there and is back at absolute X 0 when its
run is cut off. `6-2-reflex-events.png` is its one still, that death.

`sheet-moments.png` and the `hero-*.png` stills are one-off comparisons
of the screen against the model at chosen frames. The two older runs were cut with
`--out data/shots`, which is why their films sit beside their series; `ledger`'s sit
inside it, where the tool puts them by default.

Any of them is re-cut from the cartridge in two commands, the run that writes the
series and the cut:

```sh
cargo run --release --locked -p smb-ai --example bot \
  -- data/roms/smb.nes 9000 --shots data/shots/ledger --overlay --every 2
python3 tools/nes-reference/film-smb-run.py data/shots/ledger \
  --clip 600-700 --clip 830-920 --clip 1330-1400
```

Re-cutting `ledger` from its existing series reproduces all six of its files, both
films, the contact sheet and the three clips, byte for byte, and the cutter's own
check reports every dot count going in and coming out the same. That is the point: a
film that cannot be re-cut from a commit is a screenshot with a claim attached to it.

## Optional cartridge tests

Every other test in the workspace drives a `test_rom` builder, so semantic reads
of a shipped game had no regression net: the mapper tests use generated ROMs,
the reference hashes are opaque rather than meaningful, and the only semantic
game-state assertion anywhere was
`scenarios/falling/src/lib.rs::observation_has_a_stable_decodable_v1_shape`.

Five test files close that, one per layer that can be wrong:
`crates/smb-model/tests/reader.rs` for the reading,
`crates/smb-ai/tests/solidity.rs` for what the model concludes from it,
`crates/smb-ai/tests/gyms.rs` for the harness that replays it,
`crates/smb-ai/tests/skills.rs` for the envelopes the library spends and the rules
that spend them, and `crates/smb-ai/tests/levels.rs` for two players on one start.
All five are optional by necessity rather than preference: `.gitignore` keeps all of `data/`
out of the repository, so no cartridge is checked in and CI has nothing to boot.
Each skips with a one-line note when the image is absent, and passes in 0.00s:

```sh
cargo test --locked -p smb-model -p smb-ai            # skips without a ROM
SPACEWARS_SMB_ROM=/path/to/smb.nes \
  cargo test --locked -p smb-model -p smb-ai          # runs with one
```

The default is `data/roms/smb.nes` resolved from `CARGO_MANIFEST_DIR`, so it
works from any working directory. With the cartridge present the thirty-four tests
take about ten minutes in a debug build, nearly all of it emulator frames: the
reader six 25 s, the solidity pair 80 s, the gym sweeps 124 s — every committed
tape replayed and every committed verdict re-measured, which is what a gym's claim
costs — the two-player level test 135 s, and the skills suite 219 s, one test of
which replays a 1900-frame tape five times to attribute a level's ending to one
library entry. Every one of those figures swings with machine load, by half again
here, so treat them as orders of magnitude and never as a budget.

What each one pins down:

- **`the_smb_ram_map_decodes_a_booted_cartridge_and_a_repeated_script_agrees`**
  — the setup script reaches normal play on frame 120, world and level read
  `0-0`, power-up and player state are the expected values, the split absolute-X
  pair crosses the screen under a scripted run, the scripted hops come back as
  airborne frames, an enemy slot reports a goomba near the player, and running
  the identical script a second time reproduces both the state hash and every
  decoded field.
- **`checkpoint_restore_branches_a_frame_and_returns_to_it_exactly`** — four
  macro-actions from one checkpoint land in four different places, and every
  `restore` reproduces the root hash bit for bit. A planner that could not
  rewind exactly would prune on lies.
- **`the_tile_frame_rebuilds_the_background_the_ppu_actually_drew`** — the tile
  under `player_tile` is one the game holds Mario up on and the two rows his body
  occupies are not, which is what pins `PLAYER_FEET_OFFSET_PIXELS` and would have
  caught trap 5; the row is then checked a second way against the bottom of the
  player's own sprite column, so the offset is not merely asserted as a constant;
  the sky filler is `$24` and hashes to an empty pattern; the pixel-by-pixel
  background reconstruction disagrees on zero of more than 40,000 sprite-free
  pixels, which is what would have caught the `v`-versus-`t` scroll trap; and a
  400-frame sweep repeats the terrain-versus-scenery split on every grounded
  frame rather than only the sampled one.
- **`the_tile_that_stops_the_player_is_not_one_the_reader_calls_solid`** — plays
  the shared `BaseBot` rather than a scripted input, waits for 120
  frames that cover no new ground, and reads the first cell ahead that draws
  anything in the rows his body occupies. The game has stopped him; the tile that
  did it is not in the load-bearing set and `wall_ahead` says nothing is there.
  That is trap 7 as a regression test: the day the set learns about walls, this
  fails and says which section to rewrite. What it pins is narrower than it read
  when it was written, and worth saying exactly: a later layer does name that
  column now, once the game has refused him, and
  `remembering_a_refusal_names_a_wall_the_tile_ids_do_not` measures that half.
  This one is the floor underneath it — a cold model holding one frame has no wall
  to name, however much the nametable draws. It also pins trap 8 from the other
  side, because it waits for the level load to hold for eight consecutive frames
  before it starts, and the run it measures is the one the document reports.

- **`remembering_a_refusal_names_a_wall_the_tile_ids_do_not`** — the A/B above, as
  a regression test: one checkpoint state, two tails, and the ledger as the only
  difference between them. It asserts the restore is exact before it asserts
  anything about memory, because a leaky restore would produce the same "the warm
  tail did better" result for the wrong reason. The refusal it teaches is picked by
  construction — the first one whose artwork the level draws again at least 180 frames
  later, with the checkpoint taken 60 frames before that second drawing — because 1-1's
  first two refusals are airborne block bumps whose patterns never come back, and a test
  built on one of those would pass while measuring nothing. It asserts the remembering
  tail's furthest absolute X as well as the frame and pixel its ledger names, so a memory
  that only ever delayed the body by a frame cannot satisfy it.
- **`a_committed_tape_replays_to_the_bits_it_was_recorded_at`** — every committed
  gym's tape lands on its golden hash, on the frame it claims, with the player in
  control, in the level its own start bytes name, at or behind the absolute X its
  recorded verdict reports. The level assertion follows `tape.start` rather than
  hard-coding 1-1, because a replay that dropped the write would boot into world 1
  and still be a tape someone could call a pass. This is the golden hash doing its
  job: a core change that moves SMB fails here, by name.
- **`replaying_the_same_tape_twice_lands_on_the_same_bits`** — DirtSim's
  `ReplayFromRootIsDeterministic`: one tape replayed twice, 60 frames of one button
  from each landing, same absolute X every frame, same final hash.
- **`every_committed_gym_still_measures_the_verdict_it_carries`** — walks
  `gyms::ALL`, so a gym cannot join the tree with a verdict nobody re-measured. The
  sweep used to walk a list hand-copied into this file, and had become three of the
  six gyms without anyone noticing; both sweeps now read the same table the examples
  do.
- **`remembering_the_run_up_changes_nothing_at_the_camera_limit`** — the cold and
  warm pair, and the finding that they agree.
- **`a_tape_that_does_not_hold_fails_as_itself`** — a truncated tape, a tape on
  another cartridge, and a replay that lands one bit away, each reported as
  `ShortTape`, `WrongCartridge` and `Drifted` rather than as a strange verdict.
- **`the_script_hammers_a_wall_the_reflex_jumps`** — one tape, two players, and the
  pair that says what `plant-wall-304` is about. The script comes to `Stuck` after 141
  frames at absolute X 291 with fifteen refusals; the reflex is `Met` after 68 frames
  and one refusal at 335. Both are refused by the same column, so the gym cannot be
  read as a hard obstacle: the column is 64 px, inside the long jump's measured rise,
  and the reflex clears it with a longer hold. What the script loses is its own run-up —
  `hop-obstacle` lands it flush against the wall it then has to jump from — and a
  takeoff that fails its own postcondition leaves no trace in the model at all, which is
  the finding the tall-wall skill has to be written against.

The tests **fail closed on a different ROM revision** rather than misreading
it: they check the cartridge's SHA-256 against the image the map was verified
against, and `cartridge::machine` refuses any image whose identity is not the
pinned one, so `smb2.nes` or a different `smb.nes` header reports
`cartridge is not the revision this RAM map was verified against` instead of
returning plausible nonsense. That covers the first item under Gaps.

Still missing, in the order they would pay off:

1. A committed boot-to-gameplay state hash and fixed-input hash, so a core
   change that silently alters SMB is caught even without a cartridge. The tape
   above is a golden hash, but a hash of a state only a cartridge can produce, so
   it catches a core change on the machine that has the ROM and nowhere else.
2. A cartridge-profile module keyed on canonical cartridge identity, so the map
   is data rather than a test support file.
3. A versioned semantic payload on `NesObservation` for a bot to consume.
4. The rollout and DFS harness on top.
