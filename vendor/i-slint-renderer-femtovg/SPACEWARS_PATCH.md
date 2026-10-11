# Slint FemtoVG output rotation patch

Source: the published `i-slint-renderer-femtovg` 1.13.1 crate, with its original
source, normalized manifest, original manifest and licenses. The workspace and
standalone LinuxKMS backend override this exact version through Cargo patches.

Two local changes support rotated, non-square kiosk displays:

- Keep the canvas at the physical surface dimensions after flushing the
  background for `BeforeRendering`. The upstream notifier branch resets it to
  logical window dimensions, clipping 90/270-degree output even with an empty
  notifier. Window layout and the application's offscreen scene stay logical.
- Record the rendered cardinal output transform before presentation, then undo
  it only when a snapshot is requested. Screenshots use logical coordinates,
  matching software snapshots. The unrotated path keeps its original allocation;
  rotated readbacks allocate one destination buffer. Arbitrary transformations
  retain the previous physical-coordinate behavior. No per-frame copies or
  readbacks are introduced. Screenshot scheduling remains the backend's job.

Pure unit tests cover every pixel/channel of non-square inverse rotations,
transform recognition, invalid inputs and unrotated allocation reuse. The
client's opt-in `--benchmark-presentation --presentation-gpu-ui` fixture checks
physical framebuffer pixels independently of snapshots at 0/90/180/270 degrees,
with and without a notifier and through repeated scene/menu transitions. The
unpatched renderer fails 15 of the 20 rotation cases.

Keep these patches isolated until an upstream version supplies equivalent
fixes. They do not change the selected renderer, geometry policies or timing.
