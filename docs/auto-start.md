# Automatic activities

Open **App Settings → Auto-start** from the launcher or a paused scenario.
Choose **Off**, **Clock**, or **Spacewars bots**, then choose the idle delay.
The default is Off with a 30-second delay. Controller stops are 5, 10, 30,
60, 120, 300, and 600 seconds. **Start now** previews an enabled activity
from the launcher after its settings have saved.

The preference stays enabled across client/device restarts until switched Off.
Only the root launcher counts as idle. Input resets its countdown; held
controls prevent expiry. Settings, Info, Controls, paused sessions, and manual
games suspend it. Returning to the root launcher starts the full delay again.

Clock uses the saved Clock configuration. Spacewars bots starts the ordinary
destructible match with two rule bots and a fresh world. Each result remains
visible for eight seconds before another fresh world starts. The match's own
time-limit rule applies; automatic play adds no separate deadline. Unlimited
matches can therefore continue indefinitely until a pilot dies.

Press a key/gamepad button or touch a running automatic activity to return to
the launcher. That input is consumed before the menu is exposed. The idle
countdown then starts again. A session deliberately paused through the host
API uses the normal pause menu: Resume continues automatic play; returning to
the launcher ends it. Controller disconnection does not pause unattended
activities, while manual play retains its disconnection protection.

Automatic controllers, world seeds, and scenario choices are temporary. They
do not overwrite saved human-player choices, manual launch defaults, or audio.
Read-only status queries and screenshots do not reset the countdown.

Explicit direct launches (`--scenario`, `--rom`, `--kiosk`), benchmarks, render
probes, and touch diagnostics suppress idle auto-start for that process.
Launch/restart errors suppress further automatic attempts until an explicit
Start now/retry, an auto-start settings change, or a client restart. Off and
unavailable activities never launch silently.

## Saved settings and automation

```toml
[autostart]
enabled = true
activity = "clock" # or "spacewars-bots"
delay_seconds = 30

[spacewars_match]
time_limit_seconds = 600 # 0 means Unlimited
```

The file accepts idle delays from 5–600 seconds and finite match lengths from
1–3600 seconds. The UI offers common minute choices for matches. Old files
receive defaults; invalid timer fields fall back individually. Unknown
activity IDs are retained and shown as unavailable.

Use the existing structured UI API:

```sh
spacewars-cli ui activate launcher.sound
spacewars-cli ui activate settings.autostart
spacewars-cli ui state --json
spacewars-cli ui activate autostart.activity.next
spacewars-cli ui activate autostart.delay.previous
spacewars-cli ui activate autostart.back
spacewars-cli ui activate sound.back
spacewars-cli status
```

Screens are `launcher.autostart` and `pause.autostart`. Read
`autostart.save-status` before sending another guarded edit or starting a
preview: saving can change control availability and therefore the UI revision.
Countdown display updates alone do not change actionable UI revisions.

`status` reports the activity, phase, session kind, delay, and repeat count.
Spacewars diagnostics additionally report the effective controllers, remaining
gameplay seconds, ownership counts, and finish reason/result.

The [design and extension plan](design/auto-start-activities.md) describes the
small activity catalog and future screen, simulation, and training adapters.
