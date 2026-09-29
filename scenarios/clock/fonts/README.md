# Bundled Clock faces

`DejaVuSans.ttf` and `DejaVuSerif.ttf` are unmodified DejaVu 2.37 files from
Ubuntu's `fonts-dejavu-core` package, version `2.37-8build1`. Their upstream
source is recorded in [LICENSE.txt](LICENSE.txt), which contains the Bitstream
Vera permission notice and the public-domain notice for DejaVu changes.

`../build.rs` uses pinned fontdue 0.9.3 to rasterize digits 0–9 at 128 pixels.
All ten digits share one bounding box. Each of the 6×9 output cells averages
16×16 coverage samples, using a 28% threshold. The build emits ten 54-bit masks
per face; source fonts are not loaded on the device. No system fonts, network
access or separately generated files are required to reproduce the masks.

Classic comes from the existing seven-segment geometry. Matrix resamples the
project's code-native 5×7 digit alphabet into the common 6×9 grid. All four
faces use the same cell positions, glow, physics and water support geometry.
