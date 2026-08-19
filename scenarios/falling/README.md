# Falling scenario asset

This scenario bundles `falling.nes` from
[`xram64/falling-nes@52dcb8a951200562e696dfc2aba5d4d14edd0078`](https://github.com/xram64/falling-nes/commit/52dcb8a951200562e696dfc2aba5d4d14edd0078).

- File size: 40,976 bytes
- SHA-256: `e22b947542c2d7e595bf84725b333be7af8189c5965b9c53e356a249c7d79943`
- Canonical iNES FNV-1a 64: `16a4d7eebe1afc30`
- License: MIT; see [`assets/LICENSE`](assets/LICENSE)
- Copyright: 2018 tragicmuffin

The ROM is consumed at compile time with `include_bytes!`; the scenario never
depends on a current working directory or a writable filesystem.

Run it from the repository root with:

```sh
cargo run -p engine-client -- --scenario falling
```

The game uses d-pad directions and Start; standard A/B input is passed through
as well. On gamepad, Select is reserved for the host controls menu. On keyboard,
use arrows, `Z`/`Space` for A, `X` for B, `Tab` for NES Select, `Enter` for
Start, and `Esc` for the host pause menu. The client sends the deterministic
48 kHz APU output through a shallow bounded device buffer; if no output device
is available, gameplay continues silently.

Run the same bundled-ROM full-output and headless benchmark used for desktop
and Raspberry Pi validation with:

```sh
cargo run --release -p scenario-falling --bin falling-benchmark -- 2000 120
```

The two newline-delimited JSON rows include core execution average and
p50/p95/p99/max frame times plus wall time including output checksum
validation. They must finish with the same authoritative state hash regardless
of whether video and audio output are enabled.
