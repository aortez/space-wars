# reference/

Preserved artifacts from the 2008 UW Bothell CSS 450 original
(Allan + Chris, Java + JOGL). Kept for design reference during the
Rust + Slint reboot — see [`../docs/design/reboot-rust-slint.md`](../docs/design/reboot-rust-slint.md).

## Contents

- `Final.jar` — Compiled 2008 game. Runs on Linux with `libjogl-java`
  installed (see `README.TXT`).
- `README.TXT` — Original run instructions.
- `docs/`
  - `css450_proposal.pdf` — 2008 project proposal.
  - `Allan_Chris_css450_Final_Report.pdf` — 2013 final report
    (filename date; project itself is 2008).
- `src-decompiled/` — CFR decompiler output of `Final.jar`. ~4,554 lines
  of Java across 32 game files (Ship, Planet, Cannon, Laser, EscapePod,
  Debris, Particle, BGStarField, Wing_Behavior, etc.) plus ~5,305 lines
  of in-house scene graph under `UWBGL_JavaOpenGL/`.
  - **Treat as a gameplay spec, not as code to port.** Physics constants,
    weapon behavior, wing-fold states, and ship tuning are all readable.
    The `UWBGL_*` scene-graph classes are obsolete — Slint provides the
    rendering layer in the reboot.
- `lib/` — JOGL + Swing/Beans support jars needed to run `Final.jar`.
- `rec/` — 2008 game assets: sprites (sun, planets, asteroids, landing),
  sounds (`.wav`, `.au`, `.aiff`).
