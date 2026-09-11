# Launcher and app menu layout

The launcher, App Settings, Device Info, and Auto-start use the same
`MenuPage` frame in `crates/engine-client/ui/main.slint`. The compact target is the HyperPixel's
800×480 landscape display; 1024×768 Picade and 480×800 portrait layouts are
also covered by render tests. The page is centered and capped at 760×520,
with 48 px or larger action buttons. Larger windows add margins and roomier
settings rows rather than scaling down text on smaller displays.

## Navigation and scope

- Launcher: scenario picker, Play / optional Play New World, Scenario Settings /
  Controls / App Settings, then the separate Quit footer. Arrow/D-pad navigation
  follows these rows. Quit remains explicit; Back never quits the root menu.
- On the scenario picker, Left/Right browses and A/Enter confirms the displayed
  scenario by moving focus to Play. A second confirmation launches it; Start
  launches directly. Touch arrows continue to browse. The picker-focused hint
  explains this distinction, and confirmation does not change the scenario or seed.
- **Play** starts a fresh run using the current setup and seed; it is not Resume.
  **Play New World** starts a new Space-Wars match with a new seed. The current
  world seed is shown in Scenario Settings rather than on the main launcher.
- Friendly scenario names are presentation only. Registry IDs, saved settings,
  CLI arguments, and control IDs remain unchanged. Existing control indices are
  retained, with their directional neighbors in `ui_navigation.rs` updated to
  match the layout.
- App Settings and Device Info are opaque pages, shared by launcher and pause.
  Back returns one level without resuming a paused game. Existing Start/resume
  behavior is unchanged. Scenario-specific settings and controls retain their
  existing content/layout in this pass.

## Settings content and future additions

App Settings uses label/value rows: volume adjustment with a level indicator,
On/Off toggles, and chevrons for subpages. Its body scrolls by touch or mouse
wheel; controller/keyboard selection reveals the focused row when needed.
Auto-start is the fifth row, below Device Info; navigating to it reveals the
row. Its child page shares the same frame and fixed actions.
Back, save status, and Retry Save stay outside the scroll area. A save failure
does not prevent adjustment, navigation, or retrying the same settings.

Future Network and Controllers pages belong beside Device Info, not in the
scenario picker. When adding a row, update its focus mapping, body extent,
reveal range, and stable control inventory together. Do not add non-working
placeholder entries. Network permissions and credential entry remain separate
work; see [Device Info and app navigation](device-info.md).

## Verification

`ui_render_tests` exercises partial and full repaints through menu changes at
all three display sizes, including long names, Space-Wars, and save errors.
Set `SPACEWARS_MENU_TEST_ARTIFACTS` to export PNGs for visual inspection.
Keyboard/touch tests also force App Settings into a shorter window to verify
focus reveal, the Auto-start child page, and the fixed Back action. The
real-process functional suite checks launcher navigation, settings
persistence/retry, and Info return paths.
Device screenshots complement these checks; a rendered screenshot alone does
not verify the physical touchscreen or controller wiring.
