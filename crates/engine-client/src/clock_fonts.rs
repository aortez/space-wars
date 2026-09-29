//! Shared font picker for launcher settings and the paused live clock.
use crate::{
    MainWindow, host,
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use engine_common::{ClockFont, ClockFontPool, ClockFontSettings};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use spacewars_control::{UiAction, UiControl};

pub(crate) fn publish(window: &MainWindow, fonts: ClockFontSettings) {
    window.set_clock_font_selected(fonts.selected as i32);
    window.set_clock_font_rotate(fonts.rotate);
    window.set_clock_font_pool_count(fonts.pool.bits().count_ones() as i32);
    window.set_clock_font_included(ModelRc::new(VecModel::from(
        ClockFont::ALL
            .map(|font| fonts.pool.contains(font))
            .to_vec(),
    )));
}

pub(crate) fn settings(window: &MainWindow) -> Result<ClockFontSettings, String> {
    let selected = *ClockFont::ALL
        .get(window.get_clock_font_selected() as usize)
        .ok_or("Unknown Clock font")?;
    let pool = ClockFont::ALL
        .into_iter()
        .filter(|font| {
            window
                .get_clock_font_included()
                .row_data(*font as usize)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    Ok(ClockFontSettings {
        selected,
        rotate: window.get_clock_font_rotate(),
        pool: ClockFontPool::try_from(pool).map_err(str::to_string)?,
    })
}

pub(crate) fn install(window: &MainWindow, controls: host::SharedScenarioControls) {
    window.set_clock_font_labels(ModelRc::new(VecModel::from(
        ClockFont::ALL.map(|f| f.label().into()).to_vec(),
    )));
    window.set_clock_font_sources(ModelRc::new(VecModel::from(
        ClockFont::ALL.map(|f| f.source().into()).to_vec(),
    )));
    let images = ClockFont::ALL
        .map(|font| {
            RasterRenderer::new().image_from_frames_with_layout(
                &[scenario_clock::ClockScenario::font_preview(font)],
                Viewport::new(340.0, 100.0),
                FrameLayout::EqualHorizontal,
                RasterOptions::default(),
            )
        })
        .to_vec();
    window.set_clock_font_previews(ModelRc::new(VecModel::from(images)));
    let weak = window.as_weak();
    window.on_clock_fonts_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_launcher_scenario() != "clock" || window.get_clock_controls_pending() {
            return;
        }
        if !window.get_launcher_settings_visible() && !window.get_ingame_clock_visible() {
            return;
        }
        window.set_clock_font_focus(window.get_clock_font_selected());
        window.set_clock_fonts_visible(true);
    });
    let weak = window.as_weak();
    window.on_clock_font_activate(move |index| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_clock_fonts_visible() || window.get_clock_controls_pending() {
            return;
        }
        window.set_clock_font_focus(index);
        if index == 9 {
            window.set_clock_fonts_visible(false);
            return;
        }
        let Ok(mut fonts) = settings(&window) else {
            return;
        };
        match index {
            0..=3 => {
                fonts.selected = ClockFont::ALL[index as usize];
                if fonts.rotate {
                    fonts.pool.include(fonts.selected);
                }
            }
            4..=7 => fonts.pool.toggle(ClockFont::ALL[(index - 4) as usize]),
            8 => fonts.rotate = !fonts.rotate,
            _ => return,
        }
        if window.get_launcher_visible() {
            // Launcher choices are durably saved with the existing Play flow.
            publish(&window, fonts);
        } else {
            let mut controls = controls.borrow_mut();
            let Some(state) = controls.clock_state() else {
                return;
            };
            let mut settings = state.settings;
            settings.fonts = fonts;
            if controls.request_clock_settings(settings) {
                window.set_clock_controls_pending(true);
            }
        }
    });
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    if window.get_clock_controls_pending() {
        return;
    }
    const ORDER: [i32; 10] = [0, 4, 1, 5, 2, 6, 3, 7, 8, 9];
    let index = window.get_clock_font_focus();
    match action {
        UiAction::Back | UiAction::Controls | UiAction::Start => {
            window.set_clock_fonts_visible(false)
        }
        UiAction::Confirm => window.invoke_clock_font_activate(index),
        UiAction::Up | UiAction::Left | UiAction::Down | UiAction::Right => {
            let current = ORDER.iter().position(|i| *i == index).unwrap_or(0) as i32;
            let delta = if matches!(action, UiAction::Up | UiAction::Left) {
                -1
            } else {
                1
            };
            window.set_clock_font_focus(
                ORDER[crate::ui_navigation::moved_selection(current, 10, delta) as usize],
            );
        }
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    let Ok(settings) = settings(window) else {
        return Vec::new();
    };
    let enabled = !window.get_clock_controls_pending();
    let mut controls: Vec<_> = ClockFont::ALL
        .into_iter()
        .map(|font| {
            UiControl::new(
                format!("clock.fonts.select.{}", font.label().to_ascii_lowercase()),
                font.label(),
                enabled,
            )
            .with_value(if settings.selected == font {
                "Selected"
            } else {
                ""
            })
        })
        .collect();
    controls.extend(ClockFont::ALL.into_iter().map(|font| {
        UiControl::new(
            format!("clock.fonts.pool.{}", font.label().to_ascii_lowercase()),
            format!("Include {}", font.label()),
            enabled && (!settings.pool.contains(font) || settings.pool.bits().count_ones() > 1),
        )
        .with_value(if settings.pool.contains(font) {
            "On"
        } else {
            "Off"
        })
    }));
    controls.push(
        UiControl::new("clock.fonts.rotate", "Rotate each minute", enabled)
            .with_value(if settings.rotate { "On" } else { "Off" }),
    );
    controls.push(UiControl::new("clock.fonts.back", "Done", enabled));
    controls
}
