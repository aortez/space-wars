# NES world-model reader

This document records the investigation into reading semantic game state out of
the Rust NES core so a host can build bots that play a cartridge, using Super
Mario Bros. (`data/roms/smb.nes`) as the target. It is an investigation, not a
feature contract: nothing here has been promoted into `engine-nes` or
`scenario-nes` yet.

The conclusion is that **no emulator change is required**. The core already
exposes every channel a world-model reader needs, and the address map, tile
decode, rollout and search design already exist in DirtSim's C++ NES
subsystem. The work is a port plus a place to put it, not new hardware
emulation.

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

DirtSim reports only the two nearest enemies by squared distance, which keeps
the sense vector fixed-width.

## Boot contract

The reader has to get into gameplay before any of the map above means anything.
DirtSim's policy is a scripted single Start press on frame 120, gameplay
detected when `$0770` becomes 1, the setup script ends at frame 300 and a run
that has not reached gameplay by frame 500 is a setup failure. The Rust
examples use the same policy and reach gameplay on frame 120.

## Tile-frame reader and six decode traps

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
51,136 compared pixels, none wrong. Six traps came out of the decode, four
from that pixel comparison and two from joining the terrain to the player.

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
   a feet tile on none, while the `$b4`/`$b5` ground, the `$60`-`$63` pipe tops
   and bodies, and the `$45`, `$57` and `$58` solid blocks (each drawn as a lit
   top edge over a filled body) only ever held his feet. A reader that derives
   walkability from pixel coverage parks a bot inside a hill.

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
- **The nametable is a sliding ring buffer, not a level map.** SMB rewrites
  nametable columns as the camera scrolls and the pair of tables wraps every 512
  pixels, so the leftmost columns of page 0 end up holding content from X = 512
  and beyond. The reader sees the screen the camera is on, never the level. A bot
  that wants a map has to accumulate windows across a rollout, and a bot that
  wants "what is over the next hill" has to move the camera to find out.
- **`$0086` is position within the 256-pixel page, not relative to the camera.**
  On-screen position needs `wrapped_delta(absolute_x, scroll_x)`; subtracting the
  scroll from the screen byte mixes two different origins.

The dynamic half is thinner than the terrain half. The game keeps five enemy
slots live at once, so the reader sees at most five enemies, and only the ones
the game has spawned, which is a screen or so ahead of the camera. In a probe run
that reads as a goomba (type 6) 110 pixels ahead, then 10, then gone.

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
for a level clear, a no-progress timeout at 1800 gameplay frames, and a
below-screen terminal test at player screen Y above 224.

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

## What is in the tree now

One reader, three drivers, one optional test.

| File | Role |
| --- | --- |
| `crates/engine-nes/tests/support/smb_world_model.rs` | The reader: RAM map, setup policy, PPU snapshot, tile decode, background reconstruction |
| `crates/engine-nes/tests/smb_world_model.rs` | Optional test that asserts the reader against a real cartridge |
| `crates/engine-nes/examples/world_model_probe.rs` | Prints decoded state while Mario walks right |
| `crates/engine-nes/examples/world_model_tiles.rs` | Prints the tile grid, the player's cell and the pixel comparison |
| `crates/engine-nes/examples/world_model_rollout.rs` | Nametable dump plus checkpoint/branch/rewind with throughput |

The reader sits under `tests/`, not `src/`, on purpose: `engine-nes` holds no
game knowledge, so the SMB address map belongs on the host side of that
boundary. The examples reach it with `#[path]`, which is why the module carries
one `#![allow(dead_code)]` — each build uses only part of the map.

The drivers are exploration scaffolding, not a supported interface. The test is
the part with a contract.

```sh
cargo run --release --locked -p engine-nes --example world_model_probe \
  -- data/roms/smb.nes 700
cargo run --release --locked -p engine-nes --example world_model_tiles \
  -- data/roms/smb.nes 900 30
cargo run --release --locked -p engine-nes --example world_model_rollout \
  -- data/roms/smb.nes
```

Observed on x86-64 Linux with a release build:

- `world_model_probe` reaches gameplay on frame 120, reads world `0-0` (1-1),
  walks absolute X from 47 to 295, sees a goomba (type 6) 110 pixels ahead and
  then 10, and drops lives from 2 to 1 when Mario dies in player state `0x0B`.
- `world_model_tiles` decodes a grounded frame's 32x28 grid, puts Mario on
  ground tile `$b5` at tile `(5,25)` with hill decoration in the two rows his
  body occupies, confirms the mid-air tile is `$24` with an all-zero pattern,
  and reconstructs the background from the nametables and CHR with `0` wrong
  pixels out of 51,136 compared. Passing a third argument samples later frames,
  where the camera is moving and the residual described above appears.
- `world_model_rollout` branches four 120-frame rollouts from one checkpoint
  and gets four different outcomes (idle, run into a pit, run and jump, jump in
  place) and reproduces the root state hash exactly on every `restore`. Its
  frames-per-second figure swings with machine load, so treat it as an order of
  magnitude: several hundred frames per second including checkpoint restore,
  comfortably above the 60 fps the kiosk needs.

## Optional cartridge test

Every other test in this crate drives a `test_rom` builder, so semantic reads
of a shipped game had no regression net: the mapper tests use generated ROMs,
the reference hashes are opaque rather than meaningful, and the only semantic
game-state assertion anywhere was
`scenarios/falling/src/lib.rs::observation_has_a_stable_decodable_v1_shape`.

`crates/engine-nes/tests/smb_world_model.rs` closes that, and it is optional by
necessity rather than preference: `.gitignore` keeps all of `data/` out of the
repository, so no cartridge is checked in and CI has nothing to boot. The test
skips with a one-line note when the image is absent, and passes in 0.00s:

```sh
cargo test --locked -p engine-nes --test smb_world_model          # skips without a ROM
SPACEWARS_SMB_ROM=/path/to/smb.nes \
  cargo test --locked -p engine-nes --test smb_world_model        # runs with one
```

It defaults to `data/roms/smb.nes` resolved from `CARGO_MANIFEST_DIR`, so it
works from any working directory. With the cartridge present the three tests
take about 27 seconds in a debug build, which is emulator frames, not
assertions.

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

The test **fails closed on a different ROM revision** rather than misreading
it: it checks the cartridge's SHA-256 against the image the map was verified
against, so `smb2.nes` or a different `smb.nes` header reports
`cartridge is not the revision this RAM map was verified against` instead of
returning plausible nonsense. That covers the first item under Gaps.

Still missing, in the order they would pay off:

1. A committed boot-to-gameplay state hash and fixed-input hash, so a core
   change that silently alters SMB is caught even without a cartridge. The
   hashes above are asserted only against each other, not against a golden
   value, because the ROM that produces them is not in the repository.
2. A cartridge-profile module keyed on canonical cartridge identity, so the map
   is data rather than a test support file.
3. A versioned semantic payload on `NesObservation` for a bot to consume.
4. The rollout and DFS harness on top.
