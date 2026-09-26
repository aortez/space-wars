# Controller profiles

Open **App Settings → Controllers** from the launcher or pause menu. Select a
device, or choose **Identify by pressing a button** and press one of its buttons.
The device's current P1/P2 assignment is shown; this does not change its seat.
A paused game stays paused throughout setup.

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
scenario-specific Jump/Run bindings, and P1/P2 reassignment are separate work.

## Automation and checks

```sh
spacewars-cli ui activate launcher.sound
spacewars-cli ui activate settings.controllers
spacewars-cli ui state --json
spacewars-cli ui activate controllers.back
```

Use `pause.sound` from a paused game. Screen IDs are `launcher.controllers` and
`pause.controllers`. Enumerated `controllers.device.<id>` IDs are valid only for
the current connection. `controllers.detail` and `controllers.status` expose the
visible diagnostic text. Countdown/tester changes affect UI revisions; use a
screen guard instead of an old revision while observing live input.

The CLI's simulated controller remains a *logical* controller, so it bypasses
hardware profiles. Simulated presses are rejected while Controllers is open;
use `ui` commands to navigate setup and physical inputs to calibrate/test it.
Logical simulation is not a substitute for testing a real held switch on
`gpio-keys-polled`.

```sh
cargo test -p engine-common controller::
cargo test -p engine-client --bin engine-client controller_
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
