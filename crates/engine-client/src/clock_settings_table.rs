//! Every Clock setting the settings pages change, described once. The pause
//! page and the launcher both step a setting through this table, so it takes
//! the same values in the same order wherever it is shown.

use engine_common::{
    ClockCrowWaterTolerance, ClockEventProfile, ClockLampMode, ClockMarqueePreset, ClockRainAmount,
    ClockSettings, ClockThemeChoice, ClockTimeFormat,
};

use crate::ui_navigation;
use Section::{Digits, HowOften, Look, Room, Visitors};

/// One Clock setting: what it is called, the values it takes and how it reads
/// and writes them.
pub(crate) trait ClockSetting: Sync {
    /// The setting's name, label, place on the pages and help line.
    fn about(&self) -> &About;
    /// Every value's label, in the order Right steps through them.
    fn labels(&self) -> Vec<&'static str>;
    /// The current value's place in that order.
    fn index(&self, settings: &ClockSettings) -> usize;
    /// Takes the value at `index`, leaving everything else as it was.
    fn choose(&self, settings: &mut ClockSettings, index: usize);

    /// The control name after `pause.clock.` or `launcher.settings.clock.`.
    fn id(&self) -> &'static str {
        self.about().id
    }

    /// Whether the setting is just Off or On, which the pages show as a
    /// switch.
    fn is_switch(&self) -> bool {
        false
    }
}

/// What a setting is called and where its row appears, apart from its values.
pub(crate) struct About {
    /// The control name after `pause.clock.` or `launcher.settings.clock.`.
    pub(crate) id: &'static str,
    /// The row's name on the settings pages.
    pub(crate) label: &'static str,
    /// Where the row appears.
    pub(crate) section: Section,
    /// One line on what the setting does, shown while its row is focused.
    pub(crate) help: &'static str,
}

const fn about(
    id: &'static str,
    label: &'static str,
    section: Section,
    help: &'static str,
) -> About {
    About {
        id,
        label,
        section,
        help,
    }
}

/// Where a setting's row appears: on the Look page, or on the Events page,
/// first or under one of its headings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Look,
    HowOften,
    Digits,
    Visitors,
    Room,
}

impl Section {
    /// The heading over the section's rows, if it has one.
    pub(crate) const fn heading(self) -> Option<&'static str> {
        match self {
            Self::Look | Self::HowOften => None,
            Self::Digits => Some("Digits"),
            Self::Visitors => Some("Visitors"),
            Self::Room => Some("Room"),
        }
    }
}

/// The settings in the order the pages list them: the Look page's, then the
/// Events page's, how often first and then the events by group, alphabetical
/// within each.
pub(crate) static SETTINGS: [&dyn ClockSetting; 21] = [
    &Choice::<ClockTimeFormat> {
        about: about("time-format", "Time", Look, "24-hour or 12-hour digits."),
        value: |settings| &mut settings.time_format,
    },
    &Switch {
        about: about(
            "show-date",
            "Date",
            Look,
            "The weekday and date, in a quiet line above the digits.",
        ),
        on: |settings| &mut settings.show_date,
    },
    &Choice::<ClockThemeChoice> {
        about: about(
            "theme",
            "Theme",
            Look,
            "Auto: Christmas on December nights, 6 pm to 6 am.",
        ),
        value: |settings| &mut settings.theme,
    },
    &Choice::<ClockLampMode> {
        about: about(
            "lamps",
            "Lamps",
            Look,
            "Lively lamps follow what goes on; Steady lamps stay at rest.",
        ),
        value: |settings| &mut settings.lamps,
    },
    &Choice::<ClockEventProfile> {
        about: about(
            "event-profile",
            "How often",
            HowOften,
            "Off: no automatic events. Calm waits 45–75 s between events; Demo 6–10 s.",
        ),
        value: |settings| &mut settings.event_profile,
    },
    &Switch {
        about: about(
            "cell-swap",
            "Cell Swap",
            Digits,
            "Lit cells shuffle or swap places, then settle back.",
        ),
        on: |settings| &mut settings.events.cell_swap,
    },
    &Switch {
        about: about(
            "coins",
            "Coins",
            Digits,
            "Lit cells turn to coins that drop and pile up, and a magpie takes a few.",
        ),
        on: |settings| &mut settings.events.coins,
    },
    &Switch {
        about: about(
            "color-cycle",
            "Color Cycle",
            Digits,
            "The digits change color for six seconds.",
        ),
        on: |settings| &mut settings.events.color_cycle,
    },
    &Switch {
        about: about(
            "digit-slide",
            "Digit Slide",
            Digits,
            "Changed digits roll into place when the minute changes.",
        ),
        on: |settings| &mut settings.events.digit_slide,
    },
    &Switch {
        about: about(
            "explosion",
            "Explosion",
            Digits,
            "The digits burst apart and bounce, then reform.",
        ),
        on: |settings| &mut settings.events.explosion,
    },
    &Switch {
        about: about(
            "falling",
            "Falling",
            Digits,
            "The digits fall to the floor, then reform.",
        ),
        on: |settings| &mut settings.events.falling,
    },
    &OffOr::<ClockMarqueePreset> {
        about: about(
            "marquee",
            "Marquee",
            Digits,
            "The digits move and light up in a 12-second show. Random picks a different one each time.",
        ),
        on: |settings| &mut settings.events.marquee,
        value: |settings| &mut settings.marquee_preset,
    },
    &Switch {
        about: about(
            "meltdown",
            "Meltdown",
            Digits,
            "The digits melt into pools that drain away, then reform.",
        ),
        on: |settings| &mut settings.events.meltdown,
    },
    &Switch {
        about: about(
            "slot-machine",
            "Slot Machine",
            Digits,
            "The digits spin like reels and stop on the time.",
        ),
        on: |settings| &mut settings.events.slot_machine,
    },
    &OffOr::<ClockCrowWaterTolerance> {
        about: about(
            "crow",
            "Crow",
            Visitors,
            "A crow lands and hops on the digits. Shy crows avoid rain, Hardy ones don't, and Varied mixes them.",
        ),
        on: |settings| &mut settings.events.crow,
        value: |settings| &mut settings.crow_water_tolerance,
    },
    &Switch {
        about: about(
            "duck",
            "Duck",
            Visitors,
            "A duck runs along the floor, jumping what's in its way.",
        ),
        on: |settings| &mut settings.events.duck,
    },
    &Switch {
        about: about(
            "spider",
            "Spider",
            Visitors,
            "A spider drops in on a silk line and climbs and swings on the digits.",
        ),
        on: |settings| &mut settings.events.spider,
    },
    &Switch {
        about: about(
            "tanks",
            "Tanks",
            Visitors,
            "Two tanks duel on hills below the time.",
        ),
        on: |settings| &mut settings.events.tanks,
    },
    &Switch {
        about: about(
            "aquarium",
            "Aquarium",
            Room,
            "Fish swim through the Clock, with the time still readable.",
        ),
        on: |settings| &mut settings.events.aquarium,
    },
    &Switch {
        about: about(
            "grass",
            "Grass",
            Room,
            "Soil settles on the floor and grass grows in it.",
        ),
        on: |settings| &mut settings.events.grass,
    },
    &OffOr::<ClockRainAmount> {
        about: about(
            "rain",
            "Rain",
            Room,
            "Showers fill pools that drain away. Light to Heavy sets how hard; Varied changes it.",
        ),
        on: |settings| &mut settings.events.rain,
        value: |settings| &mut settings.rain_amount,
    },
];

/// The setting with this control name, if there is one.
pub(crate) fn find(id: &str) -> Option<&'static dyn ClockSetting> {
    SETTINGS.iter().copied().find(|setting| setting.id() == id)
}

/// `settings` with `setting` moved `delta` places, wrapping at either end.
pub(crate) fn stepped(
    setting: &dyn ClockSetting,
    mut settings: ClockSettings,
    delta: i32,
) -> ClockSettings {
    let count = setting.labels().len() as i32;
    let index = ui_navigation::moved_selection(setting.index(&settings) as i32, count, delta);
    setting.choose(&mut settings, index as usize);
    settings
}

/// Reads a field through the accessor that also writes it. The accessor
/// lends the field mutably, so it reads from a copy and `settings` needs only
/// a shared borrow.
fn read<T: Copy>(settings: &ClockSettings, field: fn(&mut ClockSettings) -> &mut T) -> T {
    let mut copy = *settings;
    *field(&mut copy)
}

/// Off or On.
struct Switch {
    about: About,
    on: fn(&mut ClockSettings) -> &mut bool,
}

impl ClockSetting for Switch {
    fn about(&self) -> &About {
        &self.about
    }

    fn is_switch(&self) -> bool {
        true
    }

    fn labels(&self) -> Vec<&'static str> {
        vec!["Off", "On"]
    }

    fn index(&self, settings: &ClockSettings) -> usize {
        usize::from(read(settings, self.on))
    }

    fn choose(&self, settings: &mut ClockSettings, index: usize) {
        *(self.on)(settings) = index == 1;
    }
}

/// One of a type's values.
struct Choice<T: 'static> {
    about: About,
    value: fn(&mut ClockSettings) -> &mut T,
}

impl<T: Choices> ClockSetting for Choice<T> {
    fn about(&self) -> &About {
        &self.about
    }

    fn labels(&self) -> Vec<&'static str> {
        T::ALL.iter().map(|choice| choice.label()).collect()
    }

    fn index(&self, settings: &ClockSettings) -> usize {
        let current = read(settings, self.value);
        T::ALL
            .iter()
            .position(|choice| *choice == current)
            .unwrap_or(0)
    }

    fn choose(&self, settings: &mut ClockSettings, index: usize) {
        *(self.value)(settings) = T::ALL[index];
    }
}

/// Off, or one of a type's values. Turning it off keeps the value, so the
/// saved settings still hold it for when it is on again.
struct OffOr<T: 'static> {
    about: About,
    on: fn(&mut ClockSettings) -> &mut bool,
    value: fn(&mut ClockSettings) -> &mut T,
}

impl<T: Choices> ClockSetting for OffOr<T> {
    fn about(&self) -> &About {
        &self.about
    }

    fn labels(&self) -> Vec<&'static str> {
        std::iter::once("Off")
            .chain(T::ALL.iter().map(|choice| choice.label()))
            .collect()
    }

    fn index(&self, settings: &ClockSettings) -> usize {
        if !read(settings, self.on) {
            return 0;
        }
        let current = read(settings, self.value);
        1 + T::ALL
            .iter()
            .position(|choice| *choice == current)
            .unwrap_or(0)
    }

    fn choose(&self, settings: &mut ClockSettings, index: usize) {
        *(self.on)(settings) = index != 0;
        if let Some(choice) = index.checked_sub(1) {
            *(self.value)(settings) = T::ALL[choice];
        }
    }
}

/// The values a choice takes, with the labels the pages show for them.
trait Choices: Copy + PartialEq + Sync + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
}

impl Choices for ClockTimeFormat {
    const ALL: &'static [Self] = &[Self::TwentyFourHour, Self::TwelveHour];
    fn label(self) -> &'static str {
        crate::clock_time_format_label(self)
    }
}

impl Choices for ClockEventProfile {
    const ALL: &'static [Self] = &[Self::Off, Self::Calm, Self::Demo];
    fn label(self) -> &'static str {
        crate::clock_event_profile_label(self)
    }
}

impl Choices for ClockCrowWaterTolerance {
    const ALL: &'static [Self] = &ClockCrowWaterTolerance::ALL;
    fn label(self) -> &'static str {
        ClockCrowWaterTolerance::label(self)
    }
}

impl Choices for ClockLampMode {
    const ALL: &'static [Self] = &ClockLampMode::ALL;
    fn label(self) -> &'static str {
        ClockLampMode::label(self)
    }
}

/// Random first, as Rain and Crow put Varied first, so that a step back from
/// the default turns Marquee off.
impl Choices for ClockMarqueePreset {
    const ALL: &'static [Self] = &[
        Self::Random,
        Self::ClockChase,
        Self::ClockWave,
        Self::ClockSpin,
        Self::DigitSpin,
        Self::TextScroll,
        Self::TextRibbon,
        Self::TextSpin,
    ];
    fn label(self) -> &'static str {
        ClockMarqueePreset::label(self)
    }
}

impl Choices for ClockRainAmount {
    const ALL: &'static [Self] = &ClockRainAmount::ALL;
    fn label(self) -> &'static str {
        ClockRainAmount::label(self)
    }
}

impl Choices for ClockThemeChoice {
    const ALL: &'static [Self] = &ClockThemeChoice::ALL;
    fn label(self) -> &'static str {
        ClockThemeChoice::label(self)
    }
}

/// The defaults, then the defaults with every setting stepped one to eight
/// places, one place at a time. Among them each switch is both ways round,
/// and Rain, Crow and Marquee are off keeping Heavy, Hardy and Text spin.
#[cfg(test)]
pub(crate) fn starting_points() -> Vec<ClockSettings> {
    (0..9)
        .map(|places| {
            let mut settings = ClockSettings::default();
            for setting in SETTINGS {
                for _ in 0..places {
                    settings = stepped(setting, settings, 1);
                }
            }
            settings
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use engine_common::ClockEventKind;

    use super::*;

    #[test]
    fn every_setting_has_its_own_name_and_two_or_more_distinct_values() {
        let ids = SETTINGS
            .iter()
            .map(|setting| setting.id())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), SETTINGS.len(), "two settings share a name");
        for setting in SETTINGS {
            let labels = setting.labels();
            let distinct = labels.iter().collect::<BTreeSet<_>>();
            assert!(labels.len() >= 2, "{} has one value", setting.id());
            assert_eq!(distinct.len(), labels.len(), "{}", setting.id());
        }
    }

    /// The order Right steps through the values on both pages, pinned so
    /// that it changes only on purpose.
    #[test]
    fn the_values_come_in_the_order_the_pages_step_through_them() {
        let labels = |id| find(id).unwrap().labels();
        assert_eq!(labels("time-format"), ["24-hour", "12-hour"]);
        assert_eq!(labels("event-profile"), ["Off", "Calm", "Demo"]);
        assert_eq!(labels("theme"), ["Auto", "Normal", "Christmas"]);
        assert_eq!(labels("lamps"), ["Steady", "Lively"]);
        assert_eq!(
            labels("rain"),
            ["Off", "Varied", "Light", "Medium", "Heavy"]
        );
        assert_eq!(labels("crow"), ["Off", "Varied", "Shy", "Hardy"]);
        assert_eq!(
            labels("marquee"),
            [
                "Off",
                "Random",
                "Clock chase",
                "Clock wave",
                "Clock spin",
                "Digit spin",
                "Text scroll",
                "Text ribbon",
                "Text spin",
            ]
        );
        for id in [
            "show-date",
            "cell-swap",
            "coins",
            "color-cycle",
            "digit-slide",
            "explosion",
            "falling",
            "meltdown",
            "slot-machine",
            "duck",
            "spider",
            "tanks",
            "aquarium",
            "grass",
        ] {
            assert_eq!(labels(id), ["Off", "On"], "{id}");
        }
    }

    #[test]
    fn every_step_moves_one_value_and_wraps_at_either_end() {
        for start in starting_points() {
            for setting in SETTINGS {
                let count = setting.labels().len() as i32;
                let first = setting.index(&start) as i32;
                for delta in [1, -1] {
                    let mut settings = start;
                    for step in 1..=count {
                        settings = stepped(setting, settings, delta);
                        assert_eq!(
                            setting.index(&settings) as i32,
                            (first + delta * step).rem_euclid(count),
                            "{} by {delta}",
                            setting.id()
                        );
                    }
                }
            }
        }
    }

    /// Rain, Crow and Marquee keep their amount, tolerance and recipe while
    /// Off, so a step back turns them on again as they were.
    #[test]
    fn rain_crow_and_marquee_keep_their_value_while_off() {
        for (id, last) in [
            ("rain", "Heavy"),
            ("crow", "Hardy"),
            ("marquee", "Text spin"),
        ] {
            let setting = find(id).unwrap();
            let labels = setting.labels();
            let mut on = ClockSettings::default();
            while labels[setting.index(&on)] != last {
                on = stepped(setting, on, 1);
            }
            let off = stepped(setting, on, 1);
            assert_eq!(labels[setting.index(&off)], "Off", "{id}");
            assert_eq!(
                ClockSettings {
                    events: on.events,
                    ..off
                },
                on,
                "{id} lost its value"
            );
            assert_eq!(stepped(setting, off, -1), on, "{id}");
        }
    }

    /// Each event's setting goes by the event's name, which
    /// `stepping_a_setting_changes_only_its_own_fields` relies on.
    #[test]
    fn every_event_has_a_setting_by_its_name() {
        for kind in ClockEventKind::ALL {
            assert!(
                find(kind.as_str()).is_some(),
                "{} has no setting",
                kind.as_str()
            );
        }
    }

    /// A step changes only the fields its setting names. The events are read
    /// through `ClockEventKind` and the other fields by name, apart from the
    /// table, so a setting wired to the wrong field fails here.
    #[test]
    fn stepping_a_setting_changes_only_its_own_fields() {
        for start in starting_points() {
            for setting in SETTINGS {
                for delta in [1, -1] {
                    let changed = stepped(setting, start, delta);
                    for kind in ClockEventKind::ALL {
                        if kind.as_str() != setting.id() {
                            assert_eq!(
                                changed.events.enabled(kind),
                                start.events.enabled(kind),
                                "{} by {delta} switched {}",
                                setting.id(),
                                kind.as_str()
                            );
                        }
                    }
                    let mut rest = changed;
                    rest.events = start.events;
                    match setting.id() {
                        "time-format" => rest.time_format = start.time_format,
                        "show-date" => rest.show_date = start.show_date,
                        "theme" => rest.theme = start.theme,
                        "lamps" => rest.lamps = start.lamps,
                        "event-profile" => rest.event_profile = start.event_profile,
                        "marquee" => rest.marquee_preset = start.marquee_preset,
                        "rain" => rest.rain_amount = start.rain_amount,
                        "crow" => rest.crow_water_tolerance = start.crow_water_tolerance,
                        _ => {}
                    }
                    assert_eq!(
                        rest,
                        start,
                        "{} by {delta} changed another field",
                        setting.id()
                    );
                }
            }
        }
    }

    /// Each menu's pages offer every setting in the table once, and nothing
    /// else there changes a setting.
    #[test]
    fn each_menu_offers_every_setting_once() {
        use crate::clock_menu::{self, Menu, Page};
        let mut table = SETTINGS
            .iter()
            .map(|setting| setting.id())
            .collect::<Vec<_>>();
        table.sort_unstable();
        for menu in [Menu::Pause, Menu::Launcher] {
            let rows = [Page::Top, Page::Look, Page::Events]
                .into_iter()
                .flat_map(|page| clock_menu::rows(menu, page, &ClockSettings::default()))
                .filter(|row| row.kind != crate::SettingsRowKind::Heading)
                .collect::<Vec<_>>();
            let (mut settings, rest): (Vec<_>, Vec<_>) = rows
                .iter()
                .map(|row| row.id.as_str())
                .partition(|id| find(id).is_some());
            settings.sort_unstable();
            assert_eq!(settings, table, "{menu:?}");
            assert_eq!(rest, ["look", "events", "fonts"], "{menu:?}");
        }
    }
}
