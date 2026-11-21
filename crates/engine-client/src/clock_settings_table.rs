//! Every Clock setting the settings pages change, described once. The pause
//! page and the launcher both step a setting through this table, so it takes
//! the same values in the same order wherever it is shown.

use engine_common::{
    ClockCrowWaterTolerance, ClockEventProfile, ClockLampMode, ClockMarqueePreset, ClockRainAmount,
    ClockSettings, ClockThemeChoice, ClockTimeFormat,
};

use crate::ui_navigation;

/// One Clock setting: the values it takes and how it reads and writes them.
pub(crate) trait ClockSetting: Sync {
    /// The control name after `pause.clock.` or `launcher.settings.clock.`.
    fn id(&self) -> &'static str;
    /// Every value's label, in the order Right steps through them.
    fn labels(&self) -> Vec<&'static str>;
    /// The current value's place in that order.
    fn index(&self, settings: &ClockSettings) -> usize;
    /// Takes the value at `index`, leaving everything else as it was.
    fn choose(&self, settings: &mut ClockSettings, index: usize);
}

/// The settings: how the Clock looks and how often events come, then the
/// events by group.
pub(crate) static SETTINGS: [&dyn ClockSetting; 22] = [
    &Choice::<ClockTimeFormat> {
        id: "time-format",
        value: |settings| &mut settings.time_format,
    },
    &Switch {
        id: "show-date",
        on: |settings| &mut settings.show_date,
    },
    &Choice::<ClockThemeChoice> {
        id: "theme",
        value: |settings| &mut settings.theme,
    },
    &Choice::<ClockLampMode> {
        id: "lamps",
        value: |settings| &mut settings.lamps,
    },
    &Choice::<ClockEventProfile> {
        id: "event-profile",
        value: |settings| &mut settings.event_profile,
    },
    // Events that change the digits.
    &Switch {
        id: "cell-swap",
        on: |settings| &mut settings.events.cell_swap,
    },
    &Switch {
        id: "coins",
        on: |settings| &mut settings.events.coins,
    },
    &Switch {
        id: "color-cycle",
        on: |settings| &mut settings.events.color_cycle,
    },
    &Switch {
        id: "digit-slide",
        on: |settings| &mut settings.events.digit_slide,
    },
    &Switch {
        id: "explosion",
        on: |settings| &mut settings.events.explosion,
    },
    &Switch {
        id: "falling",
        on: |settings| &mut settings.events.falling,
    },
    &Switch {
        id: "marquee",
        on: |settings| &mut settings.events.marquee,
    },
    &Choice::<ClockMarqueePreset> {
        id: "marquee-preset",
        value: |settings| &mut settings.marquee_preset,
    },
    &Switch {
        id: "meltdown",
        on: |settings| &mut settings.events.meltdown,
    },
    &Switch {
        id: "slot-machine",
        on: |settings| &mut settings.events.slot_machine,
    },
    // Visitors.
    &OffOr::<ClockCrowWaterTolerance> {
        id: "crow",
        on: |settings| &mut settings.events.crow,
        value: |settings| &mut settings.crow_water_tolerance,
    },
    &Switch {
        id: "duck",
        on: |settings| &mut settings.events.duck,
    },
    &Switch {
        id: "spider",
        on: |settings| &mut settings.events.spider,
    },
    &Switch {
        id: "tanks",
        on: |settings| &mut settings.events.tanks,
    },
    // Events in the room.
    &Switch {
        id: "aquarium",
        on: |settings| &mut settings.events.aquarium,
    },
    &Switch {
        id: "grass",
        on: |settings| &mut settings.events.grass,
    },
    &OffOr::<ClockRainAmount> {
        id: "rain",
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
    id: &'static str,
    on: fn(&mut ClockSettings) -> &mut bool,
}

impl ClockSetting for Switch {
    fn id(&self) -> &'static str {
        self.id
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
    id: &'static str,
    value: fn(&mut ClockSettings) -> &mut T,
}

impl<T: Choices> ClockSetting for Choice<T> {
    fn id(&self) -> &'static str {
        self.id
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
    id: &'static str,
    on: fn(&mut ClockSettings) -> &mut bool,
    value: fn(&mut ClockSettings) -> &mut T,
}

impl<T: Choices> ClockSetting for OffOr<T> {
    fn id(&self) -> &'static str {
        self.id
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

impl Choices for ClockMarqueePreset {
    const ALL: &'static [Self] = &ClockMarqueePreset::ALL;
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

/// The defaults, then the defaults with every setting stepped one to seven
/// places, one place at a time. Among them each switch is both ways round,
/// and Rain and Crow are off keeping Heavy and Hardy.
#[cfg(test)]
pub(crate) fn starting_points() -> Vec<ClockSettings> {
    (0..8)
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
            labels("marquee-preset"),
            ClockMarqueePreset::ALL.map(ClockMarqueePreset::label)
        );
        for id in [
            "show-date",
            "cell-swap",
            "coins",
            "color-cycle",
            "digit-slide",
            "explosion",
            "falling",
            "marquee",
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
                        "marquee-preset" => rest.marquee_preset = start.marquee_preset,
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

    /// Both pages offer every setting in the table, and nothing else there
    /// changes a setting. A setting missing from the table would need its own
    /// code on each page.
    #[test]
    fn both_pages_offer_every_setting_in_the_table() {
        let table = SETTINGS
            .iter()
            .map(|setting| setting.id())
            .collect::<BTreeSet<_>>();
        for (order, prefix, others) in [
            (
                &crate::ui_inventory::PAUSE_CLOCK_FOCUS_ORDER[..],
                "pause.clock.",
                &["preview-event", "back", "preview", "fonts"][..],
            ),
            (
                &crate::ui_inventory::LAUNCHER_CLOCK_FOCUS_ORDER[..],
                "launcher.settings.clock.",
                &["fonts"][..],
            ),
        ] {
            let names = order
                .iter()
                .filter_map(|control| control.strip_prefix(prefix))
                .collect::<Vec<_>>();
            let settings = names
                .iter()
                .copied()
                .filter(|id| find(id).is_some())
                .collect::<BTreeSet<_>>();
            let rest = names
                .iter()
                .copied()
                .filter(|id| find(id).is_none())
                .collect::<Vec<_>>();
            assert_eq!(settings, table, "{prefix}");
            assert_eq!(rest, others, "{prefix}");
        }
    }
}
