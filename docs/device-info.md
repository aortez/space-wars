# Device Info and app navigation

Both **Launcher → App Settings → Device Info** and
**Pause → App Settings → Device Info** open the same read-only screen.

- D-pad/arrow keys scroll. Touch-drag, mouse wheel, and visible Up/Down buttons
  work too. Back stays on screen even when the content is scrolled.
- A/Enter activates Back. B/Esc returns exactly one level to App Settings and
  restores the Device Info selection. Back again returns to the original menu.
- A paused scenario remains paused. Start is the explicit quick-resume shortcut
  from paused menus; Start in launcher Info only returns to App Settings.
- Opening Info does not save settings, restart a scenario, scan Wi-Fi, contact
  the internet, or read network credentials.

## Information and cost

The Linux screen reports hostname, the compiled app version/revision and
debug/release profile, OS/architecture, hardware model, active interface
addresses, CPU model/core count, aggregate CPU busy percentage, available/total
RAM, thermal-zone-0 temperature, available/total persistent storage, configuration
directory, connected gamepads with their current player assignments, and scenario
state. Controllers without a free player seat are labelled Unassigned. This
does not change assignment policy; keyboard bindings remain available.

CPU busy is a delta across **all cores**, normalized to 0–100%, not the app's
per-core process percentage. The first sample says Sampling. Memory uses
`MemAvailable`, including reclaimable memory, rather than just `MemFree`.
Disk availability excludes blocks reserved from ordinary users. Pi installations
report `/data/spacewars`; desktops report the config directory's filesystem.
Missing metrics say Unavailable. Non-Linux builds retain app/controller information
but explicitly lack the Linux system metrics.

Addresses are from active local interfaces, including IPv6 and virtual adapters.
Having an address is **not** proof of internet connectivity. No SSID/password
or saved Wi-Fi connection profiles are read. Gameplay is paused in this menu,
so frame/update rates are labelled paused; use **FPS Counter** during gameplay
or `spacewars-cli status` for live rates.

A single worker samples every two seconds while Info is visible. The UI never
waits on filesystem/network-interface queries. Closing the screen stops its UI
timer and subsequent requests; at most one already-running read may finish.
Reopening rejects old responses and starts a fresh CPU baseline. The worker
sleeps on a channel when not requested. Hidden Info has no instantiated item tree.

The build revision comes from the **build-time** Git checkout (with `-dirty`
when tracked files differ), not an installed machine's checkout. Archive builds
can set `SPACEWARS_BUILD_REVISION`; otherwise the revision is explicitly unknown.

## Automation

No separate diagnostics transport or privileged service is required. The
screen uses the existing structured control API:

```sh
spacewars-cli ui activate launcher.sound
spacewars-cli ui activate settings.device-info
spacewars-cli ui state --json
spacewars-cli ui press down --expect-screen launcher.info
spacewars-cli ui activate info.back
spacewars-cli ui activate sound.back
```

Use `pause.sound` from the pause menu. Screen IDs are `launcher.info` and
`pause.info`. `info.status` is `loading` until the first sample is ready, then
`ready`. Read-only `info.*` controls contain the same values as the display;
`info.scroll-up`, `info.scroll-down`, and `info.back` are the only activatable
Info controls. `info.scroll-offset` is zero at the top and negative when scrolled.
Metrics update UI revisions, so navigation can guard the **screen** without
guarding a transient sample revision. Existing strict revision guards continue
to reject stale data as usual.

## Menu boundaries for future work

The launcher chooses/configures scenarios. Pause handles the active session.
**App Settings** is the shared home for application/device-wide concerns:
current audio/FPS preferences and Device Info, with **Network** and **Controllers**
as future sibling screens. Scenario-specific controls do not go here.

Wi-Fi setup belongs in Network, not inside read-only Info. Its implementation
will need network selection, credential entry usable without a physical keyboard,
clear connecting/success/failure feedback, and a way back if the new connection
fails. Network mutations and credential storage need an explicit privileged
boundary on kiosks; this Info implementation does not grant one. Do not add
non-working Network buttons until that workflow exists.

Tests cover resource parsers, sampling throttling/stale visits, parent navigation,
paused-scenario preservation, scrolling, and partial versus full UI repaints at
landscape and portrait sizes. The real-process workflow is `device_info` in
the [functional suite](functional-tests.md).
