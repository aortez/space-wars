# Picade and the shared Pi 4/5 image

The `raspberrypi-spacewars` Yocto machine builds one 64-bit USB image for
Raspberry Pi 4 Model B and Raspberry Pi 5 Model B. It uses a Cortex-A72-compatible
userspace and a common 4 KiB-page kernel, installed as both `kernel8.img` and
`kernel_2712.img`. Board-specific device trees and KMS overlays are included.
This follows DirtSim's shared-machine approach without inheriting its
Miuzei-specific HDMI timings or touchscreen wiring.

Pi 4 cabinet testing verified USB boot, HDMI, joystick/action-button input,
the side-button Escape/Enter menu controls, Falling/NES gameplay, speaker
output, and the Sound panel. Two-player NES testing also confirmed that an
attached USB gamepad controls P2 while the cabinet controls P1. The new image's
Pi 5 profiles and power-button shutdown still need hardware validation;
a successful build does not prove those behaviors.

## Hardware profiles

The same image includes three mutually exclusive profiles:

| Profile | Display | Audio | Input |
| --- | --- | --- | --- |
| `hdmi` | Connected HDMI output, no rotation, advertised mode | Matching HDMI audio card when detected | USB keyboard/gamepad |
| `hyperpixel` | HyperPixel 4.0 rectangular DPI, 90-degree rotation | Existing USB adapter (`Audio`) | Existing touchscreen/gamepads |
| `picade` | HDMI, no rotation, advertised mode | Picade X HAT USB-C I2S speaker | Cabinet gamepad plus utility keys |

The Picade HAT profile currently targets **Pi 4**, not Pi 5. The shared image
supports Pi 5 with HDMI or HyperPixel; Pi 5 Picade HAT GPIO/I2S support needs its
own hardware validation. Never combine the Picade and HyperPixel overlays:
they use overlapping GPIO pins.

The image defaults to generic HDMI. Flashing requires an explicit `--profile`.
Selection lives in `/boot/spacewars-device.txt`, for example:

```text
[all]
include spacewars-profiles/picade.txt
[all]
```

This is also how to change profiles manually: select exactly one included file
and reboot. Do not edit `config.txt` to add a second hardware overlay.
`spacewars-hardware.service` reads the same selection and writes the kiosk's
display/audio environment under `/run/spacewars-hardware/hardware.env`.
It does not execute the boot file as shell code. HyperPixel's existing
backlight workaround only runs for its profile.

Optional local environment overrides go in `/boot/spacewars-kiosk.env`:

```text
SLINT_DRM_OUTPUT=HDMI-A-1
SLINT_KMS_ROTATION=0
```

Both files survive rootfs-only A/B updates. A full reflash replaces the boot
partition: reselect the profile and back up any custom boot overrides first.
The flash tool's `/data` backup does not include these boot files.

## Picade controls

The image packages a pinned version of [Pimoroni's HAT overlay](https://github.com/pimoroni/picade-hat).
No RetroPie installation or first-boot network installer is required.
The overlay patches expose two independent Linux input devices:

- `Space-Wars Picade`: joystick, six action buttons, Coin and Start, using
  `gpio-keys-polled` with a 4 ms poll interval and two digital hat axes. This
  driver advertises proper axis ranges and returns them to zero on release.
  The app's gilrs backend requires two axes; changing four keys to D-pad
  button codes alone is insufficient.
- `Space-Wars Picade Keys`: Enter, Escape and Power, using interrupt-driven
  `gpio-keys`. Its GPIO/pinctrl group is separate from the gamepad's.

Do not merge the utility keys back into the gamepad. libinput deliberately
ignores gamepad-like devices even when they also advertise a few keyboard keys
and `ID_INPUT_KEYBOARD`. The gamepad and keyboard backends must each see their
own device. The build validates the compiled overlay's device separation,
GPIO ownership, event codes, axis values and parameter targets before deploy.

| HAT connector | Application input |
| --- | --- |
| Joystick | D-pad |
| Button 1 | South / A / menu confirm |
| Button 2 | East / B / menu back |
| Button 3 | West |
| Button 4 | North |
| Button 5 / 6 | Left / right shoulder |
| Coin | Select |
| Start / 1UP | Start |
| Enter | Keyboard Enter / confirm in menus |
| Escape | Keyboard Escape / pause and menu back |
| Power | Linux power key; controlled shutdown and HAT power-off |

The existing gamepad seat assignment and menu-to-game neutral/release gate
apply without a userspace input translator. Scenario-specific mappings remain
those shown by the application's Controls menu. In Clock, HAT Button 3 / West starts
the next enabled event. Button 4 / North starts or dismisses a player duck;
the joystick moves it and Button 2 / East jumps. South/A is also a jump binding
on working controllers, but the non-reporting switch is not required. Use Escape or Start to pause, then choose Clock
Controls or Launcher. Enter confirms menu selections;
Coin/Select opens controls help. Physical button layout can be adjusted by
changing the `dtparam=buttonN=...` bindings in the Picade profile and rebooting.

### Record the physical button layout

Connector numbers do not prove front-panel positions. The following layout was
recorded on `sw-picade-2` on 2026-09-19, viewed from the player's position:

| Physical position | Color | HAT connector | Linux code | Logical input |
| --- | --- | --- | --- | --- |
| Top left | Yellow | Button 6 | 311 / BTN_TR | Right shoulder |
| Top middle | Pink | Button 5 | 310 / BTN_TL | Left shoulder |
| Top right | Blue | Button 3 | 308 / BTN_WEST | West / Clock Next Event |
| Bottom left | Pink | Not verified | No event | Not registering |
| Bottom middle | Yellow | Button 2 | 305 / BTN_EAST | East / B / Clock jump |
| Bottom right | Blue | Button 4 | 307 / BTN_NORTH | North / Clock player duck |
| Left side utility | Black | Escape | 1 / KEY_ESC | Pause / menu back |
| Right side utility | Black | Enter | 28 / KEY_ENTER | Menu confirm |

The ordered raw capture received five front-button codes (311, 310, 308, 305,
307), followed by Escape/Enter. The user then individually identified the
bottom-left pink button as producing no output, resolving the missing position
in that sequence. The top-right mapping was also separately confirmed by
repeated isolated presses. South/A (304) is the only configured front-button
code not observed; Button 1 is therefore a candidate for the bottom-left
connection, not a verified wiring assignment. The cause of the missing input
(switch, wiring, or GPIO configuration) has not been diagnosed.

This records the full physical layout, including one non-reporting button;
it does not establish eight working inputs. `sw-picade` still needs its own
verification. Colors and wiring may differ between cabinets. The top-right
Clock-only binding does not change menus, NES, or other scenarios; ordinary
gamepads use right shoulder.

In Clock, bottom-right blue starts/dismisses a player duck, joystick moves it,
and bottom-middle yellow jumps from solid ground. Top-right blue cycles compatible
visual events, Rain, Falling and Meltdown without ending the visit. In water, the joystick paddles;
neutral floats/drifts with the current. Rain shares the fixed course; joining a
shower already in progress instead keeps its water and moving floor. If a passive
duck is already floating, it becomes yours in place; otherwise your duck enters
through its door. Rain does not spawn another duck after a player joins. The
panels stay physical and gently close after the shower ends. Falling bars and
the player share one physics world: bars can push the duck, and it can stand/jump
on them. Leaving removes only the duck; the blocks finish their event, and a new
visit can reuse that arena. Meltdown's individual blocks can push the duck too;
they become water on floor contact, not on duck/block contact. The automatic duck
course and developer water-lab previews wait until the player leaves. Saved event
settings are not changed.

Run `python3 tools/capture-picade-buttons.py --host sw-picade-2.local` from the
workstation. It discovers only the two Picade input devices, reads them over SSH,
and prints a guided capture: top row left/middle/right, bottom row
left/middle/right, then left/right utility. Press/release each once, about one
second apart; never press Power. It installs nothing, changes no configuration,
and stops after eight buttons or a bounded timeout. The application still sees
the input. Start from Clock; utility keys may pause/resume it.
Use `--address <known-IP>` for an mDNS fallback while retaining the named SSH
host identity. Capture each cabinet separately and record the verified position,
color, connector, Linux event code and logical input here. `--raw` prints
individual press/release codes without assigning positions, useful for isolating
a missing button. If a button produces no output, record its physical position
explicitly instead of shifting the remaining sequence labels. The guided capture
expects eight working buttons and cannot complete with a non-reporting button.

### Simulated button verification

`spacewars-cli input press west --profile picade --hold-ms 1200` now provides
bounded virtual-controller presses through the app's shared routing, without
needing the HAT. `input press start` opens pause and `input press south` confirms
Resume. Use `--expect-screen gameplay` or `pause.main` as appropriate; `--json`
reports release completion, including early cancellation on context changes.
The app releases the virtual input even if the CLI exits. See the
[runtime control instructions](../README.md) for player selection and bounds.

`spacewars-cli ui press` sends semantic menu actions. It intentionally does not
claim to test physical-controller mapping, holds, releases, or the Linux input
backend. The virtual-controller CLI also bypasses Linux device recognition;
use the separate workstation diagnostic for that OS input path:

```sh
python3 tools/picade_input.py --host sw-picade-2.local verify-clock
python3 tools/picade_input.py --host sw-picade-2.local press west \
  --expect-screen gameplay --hold-ms 1200
```

The same `--address <known-IP>` fallback preserves the named SSH identity. Start
with Clock running and at least one event enabled; leave the cabinet controls
untouched. The diagnostic discovers/rechecks the two named Picade input devices,
requires the expected Clock screen, and injects an evdev press/release pair.
Each hold is limited to 50–2000 ms. Release traps and a five-second watchdog run
on the Pi, not across timed SSH round trips. No software is installed, no device
is grabbed, and Power is not an available input. Screen checks are preflights,
not locks against concurrent human input. A failed verification stops further
presses and may leave the pause menu open; buttons still release.

`verify-clock` checks one event per held West press, a second event after a fresh
press, ignored West input and frozen simulation while paused, Escape/Enter
pause/resume, and South/A confirmation. It verifies the Clock instance and saved
preferences remain unchanged. These checks passed on `sw-picade-2` in automatic
Clock mode on 2026-09-19. Injected South/A successfully confirmed Resume, but
that does not verify the non-reporting pink switch, its wiring, or GPIO sensing.
Manual/automatic equivalence, touch release behavior, and menu handoff guards
also have backend-neutral Rust tests.

Run the diagnostic's host-only safety tests without connecting to a cabinet:

```sh
python3 -m unittest discover -s tools -p 'test_picade_input.py'
```

### Controller seats

With the cabinet and a USB gamepad attached, both can navigate host menus but
their gameplay inputs remain separate: the cabinet occupies P1 and the USB
gamepad P2 in the tested setup. A single-player NES game can therefore appear
unresponsive to the USB gamepad even though it works in two-player mode.
Visible device/player assignment and swapping are deferred to
[issue #57](https://github.com/aortez/space-wars/issues/57); this bring-up does
not merge both devices into P1 or change player assignments during gameplay.

Upstream currently does not declare a source license. The recipe explicitly
uses Yocto's `CLOSED` license marker rather than attributing our MIT license to
Pimoroni's code. Review redistribution permission before publishing images
containing that overlay. The upstream source is fetched, not vendored here.

### Speaker volume

Open **Sound** from either the launcher or pause menu. Select Master volume and
use the joystick left/right to adjust in 1% steps up to 20%, then 5% steps.
This allows quiet levels such as 1–9% without changing existing saved gains.
Select Mute and press A or Enter to toggle it; mute preserves the chosen level.
B/Escape returns to the
previous menu, leaving gameplay paused. In Falling/NES use Escape or the
Start+Select chord to reach the host pause menu (Start alone belongs to the game).

These are application output controls, not changes to the HAT/ALSA hardware
mixer. They cover Falling and user NES cartridges through the shared audio path.
Other scenarios currently produce no sound. New installations default to 25%;
saved levels are not reset on update. Changes take effect before saving, with
**Saving settings…** and a **Retry Save** option if storage fails. Allow saving
to finish before cutting power. Settings are stored in the persistent `/data`
configuration and survive normal reboots and rootfs-only updates.

The UI control API exposes `launcher.sound` and `pause.sound`, with
`sound.volume.previous`, `sound.volume.next`, `sound.mute`, `sound.back`, and
`sound.retry` (only after a failed save). `sound.save-status` is read-only and
reports `saving`, `saved`, or `error`. For example, after opening the appropriate
Sound panel:

```sh
spacewars-cli ui activate sound.volume.previous
spacewars-cli ui state
```

## Build and flash

```sh
cd yocto
npm test
npm run build
```

Host tests require Node.js, Python 3 and `device-tree-compiler` (`dtc`/`fdtget`).
Installer/rollback tests additionally use `bubblewrap` with user namespaces to
isolate the real shell helper from the workstation. CI requires these tests;
locally they report a skip if that sandbox is unavailable.

The CI tools job is pinned to Ubuntu 24.04 and loads the CI-only
[`bwrap` AppArmor profile](../.github/ci/bwrap.apparmor) to permit user namespaces
for Bubblewrap without changing the system-wide restriction. A named preflight
checks namespace creation as the normal runner user before the tests run;
failed jobs report namespace settings and AppArmor/kernel diagnostics. The
profile is not installed by the application build or deployment tools. To
require the same installer coverage locally, run
`SPACEWARS_REQUIRE_UPDATE_SANDBOX=1 npm --prefix yocto test` from the repo root.

The default build directory is outside the Rust workspace and distinct from
other checkouts and the old Pi 5 build:

```text
../.space-wars-yocto-build-raspberrypi-spacewars/
  tmp/deploy/images/raspberrypi-spacewars/
    spacewars-image-raspberrypi-spacewars.rootfs.wic.gz
    spacewars-image-raspberrypi-spacewars.rootfs.wic.gz.boot-id
    spacewars-image-raspberrypi-spacewars.rootfs.wic.bmap
    spacewars-image-raspberrypi-spacewars.rootfs.ext4.gz
    spacewars-image-raspberrypi-spacewars.rootfs.ext4.gz.boot-id
```

`KAS_BUILD_DIR` overrides the build directory for **all** build/flash/update
commands. Reuse download and sstate caches, not a build directory actively used
by another checkout. `rm_work` frees completed recipe intermediates. BitBake
warns below 25 GiB free, stops scheduling tasks below 20 GiB, and halts below
10 GiB. Free space and rerun the same build command to resume; no cache purge
is required.

Keep the cabinet's original SD card safe. Identify the USB drive by its model,
capacity, and device identity every time. The following device name is a
placeholder, not a discovered target:

```sh
npm run flash -- --list
npm run flash -- --dry-run --profile picade --hostname picade --device /dev/sdX
npm run flash -- --profile picade --hostname picade --device /dev/sdX
```

The Picade profile defaults the hostname to `picade` even when a saved flash
configuration refers to the existing kiosk. The login remains `spacewars`.
Use Ethernet/DHCP for initial bring-up, or provision the ignored
`yocto/wifi-creds.local` described in [the Yocto runbook](../yocto/README.md).
Keep the `.wic.gz.boot-id` sidecar beside the USB image too: the flash tool
rejects legacy images without metadata before writing and checks the boot
identity on the flashed drive before selecting its hardware profile.

Do not deploy to `spacewars.local` as part of Picade bring-up. The existing Pi 5
needs separate approval and a full image flash with `--profile hyperpixel`
before moving to this machine configuration.

## First-boot validation

Connect the USB drive, Ethernet, and the existing cabinet HDMI/power wiring.
Do not connect two independent power supplies to the HAT and Pi.
Once booted:

```sh
ssh spacewars@picade.local
uname -a
getconf PAGESIZE
cat /boot/spacewars-device.txt
cat /run/spacewars-hardware/hardware.env
systemctl --no-pager status spacewars-hardware spacewars-seatd spacewars-kiosk
cat /proc/bus/input/devices
aplay -l
spacewars-cli status
```

An administrator with journal access can additionally inspect
`journalctl -b -u spacewars-hardware -u spacewars-kiosk --no-pager -n 100`.
The stock kiosk account does not have general journal access or unrestricted
sudo; use the control CLI for its application status.

Expect a 4096-byte page size, a visible launcher, separate `Space-Wars Picade`
and `Space-Wars Picade Keys` entries in the input device list, and the HAT audio
card. The keyboard entry must not list joystick axes/buttons. Check:

1. Each joystick direction and return to neutral; verify no stuck direction.
2. Enter/Escape utility keys as well as A/B and Start/Select. In Clock, Escape
   opens pause; joystick/A or Enter selects Clock Controls or Launcher.
3. At least one game and Falling/NES, including audible speaker output.
4. Release held controls across menu/game transitions; no unintended firing.
5. The cabinet power button shuts Linux down before power disappears.

If HDMI remains dark, use SSH to inspect `/sys/class/drm/card*-HDMI-A-*/status`
and `modes`, and the kernel log. Determine the actual panel mode before adding
a `video=HDMI-A-1:...` override to the existing single-line `/boot/cmdline.txt`.
Do not guess a panel resolution or overwrite its `root=`/A/B boot parameters.

## Updates and the boot compatibility boundary

```sh
./update.sh --target picade.local
```

Our A/B updater writes **only the root filesystem**, not the shared boot
partition. Each image hashes its required kernel, DTBs, firmware and profile
definitions. The full image places this identity in `/boot/spacewars-boot-id`;
the rootfs artifact carries an `.ext4.gz.boot-id` sidecar.

Before transferring an update, the updater compares that sidecar with the
target's boot identity. Missing or different identities are rejected. This
prevents old Pi 5 boot files from being paired with the unified image's kernel
modules, and catches later boot/kernel changes too. Keep the sidecar beside a
rootfs image when copying artifacts. It is a compatibility marker, not an
authentication/signature mechanism.

Application-only changes can use A/B updates while the boot assets match.
Changed boot assets require a coordinated full reflash, with data backup and
explicit hardware selection. Transactional kernel/boot-partition OTA is not
implemented by this work.
