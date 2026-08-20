# NES ROM Library

Space-Wars can run user-supplied cartridges through the same Rust-native NES
core and bounded realtime presentation path used by the bundled Falling game.
The application does not download ROMs, and the repository does not provide
commercial games. Only use cartridge images that you have the right to use.

## Add a cartridge

The launcher scans a non-recursive `roms` directory beside `settings.toml`:

- Linux default: `~/.config/spacewars/roms`
- Windows default: `%APPDATA%\spacewars\roms`
- Explicit configuration: `<directory>/roms` when using
  `--config-dir <directory>` or `SPACEWARS_CONFIG_DIR=<directory>`
- Raspberry Pi kiosk image: `/var/lib/spacewars/roms`

The application creates this directory when it starts. Copy one or more regular
files with a case-insensitive `.nes` extension into it, then enter or return to
the launcher. Select **NES Library**, open its settings, choose a cartridge, and
start it. The selected cartridge is persisted by its SHA-256 content identity,
so renaming the same file does not lose the selection. Identical images are
shown only once.

A one-off cartridge can instead be launched without adding it to the library:

```sh
cargo run -p engine-client -- --rom /path/to/game.nes
```

`--rom` implies the NES Library scenario and cannot be combined with
`--scenario`. A direct launch still uses the normal pause, restart, audio, and
return-to-launcher lifecycle.

## Sync cartridges to the kiosk

The repository reserves `data/` for local runtime data and ignores the entire
directory in Git. Put legally obtained cartridges in `data/roms/`, inspect the
transfer, and then sync them:

```sh
mkdir -p data/roms
./sync-data.sh --dry-run
./sync-data.sh
```

The script checksum-compares the local directory with
`spacewars@spacewars.local:/var/lib/spacewars/roms` and transfers updates over
SSH with `rsync`. It is additive by default, so unrelated remote cartridges are
not removed. `--delete` explicitly mirrors the local directory and removes
remote ROMs that are absent locally. Host, user, and local/remote data roots
can be changed with command-line options or `SPACEWARS_*` environment
variables; see `./sync-data.sh --help`.

ROMs live on the persistent `/data` partition behind
`/var/lib/spacewars`, independently of A/B root filesystem updates. Return to
the launcher after a sync so it rescans the library.

## Current compatibility boundary

The library currently accepts:

- iNES 1.0 cartridges for the standard NES console;
- mapper 0 / NROM with one or two 16 KiB PRG ROM banks and zero or one 8 KiB
  CHR ROM bank, with zero selecting CHR RAM;
- mapper 1 / MMC1 for the conventional iNES SxROM subset with 2-16
  power-of-two 16 KiB PRG ROM banks (up to 256 KiB), 8 KiB CHR RAM or 1-16
  power-of-two 8 KiB CHR ROM banks (up to 128 KiB), serial register writes,
  16/32 KiB PRG modes, 4/8 KiB CHR modes, mapper-controlled one-screen,
  horizontal, or vertical mirroring, MMC1B PRG-RAM disable, and the hardware's
  consecutive-cycle write filter;
- mapper 2 / UxROM with 2-128 power-of-two 16 KiB PRG ROM banks, a switchable
  `$8000-$BFFF` bank, the final bank fixed at `$C000-$FFFF`, 8 KiB CHR RAM, and
  fixed horizontal or vertical mirroring;
- mapper 3 / CNROM with one or two 16 KiB PRG ROM banks, two or four
  switchable 8 KiB CHR ROM banks, and fixed horizontal or vertical mirroring;
- mapper-0 horizontal, vertical, or four-screen mirroring and optional trainers;
- NTSC execution; and
- the 151 official RP2A03/6502 opcodes.

Generic iNES mapper-2 and mapper-3 bank writes are modeled without bus
conflicts. Mapper-1 SUROM/SXROM 512 KiB outer PRG banking and multi-bank PRG
RAM are not yet supported; images declaring 32 16 KiB PRG banks are rejected
instead of being partially emulated. NES 2.0, other mappers, PAL/Dendy timing,
peripherals, expansion audio, and unofficial CPU opcodes are not yet supported. Battery-backed
cartridge RAM is emulated for a running machine but is not persisted to disk.
Passing header inspection therefore means a cartridge fits the supported
format; it is not a promise that every game is already behaviorally compatible.

The launcher retains rejected, unreadable, and over-limit entries and displays
the reason. Cartridge reads are bounded to 16 MiB, library scans ignore
symlinked cartridge entries, and a selected file is hashed and parsed again at
launch. If its contents changed after scanning, return to the launcher to
rescan it.

## Controls

Each assigned gamepad becomes one standard NES controller. D-pad, `A`, `B`,
`Select`, and `Start` are passed to the cartridge as complete per-frame input
snapshots. Press `Start` + `Select` together to open the host controls menu.
Input must return to neutral after a launcher/game/menu transition before it is
forwarded again, preventing the launch button from also activating the game.

Player 1 keyboard controls are:

| NES input | Keyboard |
| --- | --- |
| D-pad | Arrow keys |
| A | `Z` or `Space` |
| B | `X` |
| Select | `Tab` |
| Start | `Enter` |
| Host pause | `Esc` |

## Architecture

Filesystem discovery and validation belong to `engine-client`; untrusted paths
and bytes never become a core responsibility. A successfully parsed owned
`CartridgeImage` crosses into the platform-independent `scenario-nes` adapter.
Both `scenario-nes` and the bundled `scenario-falling` then use the shared
client-side NES realtime adapter, which owns pacing, bounded video/audio
handoffs, controller snapshots, and lifecycle transitions.

```text
config/roms/*.nes
        |
        v
engine-client catalog and bounded loader
        |
        v
scenario-nes (synchronous, platform-independent)
        |
        v
engine-nes (deterministic machine)
        |
        v
shared client realtime video/audio/input adapter
```

Adding broader cartridge support should extend `engine-nes` rather than add a
game-specific emulator path. See
[`nes-extension-guide.md`](nes-extension-guide.md) for the mapper, state,
testing, and benchmarking obligations.
