# Wi-Fi setup

Choose **App Settings → Network** from either the launcher or a paused scenario.
The game stays paused; Back returns one menu level. The Picade controls, a
gamepad, touch, and a physical keyboard all use the same workflow.

1. Open Network to scan automatically, then choose a nearby network. **Rescan**
   requests fresh results; the existing list stays visible during discovery.
   SSIDs appear above smaller security/signal details, with **Connected** and
   **Saved** badges. Duplicate access points for the same SSID/security are
   grouped per adapter. Periodic updates preserve the selected network.
2. Use **Connect using saved settings**, **Connect to open network**, or
   **Enter password…**. The on-screen keyboard has lowercase, uppercase and
   punctuation, Space and Erase. D-pad moves the highlight; A types/selects,
   B goes back, Select (or the west face button) erases, and Start connects.
   A physical keyboard can type directly; Backspace erases and Enter submits.
   **Show password / Hide password** toggles visibility without losing input.
   Passwords start masked, hide on focus loss, and are cleared when submitted,
   cancelled or the panel closes. Visibility resets for each new entry.
3. Wait for association and IP configuration (up to 40 seconds).
4. Choose **Keep this network** within 30 seconds. Otherwise the previous
   network configuration is restored. A Wi-Fi connection does **not** prove
   Internet access; captive-portal login is not provided in this first pass.

The initial scope is visible infrastructure networks using open, WPA/WPA2
Personal or WPA3 Personal security. Password entry supports printable ASCII;
WPA/WPA2 accepts 8–63 characters or a 64-digit hexadecimal PSK. Unsupported
security remains visible, but cannot be activated. Hidden-SSID entry, enterprise
username/certificate authentication, WEP, captive-portal browsing, radio toggles,
and saved-profile deletion are not included yet.

## Safety and ownership

The Linux client talks directly to NetworkManager's D-Bus API, not a shell or a
new privileged helper. NetworkManager remains the authorization and credential
storage boundary. The app checks the caller's `network-control`,
`settings.modify.system`, `checkpoint-rollback` and `wifi.scan` permissions.
It does not grant them, use sudo, or try to bypass a desktop authorization prompt.
Missing permissions, service, adapter, or a blocked radio leave Back/Rescan
available. Other operating systems display an unsupported message.

The protocol targets the kiosk's NetworkManager 1.46 API:

- A checkpoint covers **only the selected Wi-Fi adapter**, not wired interfaces.
  It expires after 100 seconds independently of the app. No attempt starts if
  checkpoint creation fails; another program's checkpoint is never destroyed.
- A new profile is volatile until Keep, so an unconfirmed password is not
  written to disk. Keep persists the candidate and destroys our checkpoint.
  Saved profiles are activated without reading their secrets or editing them.
- Cancel, failure, panel close, or confirmation timeout requests rollback.
  After confirmed rollback, cleanup removes only the profile created by this
  attempt. An uncertain rollback never deletes a possibly successful connection
  whose Keep reply was lost. If the app crashes,
  NetworkManager's own checkpoint timer is the recovery mechanism. Restoring
  configuration does not guarantee the old access point is still reachable.
- If recovery cannot be confirmed, the UI says so rather than claiming success.
  The app never restarts NetworkManager, unloads Wi-Fi drivers, changes adapter
  power, or resets the shared Bluetooth/Wi-Fi hardware.
- Passwords are not included in `settings.toml`, CLI status, error messages or
  app logs, even while revealed on screen. **Photos and screenshots can include
  a revealed password**; hide it before capturing or sharing the display. Do
  not record CLI key-entry commands for real passwords: a command history could
  reconstruct typed characters.
- Profiles are saved by NetworkManager. On the kiosk, its root-owned
  `/etc/NetworkManager/system-connections` is already backed by persistent
  `/data/NetworkManager/system-connections`; ordinary A/B updates preserve it.

Only one bounded background worker runs, and only while Network is open (plus
bounded cancellation cleanup). Inventory refreshes every three seconds and
one scan is requested on opening (when permitted), with subsequent scans
explicit/rate-limited. RequestScan acknowledges a request, not scan completion;
the UI says results update automatically rather than promising a finished scan.
Discovery never presents a connection-cancellation or rollback action. There
is no gameplay/frame-loop polling, no unbounded scan thread, and no UI-thread
network calls. Rapid close/reopen cannot
overlap connection attempts. AP lists and saved-profile enumeration are capped.

API references: [connection checkpoints and activation](https://networkmanager.dev/docs/api/1.46.0/gdbus-org.freedesktop.NetworkManager.html),
[profile persistence](https://networkmanager.dev/docs/api/1.46.0/gdbus-org.freedesktop.NetworkManager.Settings.Connection.html).
DirtSim's network flow and Wi-Fi investigation informed the separate network
list/credential/connecting pages and explicit recovery. Scanner/driver-switching
features are deliberately outside this setup screen.

## Automation and tests

```sh
spacewars-cli ui activate launcher.sound
spacewars-cli ui activate settings.network
spacewars-cli ui state --json
# Read the exact network.select.* ID from that snapshot before selecting it.
# Background refreshes change revisions: guard the screen, not an old revision.
spacewars-cli ui activate network.scan --expect-screen launcher.network
spacewars-cli ui activate network.back --expect-screen launcher.network
```

Use `pause.sound` from a paused scenario. Screen IDs are `launcher.network` and
`pause.network`. `network.status` and `network.summary` are read-only; credentials
are never exposed. Character keys and Connect/Keep/Cancel go through the same
callbacks as the hardware controls. No separate privileged CLI API is added.

```sh
cargo test --profile ci -p engine-client --bin engine-client network
SPACEWARS_NETWORK_ARTIFACTS=/tmp/spacewars-network-ui \
  cargo test --profile ci -p engine-client --bin engine-client \
  network_ui_navigation_password_privacy_and_cabinet_layouts
xvfb-run -a cargo test --profile ci -p engine-client --test ui_control_functional \
  network:: -- --ignored --test-threads=1
```

Unit/worker tests cover validation, stale selection, checkpoint failure, explicit
Keep, wrong-password failure, timeout, cancel/close during both activation and
confirmation, save failure and recovery failure. Discovery tests verify an
automatic first scan, rate limiting, retaining cached results on a read failure,
and rejecting cancellation of a scan. A private `dbus-daemon` with fake
NetworkManager services verifies real D-Bus signatures,
volatile profiles, secret-free inventory, saved-profile reuse, and targeted
rollback. No test changes the host's networks. UI tests exercise masked/revealed
typing (including CLI redaction), hide-on-focus-loss, Start/Select shortcuts,
parent navigation, cleanup, and 800×480 / 1024×768 / 480×800 layouts. The
real-process workflow verifies missing-service behavior and paused
session preservation, with the host system bus isolated.

## Hardware verification

The release build was deployed to the Pi 4 cabinet `sw-picade-2` on 2026-09-28.
The user confirmed these manual checks:

- Switching between two password-protected networks and back, including saved
  connection reuse.
- The refined Network/password-entry UI looks correct on the cabinet.
- Wrong-password recovery and cancellation of a connection trial.
- Letting the Keep countdown expire restores the previous connection.
- Keeping a connection, rebooting, and reconnecting automatically.

Deployment checks also verified a healthy application/control socket, matching
client/CLI checksums, and unchanged app settings. The application-only update
did not reboot or change Wi-Fi; the subsequent recovery/reboot checks above
were performed by the user.

Open networks, WPA3-only access points, and physical HyperPixel touch entry have
not yet been hardware-verified. The portrait layout is covered by render tests;
that is not a substitute for testing a real touch device/access point.

For future recovery tests, keep a wired connection or local console available
before deliberately interrupting Wi-Fi. Do not test a remote-only network
switch without a recovery plan.
