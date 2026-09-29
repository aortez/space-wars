# Controller profiles

Open **App Settings → Controllers** from the launcher or pause menu. Select a
device, or choose **Identify by pressing a button** and press one of its buttons.
The device's current P1/P2 assignment is shown. Select **Use as Player 1** or
**Use as Player 2** to change it; merely opening settings does not claim a seat.
A paused game stays paused throughout setup.

## Bluetooth controllers (Linux)

Choose **Bluetooth controllers…** in the same screen. Use the cabinet buttons,
an already connected controller, keyboard, or touch to complete setup:

1. Put the new controller in pairing mode. Choose **Scan for controllers**.
   Scanning has a 30-second limit and stops when you leave/start an operation.
2. Select the controller by name and Bluetooth address, then **Pair and connect**.
   Keep it awake and nearby. Pairing has a 40-second limit; connection operations
   have a 15-second limit. **Cancel operation** or **Back** remains available.
3. Go Back to the ordinary Controllers screen to assign P1/P2 and test buttons.
   Pairing does not replace an occupied/reserved player slot or remap controls.

Remembered controllers offer **Connect**, **Disconnect**, and **Forget
controller…**. Forget requires a second confirmation; it removes the Bluetooth
bond, not button profiles or player preferences. Cancelling a pairing that has
already completed does not delete that bond; use Forget explicitly if needed.
Reconnect uses BlueZ's trusted-device behavior; the controller must also wake
and initiate/accept a connection. It never automatically resumes a paused game.
Pairing always requests profile connection, even if BlueZ already reports a
Bluetooth link. A link alone does not mean the gamepad's HID input is ready.
The device detail shows the current Bluetooth connection separately from the
last operation's result, updating while this screen is open. Use the ordinary
Controllers screen to verify input and player assignment.
Setup operations log their start and outcome in the app log
(`spacewars-cli logs --lines 80`), without polling or button-event logging.

Only devices identified as game controllers by BlueZ's icon, device class, or
BLE appearance are listed. If none appear, check input mode and pairing mode,
then rescan. This first pass supports no-PIN gamepad pairing (including the
8BitDo Micro's previously tested D mode), not PIN-entry keyboards or general Bluetooth
accessories. Other operating systems use their own Bluetooth settings; their
connected controllers still use this app's assignment/mapping UI.

The UI never waits on Bluetooth calls. A bounded background worker and private
application pairing agent exist only while this panel is open. The agent only
authorizes the selected controller and HID services. Scanning does not change
adapter power, global discoverability/pairability, or the default agent, and
leaving releases only this application's discovery request. The kiosk's BLE
Wi-Fi provisioner can continue using the same adapter. If the adapter is off,
missing, or inaccessible, the panel reports it without changing system policy.

BlueZ owns bonds separately from app settings. New Yocto images mount its
root-only `/data/bluetooth` storage at `/var/lib/bluetooth`; a **full OS update**
is needed for that persistence service. An app-only fast update adds the UI but
does not make an older image's pairing keys survive an A/B update. See the
[first-upgrade caveat](pi-kiosk.md#bluetooth-pairing-storage). Desktop Linux uses
the system's ordinary BlueZ storage. Physical identity for two identical pads
is still subject to the model-based assignment limits below.

## NES button layout

NES Library and Falling translate logical **East (right face button) to NES A**
and **South (bottom face button) to NES B** for both players. This follows the
Nintendo layout: on an SN30 Pro, the printed A jumps and B runs in Super Mario
Bros. The mapping does not depend on the reported device name: a Nintendo-layout
controller may identify itself as an Xbox controller over USB.

Both players can use the **D-pad or left stick** for NES directions. Stick
input becomes digital at half travel on each axis; smaller movement is ignored
to avoid drift. D-pad input takes priority on each axis, and diagonal input is
supported. This also supports controllers such as the 8BitDo Micro when their
physical D-pad reports left-stick axes. The right stick is not used for NES.

This is a scenario binding, not a global controller remap. The setup/tester's
`A / South` and `B / East` labels describe the common host controls, not NES
buttons. Menu confirm/back and other scenarios are unchanged; NES keyboard
controls remain Z/Space = A and X = B. Custom profiles first map physical inputs
to logical positions; NES then applies the same East/South binding. On a pad
with Xbox-style printed labels, the NES binding follows positions, not letters.

## Player assignments

- Moving a seated controller swaps it with the destination slot. For example,
  choose the USB gamepad and **Use as Player 1**: it now controls single-player
  NES/Falling, while the cabinet becomes P2. Scenarios keep independent inputs.
- Choosing an unassigned controller replaces the destination; the displaced
  controller becomes unassigned. It is not silently moved to another player.
- Any connected controller—including an unassigned one—can navigate host menus.
  Unassigned controllers do not send gameplay or Clock event/duck actions.
- Assignment changes immediately clear both players' held gamepad state and any
  CLI input lease. Each physical controller must return to neutral before its
  buttons/directions are forwarded again. No confirming A becomes a NES button
  or ship laser on resume. Keyboard and Picade utility keys are unchanged.
- Explicit preferences persist locally as `controls.player_1_device` and
  `controls.player_2_device`. Without preferences, devices take free slots in
  connection order.
  Preferences use the same OS-specific **model** keys as profiles, never backend
  connection IDs. Assignments apply across scenario launches and app restarts.
- A disconnected controller reserves its slot. A uniquely distinguishable model
  can reclaim it even with a new connection ID; a different device cannot steal
  it. Manual gameplay still pauses on a seated controller's disconnect, and
  reconnecting does not automatically resume. Replace the slot explicitly or
  use **Reset player assignments** to forget reservations and saved preferences.
- Identical models are not individually identifiable. Their live assignments
  work, but are **session-only** and are not saved. Once duplicate models have
  been observed in a session, reconnect requires an explicit choice, even if
  the backend reuses an ID. At startup, an ambiguous saved model is left reserved
  rather than guessing which physical pad it represents. Menus remain usable.
- Resetting player assignments chooses the first two available devices and
  leaves button profiles unchanged. Saving uses the shared background writer;
  a failed save keeps the live assignment and offers App Settings → Retry Save.

## Setup and recovery

1. Choose **Set up mapping**. Release everything before each prompt, then press
   and release the desired input. Directions accept D-pad buttons or centered
   stick directions. A/South and B/East are required for menu navigation.
2. Assign the remaining logical buttons. For a missing optional button, **hold
   any button for two seconds, then release** to skip it. The touch Skip button
   does the same. A skipped button is disabled, not inherited from the old map.
3. Choose **Try this mapping**. This starts a 15-second trial. Using the *new*
   mapping, press A/South to keep and save, or B/East to revert. Doing nothing
   restores the previous mapping. Touch/keyboard confirmation works too.
4. Use **Test buttons and joystick** to check presses, holds, releases, and
   simultaneous inputs. Hold B/East for two seconds to leave the tester; it
   also has a Back button and a 60-second automatic return.

Duplicate assignments are rejected. Capturing only listens to the selected
device; another controller cannot accidentally fill a prompt. Holding an input
for two seconds during a **required** prompt cancels setup. Escape or touch
Cancel works throughout. Each capture prompt times out after two minutes.
Disconnecting the selected device or closing its settings screen discards the
draft/trial. All profile/flow changes require neutral input before normal menu
or gameplay input resumes.

**Try default mapping** has the same timed confirmation/revert behavior. Saving
uses the ordinary ordered background settings writer. If saving fails, the
confirmed mapping works for the current session and App Settings offers Retry
Save. No settings I/O occurs during normal controller polling.

## Consistent cabinet positions

Colors differ between the two cabinets; use positions viewed from the player.
To retain the existing `sw-picade-2` Clock layout on both cabinets, assign:

| Position | Logical control |
| --- | --- |
| Top left | Right shoulder / R1 |
| Top middle | Left shoulder / L1 |
| Top right | X / West (Clock next event) |
| Bottom left | A / South (menu confirm) |
| Bottom middle | B / East (menu back) |
| Bottom right | Y / North (Clock join/dismiss duck) |

This is a suggestion, not a hard-coded position map. The setup prompts for
logical functions, so any working button can be assigned. Picade utility
Escape/Enter buttons are a separate keyboard device and remain unchanged.
See the [measured raw layouts and photo](picade.md#record-the-physical-button-layout).
A profile cannot repair a switch that produces no input events.

## Persistence and scope

Profiles live in `controls.controller_profiles` in the local settings TOML
(`/data/spacewars/config/settings.toml` on kiosks). They are not checked into
source control or copied between cabinets by the app update process.
The shared [settings recovery rules](settings.md) preserve unknown settings and
default invalid fields without resetting unrelated preferences. Structurally
malformed profile records are skipped with a backup and warning, not partially
applied; valid sibling profiles survive.

Identity uses the backend-schema version, OS, gilrs UUID, vendor/product IDs,
and OS-reported name—not a temporary connection ID, port order, or player seat.
**Identical controller models share a profile on the same machine**, including
during the trial. Different wiring in identical simultaneously connected pads
requires a future per-instance identity/selection design; we do not pretend
that a model UUID is a unique serial number. Profiles from another OS do not
match. Invalid/duplicate saved profiles are ignored and reported in setup.

The existing gilrs device normalization (including the retro USB axis fix)
runs first. Custom profiles then translate physical codes into the common
controller layout for both button edges and continuous held snapshots. Scenario
code keeps its existing actions. Unclaimed analog sticks retain their normal
behavior; a stick axis assigned to digital directions no longer sends its old
analog direction as well. Whole-stick calibration, keyboard remapping,
and scenario-specific Jump/Run bindings are separate work.

## Automation and checks

```sh
spacewars-cli ui activate launcher.sound
spacewars-cli ui activate settings.controllers
spacewars-cli ui state --json
# Bluetooth uses the same UI actions (no separate privileged CLI API):
spacewars-cli ui activate controllers.bluetooth
spacewars-cli ui activate controllers.bluetooth.scan
spacewars-cli ui state --json
# Select the exact controllers.bluetooth.device.<path> ID from that snapshot.
# Back stops this app's scan and returns to controller assignment.
spacewars-cli ui activate controllers.back
# Use the connection ID listed in the snapshot, not a hard-coded cabinet ID:
spacewars-cli ui activate controllers.device.0
spacewars-cli ui activate controllers.assign-p1
spacewars-cli ui activate controllers.back
spacewars-cli ui activate controllers.back
```

Use `pause.sound` from a paused game. Screen IDs are `launcher.controllers` and
`pause.controllers`. Enumerated `controllers.device.<id>` IDs are valid only for
the current connection. `controllers.players`, `controllers.detail`, and
`controllers.status` expose player names/assignments and diagnostic text.
`controllers.assign-p1`, `controllers.assign-p2`, and `controllers.reset-players`
use the same callbacks as touch/controller UI. Countdown/tester changes affect
UI revisions; use a screen guard instead of an old revision while observing
live input.

The CLI's simulated controller remains a *logical* controller, so it bypasses
hardware profiles. Simulated presses are rejected while Controllers is open;
use `ui` commands to navigate setup and physical inputs to calibrate/test it.
Logical simulation is not a substitute for testing a real held switch on
`gpio-keys-polled`.

```sh
cargo test -p engine-common controller::
cargo test -p engine-client --bin engine-client controller_
cargo test -p engine-client --bin engine-client gamepad::
SPACEWARS_CONTROLLER_ARTIFACTS=/tmp/controller-layouts \
  cargo test -p engine-client --bin engine-client \
  ui_render_tests::controller_setup_renders_on_picade_and_hyperpixel_layouts -- --exact
# With Xvfb installed:
xvfb-run -a cargo test -p engine-client --test ui_control_functional \
  controllers:: -- --ignored --test-threads=1
```

Tests cover the two observed cabinet permutations, edge/held parity, signed
axes, duplicate/diagonal input rejection, neutral/release gating, optional skips,
cancel/timeout/disconnect rollback, same-model profiles, save/reload, and default
restoration. Layouts are rendered at 800×480, 1024×768, and 480×800. Physical
cabinet/gamepad validation remains necessary before calling the mapping verified.

Assignment tests cover swaps, an unassigned replacement, saved preference reload
with changed IDs/order, reserved slots, duplicate models, ID reuse, reset, and
isolated settings recovery. Tests drive the real menu callbacks and shared
sampling gates into the NES controller inputs, verifying independent ports and
held-button suppression. Unassigned-pad menu routing is tested separately from
gameplay. The real-app UI workflow checks both launcher and pause entry/reset.

Bluetooth checks require `dbus-daemon` on Linux, but **no radio or controller**:

```sh
cargo test --locked -p engine-client --bin engine-client --profile ci bluetooth::
SPACEWARS_BLUETOOTH_ARTIFACTS=/tmp/bluetooth-layouts \
  cargo test --locked -p engine-client --bin engine-client --profile ci bluetooth_callbacks_focus
npm --prefix yocto test
npm --prefix yocto run build -- --target spacewars-bluetooth
```

The transport test starts a private D-Bus with fake BlueZ, verifying discovery,
pair/trust/connect, link-connected but HID-disconnected pairing, already-connected
handling, connection errors, cancellation, and removal without accessing the host
system bus. Panel tests verify live disconnect/reconnect status after pairing.
Worker tests use explicit events and injected scan
deadlines, not real-time waits. They cover closing with queued/pending actions,
failed discovery cleanup, stale devices, duplicate clicks, and Forget confirmation.
Rendering/callback tests cover all three cabinet display layouts and preserved
focus when discovery changes the list. Sandbox migration tests exercise first
copy, failed-copy retry, old-slot precedence, and keeping forgotten keys deleted.

Before calling the hardware flow verified, test fresh pairing using cabinet or
touch input, cancel and retry, controller power-off/on reconnect, NES D-pad/A/B,
two-player assignment, and kiosk reboot/reconnect. A subsequent A/B OS update
should retain the bond without pairing again. These physical checks are separate
from the simulated transport tests.

Hardware validation (2026-09-28): on `sw-picade-2`, the 8BitDo Micro in D mode
successfully paired through the app and became usable without a power cycle.
Controller wake/reconnect and input were also confirmed manually. The app-only
deployment preserved existing settings and bonds. The full-image persistence
service has not yet been deployed there; reboot/A/B-update persistence remains
a separate hardware check.
