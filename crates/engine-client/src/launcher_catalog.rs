//! Picker policy, separate from scenario lookup and launch authorization.

use std::sync::{Arc, RwLock};

use engine_common::Settings;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::client_scenarios::{self, LauncherCategory};
use crate::{MainWindow, settings_writer::SettingsWriter};

const FALLBACK: &str = "spacewars";

#[derive(Debug, PartialEq, Eq)]
struct Selection {
    scenario: &'static str,
    show_all: bool,
}

impl Selection {
    fn restore(scenario: &str, show_all: bool) -> Self {
        match client_scenarios::registration(scenario) {
            Some(registration) if registration.launcher_category != LauncherCategory::Hidden => {
                Self {
                    scenario: registration.id,
                    show_all: show_all || registration.launcher_category == LauncherCategory::Extra,
                }
            }
            _ => Self {
                scenario: FALLBACK,
                show_all,
            },
        }
    }

    fn toggle(scenario: &str, show_all: bool) -> Self {
        let mut selection = Self::restore(scenario, show_all);
        selection.show_all = show_all;
        if !show_all
            && client_scenarios::registration(selection.scenario).is_some_and(|registration| {
                registration.launcher_category == LauncherCategory::Extra
            })
        {
            selection.scenario = FALLBACK;
        }
        selection
    }
}

pub(crate) fn scenario_names(show_all: bool) -> Vec<&'static str> {
    let mut registrations: Vec<_> = client_scenarios::launcher_registrations()
        .filter(|registration| {
            show_all || matches!(registration.launcher_category, LauncherCategory::Main(_))
        })
        .collect();
    // Stable sorting keeps extras in registry order.
    registrations.sort_by_key(|registration| match registration.launcher_category {
        LauncherCategory::Main(position) => u16::from(position),
        _ => u16::MAX,
    });
    registrations
        .into_iter()
        .map(|registration| registration.id)
        .collect()
}

fn publish(window: &MainWindow, selection: Selection) {
    window.set_launcher_show_all(selection.show_all);
    window.set_launcher_scenarios(ModelRc::new(VecModel::from(
        scenario_names(selection.show_all)
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    window.set_launcher_scenario(selection.scenario.into());
    crate::apply_scenario_metadata(window, selection.scenario);
    window.set_launcher_settings_focus_index(0);
}

fn persist(settings: &Arc<RwLock<Settings>>, writer: &SettingsWriter, show_all: bool) {
    let mut settings = settings.write().unwrap();
    if settings.launcher.show_all == show_all {
        return;
    }
    settings.launcher.show_all = show_all;
    let snapshot = settings.clone();
    drop(settings);
    let _ = writer.save(snapshot);
}

pub(crate) fn restore(
    window: &MainWindow,
    scenario: &str,
    settings: &Arc<RwLock<Settings>>,
    writer: &SettingsWriter,
) {
    let selection = Selection::restore(scenario, settings.read().unwrap().launcher.show_all);
    persist(settings, writer, selection.show_all);
    publish(window, selection);
}

pub(crate) fn install(
    window: &MainWindow,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
) {
    let weak = window.as_weak();
    window.on_launcher_toggle_show_all(move || {
        let Some(window) = weak.upgrade() else { return };
        let selection = Selection::toggle(
            window.get_launcher_scenario().as_str(),
            !window.get_launcher_show_all(),
        );
        persist(&settings, &writer, selection.show_all);
        publish(&window, selection);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn main_order_and_complete_extra_catalog_are_explicit() {
        let main = scenario_names(false);
        assert_eq!(
            main,
            [
                "clock",
                "spacewars",
                "scorched-earth",
                "pizza",
                "falling",
                "nes"
            ]
        );
        let all = scenario_names(true);
        assert!(all.starts_with(&main));
        let extras: Vec<_> = client_scenarios::registrations()
            .iter()
            .filter(|registration| registration.launcher_category == LauncherCategory::Extra)
            .map(|registration| registration.id)
            .collect();
        assert_eq!(&all[main.len()..], extras);
        assert_eq!(all.len(), all.iter().collect::<BTreeSet<_>>().len());
        assert!(all.contains(&"spacewars-classic"));
        assert!(all.contains(&"garden-lab"));
        assert!(!all.contains(&"null"));
        assert!(client_scenarios::registration("null").is_some());
        assert!(client_scenarios::registration("not-a-scenario").is_none());
    }

    #[test]
    fn restoration_and_toggling_keep_every_selection_visible() {
        for scenario in scenario_names(false) {
            for show_all in [false, true] {
                assert_eq!(
                    Selection::restore(scenario, show_all),
                    Selection { scenario, show_all }
                );
                assert_eq!(
                    Selection::toggle(scenario, show_all),
                    Selection { scenario, show_all }
                );
            }
        }
        for scenario in scenario_names(true)
            .into_iter()
            .filter(|id| !scenario_names(false).contains(id))
        {
            for show_all in [false, true] {
                assert_eq!(
                    Selection::restore(scenario, show_all),
                    Selection {
                        scenario,
                        show_all: true
                    }
                );
            }
            assert_eq!(
                Selection::toggle(scenario, false),
                Selection {
                    scenario: FALLBACK,
                    show_all: false
                }
            );
            assert_eq!(
                Selection::toggle(scenario, true),
                Selection {
                    scenario,
                    show_all: true
                }
            );
        }
        for show_all in [false, true] {
            assert_eq!(
                Selection::restore("null", show_all),
                Selection {
                    scenario: FALLBACK,
                    show_all
                }
            );
        }
    }
}
