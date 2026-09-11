# Automatic activities

Status: first implementation desktop-validated, 2026-09-11, on
`auto-start-activities`. Includes merged main `4f8894e` and the committed
Device Info change `2e6791f`. See [the user guide](../auto-start.md) for current
controls and saved settings.

## Goal and first slice

Let each cabinet choose what to run unattended after a configurable delay:
Clock or repeated two-bot matches in the ordinary destructible `spacewars`
scenario. Keep a small common lifecycle so future targets can include Device
Info, a gardening simulation, or a training activity.

The confirmed trigger is **launcher inactivity**, including after device
startup. Auto-start is sticky: once enabled, the saved activity and delay
remain in effect across launcher visits and device restarts until the user
turns it off. Each eligible launcher visit starts a full countdown; deliberate
interaction resets it. The countdown starts when the launcher is ready,
not during OS boot or asset loading. Default to disabled on existing and new
installations until explicitly enabled.

First implementation:

- App Settings has an **Auto-start** child screen next to Device Info.
- **Activity:** Off / Clock / Spacewars bots.
- **Start after:** configurable idle delay; 30 seconds by default,
  accepting 5–600 seconds, with controller stops of
  5, 10, 30, 60, 120, 300, and 600 seconds.
- **Start now:** previews the configured automatic activity using the same
  launch path as timeout expiry.
- The root launcher shows, for example, “Clock starts in 24 seconds.”
- Clock runs continuously with its saved Clock configuration.
- Spacewars starts a fresh world with both seats using the existing rule bot.
  After a finished round, show its actual result for 8 seconds, then start
  another fresh world. It uses the ordinary match's time-limit setting and
  completion result. The 8-second result display is activity policy; the
  match duration belongs to Spacewars rules and settings.

## Match time limit

The time limit belongs to the match itself, as requested by the user. The
implementation extends `MatchRound` in
[`match_rules.rs`](../../scenarios/spacewars/src/surface_sortie/match_rules.rs)
with duration, elapsed time, and a finish reason. Before this change, two living
pilots could continue indefinitely. Three-minute simulation test budgets are
still distinct from gameplay deadlines.

**Match length** is a Spacewars scenario setting shared by human-versus-human,
human-versus-bot, and bot-versus-bot matches. The UI offers 5, 10, or 15 minutes
and Unlimited, defaulting to 10 minutes. The saved seconds field also supports
short controlled fixtures without adding a demo-only deadline.

The scenario owns configured duration, elapsed/remaining gameplay time, and
the final result. Advance its clock using simulation time, freeze it when the
match is paused or finished, and reset it for both rematches and fresh worlds.
Show the remaining time in the HUD and expose it in match observations. This
keeps the rule deterministic in headless tests and consistent with the rest
of the simulation; ten gameplay minutes can take longer on a device whose
simulation is running slowly.

At the deadline, process that step's damage before evaluating expiry so a
pilot death on the final step retains the existing elimination result.
The confirmed expiry rule with both pilots alive is **most planets owned
wins; equal ownership draws**. Count current surviving claims at the final
step, including flags destroyed or neutralized during that step. Neutral
planets score for neither player; owning zero planets each is a draw. Do not
add other hidden tie-breakers or count past ownership.

Record a finish reason independently of winner/draw so HUD, results, and
diagnostics can distinguish pilot death, simultaneous deaths, and time expiry.
Both the client result message and scenario-rendered result currently describe
pilot deaths, so update both to use the actual reason. An expired match freezes
through the existing match-completion path; its pilots need not be marked dead.

The auto-start controller has no separate match deadline. It observes ordinary
completion, displays the result, and requests a fresh world. It preserves the
saved match length along with the other Spacewars preferences. Unlimited also
means an unattended round can continue until elimination; there is no hidden
demo cutoff. Historical labs/endurance fixtures without match rules remain
unlimited unless they explicitly opt into a timed match.

A finite match length also bounds stalemates such as the long landing/search
stalls in [the investigation guide](../landing-investigation-guide.md) and
[fresh-world survey](../fresh-world-survey.md). It is a gameplay rule for every
match, independent of automatic repetition.

## When the device yields to a person

Only the idle root launcher can start an activity automatically. Settings,
Controls, Device Info, loading, save errors, paused sessions, match results
from manual play, and running manual scenarios are ineligible. Opening a
submenu or choosing manual play suspends auto-start without disabling the
saved preference. Returning to the root launcher starts a full new delay;
time spent elsewhere does not count toward it.

Keyboard/gamepad presses and deliberate pointer interaction reset the launcher
countdown. Held controls keep the launcher active; the full idle delay starts
after they are released. Capture interaction during initialization so held
input before the launcher is ready cannot be forgotten. Ignore neutral
controller polling, small stick drift, connection announcements, animation,
and telemetry. Use the existing controller thresholds and handoff machinery
where applicable.

During an automatically started Clock or bot demo, an intentional input exits
to the launcher and begins a fresh idle delay. Display a short “Press a button
to return to the menu” hint. Consume that input and clear held actions before exposing
the menu, so the same press cannot also start a game. Human takeover of a bot's
ship is separate future work.

Remote read-only inspection, including screenshots and status, does not count
as activity. Mutating UI controls count as operator interaction. Host pause
suspends automatic transitions and timing. A deliberately paused automatic
session uses the ordinary pause menus; Resume continues the automatic session. Returning
to the launcher ends it. These rules also make remote testing predictable.

Automatic activities do not require a connected gamepad. The existing
disconnect-to-pause protection must continue for manual play but must not
strand an unattended Clock or two-bot session when a controller disconnects.

Keep launcher idle eligibility separate from the running activity's lifecycle.
Once a bot session starts, normal match completion continues its repeat loop
without visiting the launcher. Interrupting the activity ends that loop while
leaving the saved auto-start preference enabled. Changing Auto-start preferences
applies when the user returns to the launcher, with a full delay. **Start now**
remains an explicit way to preview the chosen activity immediately. Choosing
Off disables future automatic starts and repeats until enabled again.

A kiosk process restart reads the same saved preference and starts a fresh
launcher countdown. No per-boot consumption marker or startup-only gate is
needed. Both Pi and desktop use the same idle controller.

## Reuse and required boundaries

The current app already provides:

- Safely persisted settings in `engine-common::Settings`, with background
  writes and visible retry status in `settings_writer.rs` and
  `sound_controls.rs`.
- A single asynchronous launcher in `launcher.rs`, which prepares assets and
  starts scenarios through the existing host.
- `ScenarioControlRequest::NewMatch` in `host.rs`, which replaces a Spacewars
  world, clears held input, and selects a fresh seed.
- Host game-over state and scenario revisions, plus public UI/host control
  APIs and real-process functional tests.
- An existing Pi service command that shows the launcher. No service restart
  loop, OS scheduling change, or additional simulation process is needed.

Two existing persistence paths need explicit treatment. Manual launcher
starts commit their selections; `on_match_world_changed` also saves the new
seed and launch scenario. Automatic launches and repeats must bypass those
manual preference writes. `handle_return_to_launcher` currently also carries
the active Spacewars seed back to the launcher, so automatic exit must restore
the manual selection instead.

The client distinguishes automatic sessions by activity ID and suppresses idle
launches for explicit CLI sessions. It keeps canonical saved settings separate
from effective session settings. An automatic bot match clones the normal configuration,
overrides only its two controllers and world seed, and passes that snapshot
to the existing scenario host. Its repeats retain those effective overrides.
Audio, renderer, Clock configuration, match length, bot breaks, and asteroid
settings still come from the user's preferences. Normal audio/settings edits continue to
save from the canonical settings object, never from the demo snapshot.

Do not emulate button presses against the mutable launcher to start a demo.
Factor a launch request/preparation boundary shared by manual and automatic
starts, with an explicit persistence policy. Preserve the existing busy/error
handling and only allow one start or replacement in flight.

Explicit CLI launches (`--scenario`, `--rom`, `--kiosk`), benchmarks, render
probes, and touch diagnostics suppress saved auto-start for that process.
Their existing one-shot behavior takes precedence, including after returning
to the launcher. Ordinary fullscreen launcher startup can honor auto-start.

## Small activity catalog

An activity is a named unattended use of the app. A scenario is one possible
way to implement it. Keep a compile-time catalog with stable IDs and labels,
an availability check, a start/stop adapter, and a completion policy:

| Activity ID | Implementation | Completion behavior |
| --- | --- | --- |
| `clock` | Existing Clock scenario and saved configuration | Continuous |
| `spacewars-bots` | Existing `spacewars`, fresh seed, two rule bots | Ordinary match completion, result delay, then fresh world |
| Future `device-info` | Existing information screen | Continuous; stop its sampler on exit |
| Future gardening activity | Its own scenario/session adapter | Decide resume versus fresh world when added |
| Future training activity | Managed training job plus a progress view | Explicit job/checkpoint/stop semantics |

Only implemented and available activities appear in the selector. This does
not require a plugin system, arbitrary shell commands, or registering every
lab/benchmark as a supported unattended activity. Do not refactor the entire
scenario registry. The small activity catalog delegates to it where useful.

The Device Info branch already gives App Settings a child screen and controls
its worker through visibility. Reuse that screen later, adding an automatic
entry/exit context so Back has a meaningful destination. It need not become
a fake physics scenario. The committed Device Info navigation was integrated before the overlapping
menu edits; the other checkout remains untouched.

Future training needs a different stop contract: returning to the launcher
may detach a view or request a checkpoint rather than destroy a job. Define
that contract when a training activity exists. A small extensible catalog now
is sufficient; a general task scheduler is not part of this slice.

Saved shape:

```toml
[autostart]
enabled = false
activity = "clock"
delay_seconds = 30
```

Off toggles `enabled` while preserving the last activity and delay. Store a
stable activity ID, not a menu index or command line. Unknown/unavailable IDs
remain readable, display an explanation, and suppress automatic launch rather
than invalidating the whole settings file. Invalid timer fields normalize individually without discarding unrelated preferences.

## Timing, errors, and observability

Use one client-side controller with injected monotonic time for unit tests.
Keep it outside the physics timestep; low rendering rates must not turn a
30-second launcher delay into minutes. This controller owns only the launcher
countdown and post-result delay. The scenario owns the match clock, measured
in gameplay time as described above. Continuous Clock needs no periodic reset.

The controller distinguishes disabled/ineligible, countdown, launching,
running, result delay, and failed states. A start or replacement is requested
once; its completion is acknowledged before another transition can occur.
Tie delayed actions to the automatic session identity and scenario revision.
If the user exits or starts something else, stale timer callbacks do nothing.

Launch/restart failures stop automatic transitions and leave a visible error
with a usable route to the launcher. A failed activity does not retry forever;
an explicit retry/Start now, an auto-start configuration change, or a new
client process can clear failure suppression. Merely waiting or revisiting
the launcher does not retry the failed activity. This transient failure state
does not turn off the saved preference. Manual launches remain available.

The existing status API reports automatic activity ID, phase, session origin,
repeat count, and configured delay. The visible caption shows the countdown.
World seed and match finish reason are available through the host/result
inventory and match diagnostics. A structured countdown and stop-reason history
can be added when additional activity types need them.
Add stable App Settings/Auto-start control IDs. Read-only countdown ticks
should not invalidate action revision guards every second; use a telemetry
field or otherwise distinguish changing display values from actionable state.
This policy supplements existing strict screen/revision guards.

## Implementation and verification order

1. Add the shared match time-limit setting and scenario rule first. Test exact
   expiry boundaries, zero-duration steps, pause/resume, reset, Unlimited,
   final-step elimination precedence, ownership-based expiry results, and frozen
   post-match state. Verify remaining time and finish reason in the HUD,
   observations, and result screen. Exercise human/bot seat combinations and
   keep untimed lab fixtures unchanged. Cover each player leading, tied
   ownership (including zero each), neutral planets, and ownership changes
   from capture or flag destruction on the final step.
2. Add saved auto-start preferences, validation, the initial activity catalog,
   and a deterministic controller. Test deadline boundaries, idle reset/suspension,
   input-versus-timeout precedence, stale sessions, failure suppression, and
   direct-launch precedence using virtual time. Cover full-delay rearming
   after submenus, manual games, interrupted demos, settings changes, and kiosk
   process restarts. Verify that disabling persists and prevents later starts.
3. Add the launch request/session-origin boundary. Verify that automatic
   controllers, seeds, last scenario, and manual launcher selections never
   leak into saved preferences, including on repeat, exit, and save failure.
4. Add the Auto-start screen and countdown through the existing keyboard,
   gamepad, touch, and structured UI paths. Integrate Device Info navigation
   first if available; preserve existing stable App Settings IDs.
5. Wire Clock, then bot matches, to the existing host. Verify that both pilot
   elimination and time-limit results lead to one fresh-world replacement. Use
   controlled host fixtures to force results/errors; do not require bots to
   win a randomly generated match before a UI test can pass.
6. Run focused real-process workflows with isolated settings: settings survive
   a new client process; Clock starts; input returns to the launcher without
   firing a second action; bot seats are effective only for demos; multiple
   fresh worlds replace cleanly; pause and read-only inspection behave as
   specified; returning to the launcher and leaving it idle starts the saved
   activity after a full delay. Check each new scenario revision and clean input/resource teardown,
   not only screenshots.
7. Run the relevant workspace and UI regression checks, then deploy to
   `sw-picade`. Verify boot countdown, both physical controllers, entering
   settings during countdown, manual play, Clock, and repeated bot rounds.
   Observe at least three real unattended bot rounds, including a controlled
   match-expiry path in tests. Capture settings and resulting runtime status.

This delivers unattended Clock and bot matches with clear human control. Add
Device Info as the next activity to verify the catalog works for a screen;
consider rotation/playlists and time-of-day schedules only when requested.

## Validation record — 2026-09-11

- `cargo test --locked --workspace --all-targets` passed. Display-dependent
  workflows remain explicitly ignored in that command. Subsequent focused
  checks passed 289 client unit tests (one unrelated ignored test), 19 match
  rule tests, and the physical final-step flag-destruction expiry test.
- Two real-client auto-start workflows passed under a private Xvfb display.
  Clock stayed in settings beyond the idle delay, launched from the idle
  launcher, resumed from a host pause, survived a client restart, and stayed
  disabled after Off. Three short bot matches completed on the ordinary match
  timer and were replaced with fresh seeds/revisions. The host reported both
  effective bot controllers while saved human settings remained unchanged.
- Five existing UI workflows passed: launcher navigation, Device Info, sound
  persistence, all manual controller choices with both renderers, and fresh
  worlds/rematches/process restart. Backend-neutral Slint input checks verify
  that keyboard repeats and touch release do not activate the newly exposed
  launcher; focus-loss cleanup prevents a lost release blocking idle forever.
- Default-duration endurance and physical cabinet-button testing are separate
  playtest follow-ups. The short functional fixtures exercise the actual match
  rule and host lifecycle without waiting ten minutes for each result.

The numbered verification plan above also lists useful future fault-injection
cases. It is not a claim that every error, stale callback, or direct-launch
combination has a dedicated end-to-end test.
