//! The Clock's settings as pages of rows: the top page and the two it opens,
//! Look and Events, where each event's row can also play it. The rows come
//! from the settings table, so a setting added there appears on its page with
//! nothing to change here.
//!
//! The window holds which page shows and the focused row. Keyboard, gamepad,
//! touch and control-ID activation all move between pages through here.

use engine_common::{ClockEventKind, ClockSettings};
use slint::{Model, ModelRc, VecModel};
use spacewars_control::{UiAction, UiControl, UiScreen};

use crate::clock_settings_table::{self, ClockSetting, Section};
use crate::{MainWindow, SettingsListRow, SettingsRowKind};

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

    /// The screen the control API reports while the page shows.
    pub(crate) const fn screen(self) -> UiScreen {
        match self {
            Self::Top => UiScreen::PauseClock,
            Self::Look => UiScreen::PauseClockLook,
            Self::Events => UiScreen::PauseClockEvents,
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

/// Shows every page's rows for `settings`.
pub(crate) fn publish(window: &MainWindow, settings: &ClockSettings) {
    let model = |page| ModelRc::new(VecModel::from(rows(page, settings)));
    window.set_clock_menu_top_rows(model(Page::Top));
    window.set_clock_menu_look_rows(model(Page::Look));
    window.set_clock_menu_events_rows(model(Page::Events));
}

/// The rows of `page`, each named by its control's ID after `pause.clock.`.
pub(crate) fn rows(page: Page, settings: &ClockSettings) -> Vec<SettingsListRow> {
    let mut rows = Rows::default();
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
                "How often events come, which ones, and Play to see one now.",
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

/// The row at `index` on the page showing, or `None` for Back, which follows
/// the rows.
pub(crate) fn row(window: &MainWindow, index: i32) -> Option<SettingsListRow> {
    let index = usize::try_from(index).ok()?;
    window.get_clock_menu_rows().row_data(index)
}

/// Shows `page`, focused on its row named `focus`, or on its first row.
pub(crate) fn show(window: &MainWindow, page: Page, focus: Option<&str>) {
    window.set_clock_menu_page(page.name().into());
    let index = focus
        .and_then(|id| {
            window
                .get_clock_menu_rows()
                .iter()
                .position(|row| row.id == id)
        })
        .unwrap_or(0);
    window.set_ingame_clock_focus_index(index as i32);
}

/// Starts Clock Controls over on the top page, for a launch of the Clock.
/// Until the next launch, Restart included, they open where they were left.
pub(crate) fn reset(window: &MainWindow) {
    show(window, Page::Top, None);
}

/// Opens the page that the row at `index` names, if it names one.
pub(crate) fn open_row(window: &MainWindow, index: i32) {
    let Some(row) = row(window, index) else {
        return;
    };
    match row.id.as_str() {
        "look" => show(window, Page::Look, None),
        "events" => show(window, Page::Events, None),
        "fonts" => window.invoke_clock_fonts_open(),
        _ => {}
    }
}

/// Back: to the page that opens this one, on the row that opens it, or from
/// the top page out of Clock Controls, which next open on their first row.
pub(crate) fn back(window: &MainWindow) {
    match page(window).parent() {
        Some((parent, row)) => show(window, parent, Some(row)),
        None => {
            reset(window);
            window.set_ingame_clock_visible(false);
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

/// The controls on the page showing, as the control API lists them, each with
/// its row and the action that presses it there: a switch by its name, a
/// choice by its ‹ and ›, a page by its name, Play, and Back.
pub(crate) fn targets(window: &MainWindow) -> Vec<(UiControl, i32, UiAction)> {
    let rows = window.get_clock_menu_rows();
    let mut targets = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let index = index as i32;
        let id = format!("pause.clock.{}", row.id);
        let named = |label: &str| UiControl::new(id.as_str(), label, true);
        match row.kind {
            SettingsRowKind::Heading => continue,
            SettingsRowKind::Choice => {
                for (end, label, action) in [
                    ("previous", "‹", UiAction::Left),
                    ("next", "›", UiAction::Right),
                ] {
                    let control = UiControl::new(format!("{id}.{end}"), label, true);
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
            let control = UiControl::new(format!("{id}.play"), "Play", true);
            targets.push((control, index, UiAction::Confirm));
        }
    }
    targets.push((
        UiControl::new("pause.clock.back", "Back", true),
        rows.row_count() as i32,
        UiAction::Confirm,
    ));
    targets
}

/// The focused row, as the control API names it.
pub(crate) fn selected(window: &MainWindow) -> String {
    row(window, window.get_ingame_clock_focus_index()).map_or_else(
        || "pause.clock.back".to_owned(),
        |row| format!("pause.clock.{}", row.id),
    )
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

/// A page's rows as they are added, counting the headings so far.
#[derive(Default)]
struct Rows {
    rows: Vec<SettingsListRow>,
    headings: i32,
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
            event(about.id).is_some(),
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
