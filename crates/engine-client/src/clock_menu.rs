//! The Clock's settings as pages of rows: the top page and the two it opens,
//! Look and Events. Two menus show them. Clock Controls, in the pause menu,
//! can also play each event from its row. The Clock's Scenario Settings, in
//! the launcher, also choose the renderer on their top page. The rows come
//! from the settings table, so a setting added there appears on its page in
//! both menus, with nothing to change here.
//!
//! The window holds which page shows and the focused row, and the rows' models
//! hold the settings they show. Keyboard, gamepad, touch and control-ID
//! activation all move between pages through here.

use std::any::Any;

use engine_common::{ClockEventKind, ClockSettings};
use slint::{Model, ModelRc, ModelTracker, VecModel};
use spacewars_control::{UiAction, UiControl, UiScreen};

use crate::clock_settings_table::{self, ClockSetting, Section};
use crate::{ClockMenuPages, MainWindow, SettingsListRow, SettingsRowKind};

/// A menu that shows the pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Menu {
    /// Clock Controls, in the pause menu over the running Clock.
    Pause,
    /// The Clock's Scenario Settings, in the launcher.
    Launcher,
}

impl Menu {
    /// The menu showing the pages, if one is.
    pub(crate) fn showing(window: &MainWindow) -> Option<Self> {
        if window.get_launcher_visible() {
            (window.get_launcher_settings_visible() && window.get_launcher_scenario() == "clock")
                .then_some(Self::Launcher)
        } else {
            window.get_ingame_clock_visible().then_some(Self::Pause)
        }
    }

    /// The focused row on the page showing, or Back after the last row.
    pub(crate) fn focus(self, window: &MainWindow) -> i32 {
        match self {
            Self::Pause => window.get_ingame_clock_focus_index(),
            Self::Launcher => window.get_launcher_settings_focus_index(),
        }
    }

    pub(crate) fn set_focus(self, window: &MainWindow, index: i32) {
        match self {
            Self::Pause => window.set_ingame_clock_focus_index(index),
            Self::Launcher => window.set_launcher_settings_focus_index(index),
        }
    }

    /// The control API's name for the row named `id`, or for Back. In the
    /// launcher, Renderer, Raster Scale and Back keep the names they have in
    /// every scenario's settings.
    fn control(self, id: &str) -> String {
        match (self, id) {
            (Self::Pause, _) => format!("pause.clock.{id}"),
            (Self::Launcher, "renderer" | "raster-scale" | "back") => {
                format!("launcher.settings.{id}")
            }
            (Self::Launcher, _) => format!("launcher.settings.clock.{id}"),
        }
    }

    /// The menu's pages, as the window holds them.
    fn pages(self, window: &MainWindow) -> ClockMenuPages {
        match self {
            Self::Pause => window.get_clock_menu_pause(),
            Self::Launcher => window.get_clock_menu_launcher(),
        }
    }
}

/// A Clock settings page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Top,
    Look,
    Events,
}

impl Page {
    /// The window's name for the page, in `clock-menu-page`.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Look => "look",
            Self::Events => "events",
        }
    }

    /// The page the window names, or the top page for a name it doesn't know.
    pub(crate) fn named(name: &str) -> Self {
        match name {
            "look" => Self::Look,
            "events" => Self::Events,
            _ => Self::Top,
        }
    }

    /// The screen the control API reports while `menu` shows the page.
    pub(crate) const fn screen(self, menu: Menu) -> UiScreen {
        match (menu, self) {
            (Menu::Pause, Self::Top) => UiScreen::PauseClock,
            (Menu::Pause, Self::Look) => UiScreen::PauseClockLook,
            (Menu::Pause, Self::Events) => UiScreen::PauseClockEvents,
            (Menu::Launcher, Self::Top) => UiScreen::LauncherSettings,
            (Menu::Launcher, Self::Look) => UiScreen::LauncherClockLook,
            (Menu::Launcher, Self::Events) => UiScreen::LauncherClockEvents,
        }
    }

    /// The page that opens this one, and its row that does.
    const fn parent(self) -> Option<(Self, &'static str)> {
        match self {
            Self::Top => None,
            Self::Look => Some((Self::Top, "look")),
            Self::Events => Some((Self::Top, "events")),
        }
    }
}

/// Shows every page's rows for `settings` in both menus, and keeps the
/// settings with the rows for [`settings`] to read back. The launcher's top
/// page takes its renderer rows from the window, and its Clock card a line
/// about the time and the events.
pub(crate) fn publish(window: &MainWindow, settings: &ClockSettings) {
    let model = |rows| -> ModelRc<SettingsListRow> {
        ModelRc::new(PageRows {
            settings: *settings,
            rows: VecModel::from(rows),
        })
    };
    let pages = |menu| ClockMenuPages {
        top: model(rows(menu, Page::Top, settings)),
        look: model(rows(menu, Page::Look, settings)),
        events: model(rows(menu, Page::Events, settings)),
    };
    window.set_clock_menu_pause(pages(Menu::Pause));
    let mut top = rows(Menu::Launcher, Page::Top, settings);
    top.extend(renderer_rows(window));
    window.set_clock_menu_launcher(ClockMenuPages {
        top: model(top),
        ..pages(Menu::Launcher)
    });
    window.set_clock_menu_summary(
        format!(
            "{}  ·  local time  ·  {} events",
            value("time-format", settings),
            value("event-profile", settings)
        )
        .into(),
    );
}

/// The settings the pages show, as last published.
pub(crate) fn settings(window: &MainWindow) -> Option<ClockSettings> {
    window
        .get_clock_menu_pause()
        .top
        .as_any()
        .downcast_ref::<PageRows>()
        .map(|rows| rows.settings)
}

/// The Clock's rows on `page` in `menu`, each named by its control's ID after
/// the menu's prefix: `pause.clock.` or `launcher.settings.clock.`.
pub(crate) fn rows(menu: Menu, page: Page, settings: &ClockSettings) -> Vec<SettingsListRow> {
    let mut rows = Rows {
        playable: menu == Menu::Pause,
        ..Rows::default()
    };
    match page {
        Page::Top => {
            rows.page(
                "look",
                "Look",
                &look_summary(settings),
                "Time, date, theme, lamps and fonts.",
            );
            rows.page(
                "events",
                "Events",
                &events_summary(settings),
                match menu {
                    Menu::Pause => "How often events come, which ones, and Play to see one now.",
                    Menu::Launcher => "How often events come, and which ones.",
                },
            );
        }
        Page::Look => {
            for setting in clock_settings_table::SETTINGS {
                if setting.about().section == Section::Look {
                    rows.setting(setting, settings);
                }
            }
            rows.page(
                "fonts",
                "Fonts",
                &fonts_summary(settings),
                "The digits' font, or a different one each minute.",
            );
        }
        Page::Events => {
            let mut section = None;
            for setting in clock_settings_table::SETTINGS {
                let about = setting.about();
                if about.section == Section::Look {
                    continue;
                }
                if section != Some(about.section) {
                    section = Some(about.section);
                    if let Some(heading) = about.section.heading() {
                        rows.heading(heading);
                    }
                }
                rows.setting(setting, settings);
            }
        }
    }
    rows.rows
}

/// The event a setting switches, which its row can play.
pub(crate) fn event(id: &str) -> Option<ClockEventKind> {
    ClockEventKind::ALL
        .into_iter()
        .find(|kind| kind.as_str() == id)
}

/// The page showing.
pub(crate) fn page(window: &MainWindow) -> Page {
    Page::named(&window.get_clock_menu_page())
}

/// The rows of the page showing in `menu`.
pub(crate) fn page_rows(window: &MainWindow, menu: Menu) -> ModelRc<SettingsListRow> {
    let pages = menu.pages(window);
    match page(window) {
        Page::Top => pages.top,
        Page::Look => pages.look,
        Page::Events => pages.events,
    }
}

/// The row at `index` on the page showing in `menu`, or `None` for Back,
/// which follows the rows.
pub(crate) fn row(window: &MainWindow, menu: Menu, index: i32) -> Option<SettingsListRow> {
    let index = usize::try_from(index).ok()?;
    page_rows(window, menu).row_data(index)
}

/// Shows `page` in `menu`, focused on its row named `focus`, or on its first
/// row.
pub(crate) fn show(window: &MainWindow, menu: Menu, page: Page, focus: Option<&str>) {
    window.set_clock_menu_page(page.name().into());
    let index = focus
        .and_then(|id| page_rows(window, menu).iter().position(|row| row.id == id))
        .unwrap_or(0);
    menu.set_focus(window, index as i32);
}

/// Starts `menu` over on the top page. Clock Controls start over for each
/// launch of the Clock, and until the next one, Restart included, they open
/// where they were left. Scenario Settings start over each time the launcher
/// shows, or its scenario changes.
pub(crate) fn reset(window: &MainWindow, menu: Menu) {
    show(window, menu, Page::Top, None);
}

/// Opens the page that the row at `index` names, if it names one.
pub(crate) fn open_row(window: &MainWindow, menu: Menu, index: i32) {
    let Some(row) = row(window, menu, index) else {
        return;
    };
    match row.id.as_str() {
        "look" => show(window, menu, Page::Look, None),
        "events" => show(window, menu, Page::Events, None),
        "fonts" => window.invoke_clock_fonts_open(),
        _ => {}
    }
}

/// Back: to the page that opens this one, on the row that opens it, or from
/// the top page out of the menu, which next opens on its first row.
pub(crate) fn back(window: &MainWindow, menu: Menu) {
    match page(window).parent() {
        Some((parent, row)) => show(window, menu, parent, Some(row)),
        None => {
            reset(window, menu);
            match menu {
                Menu::Pause => window.set_ingame_clock_visible(false),
                Menu::Launcher => window.set_launcher_settings_visible(false),
            }
        }
    }
}

/// Where Up or Down takes the focus from `focus`: the next row that way that
/// isn't a heading, with Back after the last row, and around from either end.
pub(crate) fn moved_focus(rows: &[SettingsListRow], focus: i32, action: UiAction) -> i32 {
    let stops = (0..=rows.len())
        .filter(|&index| {
            rows.get(index)
                .is_none_or(|row| row.kind != SettingsRowKind::Heading)
        })
        .collect::<Vec<_>>();
    let at = stops
        .iter()
        .position(|&stop| stop as i32 == focus)
        .unwrap_or(0);
    let next = match action {
        UiAction::Up => at + stops.len() - 1,
        UiAction::Down => at + 1,
        _ => at,
    };
    stops[next % stops.len()] as i32
}

/// The controls on the page showing in `menu`, as the control API lists them,
/// each with its row and the action that presses it there: a switch by its
/// name, a choice by its ‹ and ›, a page by its name, Play, and Back.
pub(crate) fn targets(window: &MainWindow, menu: Menu) -> Vec<(UiControl, i32, UiAction)> {
    let rows = page_rows(window, menu);
    let mut targets = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let index = index as i32;
        let id = menu.control(&row.id);
        let named = |label: &str| UiControl::new(id.as_str(), label, row.enabled);
        match row.kind {
            SettingsRowKind::Heading => continue,
            SettingsRowKind::Choice => {
                for (end, label, action) in [
                    ("previous", "‹", UiAction::Left),
                    ("next", "›", UiAction::Right),
                ] {
                    let control = UiControl::new(format!("{id}.{end}"), label, row.enabled);
                    targets.push((control.with_value(row.value.as_str()), index, action));
                }
            }
            SettingsRowKind::Switch => targets.push((
                named(&row.label).with_value(row.value.as_str()),
                index,
                UiAction::Right,
            )),
            SettingsRowKind::Page => targets.push((
                named(&row.label).with_value(row.value.as_str()),
                index,
                UiAction::Confirm,
            )),
        }
        if row.playable {
            let control = UiControl::new(format!("{id}.play"), "Play", row.enabled);
            targets.push((control, index, UiAction::Confirm));
        }
    }
    targets.push((
        UiControl::new(menu.control("back"), "Back", true),
        rows.row_count() as i32,
        UiAction::Confirm,
    ));
    targets
}

/// The focused row in `menu`, as the control API names it.
pub(crate) fn selected(window: &MainWindow, menu: Menu) -> String {
    row(window, menu, menu.focus(window))
        .map_or_else(|| menu.control("back"), |row| menu.control(&row.id))
}

/// The label of the setting's current value.
fn value(id: &str, settings: &ClockSettings) -> &'static str {
    let setting = clock_settings_table::find(id).expect("a setting in the table");
    setting.labels()[setting.index(settings)]
}

/// What the Look page holds, as its row on the top page shows it.
fn look_summary(settings: &ClockSettings) -> String {
    format!(
        "{} · Date {} · {}",
        value("time-format", settings),
        value("show-date", settings),
        value("theme", settings)
    )
}

/// How often events come, and how many of them are on.
fn events_summary(settings: &ClockSettings) -> String {
    let on = ClockEventKind::ALL
        .iter()
        .filter(|kind| settings.events.enabled(**kind))
        .count();
    format!(
        "{} · {on} of {} on",
        value("event-profile", settings),
        ClockEventKind::ALL.len()
    )
}

/// The chosen font, and whether it changes each minute.
fn fonts_summary(settings: &ClockSettings) -> String {
    let font = settings.fonts.selected.label();
    if settings.fonts.rotate {
        format!("{font} · rotates")
    } else {
        font.to_owned()
    }
}

/// The renderer and its scale, which the launcher's settings choose for every
/// scenario, as rows after the Clock's own on its top page. A display that can
/// draw only raster fixes the renderer.
fn renderer_rows(window: &MainWindow) -> Vec<SettingsListRow> {
    let fixed = window.get_raster_only();
    let mut rows = Rows::default();
    rows.push(
        "renderer",
        if fixed { "Renderer (fixed)" } else { "Renderer" },
        &window.get_launcher_renderer(),
        "Raster draws each frame as an image, on any screen. Vector draws the shapes themselves, and needs a GPU.",
        SettingsRowKind::Choice,
        false,
    );
    rows.rows[0].enabled = !fixed;
    rows.push(
        "raster-scale",
        "Raster Scale",
        &format!("{}×", window.get_launcher_raster_scale_text()),
        "Draws raster frames this many times the screen's size, then fits them to it: smoother, but slower.",
        SettingsRowKind::Choice,
        false,
    );
    rows.rows
}

/// A page's rows, with the settings they show. The window holds both, so a
/// press steps the setting itself rather than reading it back from its label.
struct PageRows {
    settings: ClockSettings,
    rows: VecModel<SettingsListRow>,
}

impl Model for PageRows {
    type Data = SettingsListRow;

    fn row_count(&self) -> usize {
        self.rows.row_count()
    }

    fn row_data(&self, row: usize) -> Option<SettingsListRow> {
        self.rows.row_data(row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        self.rows.model_tracker()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A page's rows as they are added, counting the headings so far.
#[derive(Default)]
struct Rows {
    rows: Vec<SettingsListRow>,
    headings: i32,
    /// Whether an event's row has a Play button.
    playable: bool,
}

impl Rows {
    /// A setting's row: a switch or a choice, playable where it is an event.
    fn setting(&mut self, setting: &dyn ClockSetting, settings: &ClockSettings) {
        let about = setting.about();
        let kind = if setting.is_switch() {
            SettingsRowKind::Switch
        } else {
            SettingsRowKind::Choice
        };
        self.push(
            about.id,
            about.label,
            value(about.id, settings),
            about.help,
            kind,
            self.playable && event(about.id).is_some(),
        );
    }

    fn page(&mut self, id: &str, label: &str, summary: &str, help: &str) {
        self.push(id, label, summary, help, SettingsRowKind::Page, false);
    }

    fn heading(&mut self, label: &str) {
        self.push("", label, "", "", SettingsRowKind::Heading, false);
        self.headings += 1;
    }

    fn push(
        &mut self,
        id: &str,
        label: &str,
        value: &str,
        help: &str,
        kind: SettingsRowKind,
        playable: bool,
    ) {
        self.rows.push(SettingsListRow {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            help: help.into(),
            kind,
            playable,
            enabled: kind != SettingsRowKind::Heading,
            headings_before: self.headings,
        });
    }
}
