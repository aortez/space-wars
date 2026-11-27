//! A visual matrix of crow visits (#372): one to four crows on a landscape
//! and a portrait face, a flock on each other font, wet flocks, and flocks
//! whose perches change under them, through both render adapters.
use super::*;
use engine_common::{
    ClockCrowWaterTolerance, ClockFont, ClockFontSettings, ClockRainAmount, ClockTimeFormat,
};
use scenario_clock::ClockDate;

/// A visit's cap: by this tick every crow has gone.
const CAP: u64 = 22 * 60;
/// Every fourth step is a frame of the exported playback: 15 a second.
const FRAME_STEP: u64 = 4;
/// How long Heavy Rain has been falling when an after-rain flock comes: its
/// 20 seconds of rain are over and its water lies pooled, as in the
/// benchmark's rain fixtures.
const AFTER_RAIN: u64 = 24 * 60;

/// What changes under a case's crows, and when, in ticks of their visit.
#[derive(Clone, Copy)]
enum Change {
    /// Heavy Rain begins.
    Rain(u64),
    /// Heavy Rain began before the crows came, and has stopped.
    AfterRain,
    /// The face turns from 12:34 to 11:11. A one keeps only its upright, the
    /// narrowest top a digit has.
    Ones(u64),
    /// Falling takes every perch.
    Falling(u64),
}

struct Case {
    name: String,
    width: u32,
    height: u32,
    font: ClockFont,
    crows: usize,
    tolerance: ClockCrowWaterTolerance,
    /// In 12-hour time, with the date shown.
    dated: bool,
    change: Option<Change>,
}

fn cases() -> Vec<Case> {
    let case = |width, height, font, crows, change, name: &str| Case {
        name: format!("{width}x{height}-{name}"),
        width,
        height,
        font,
        crows,
        tolerance: ClockCrowWaterTolerance::Varied,
        dated: false,
        change,
    };
    let mut cases = Vec::new();
    for (width, height) in [(1024, 768), (480, 800)] {
        for crows in 1..=engine_common::CLOCK_MAX_CROWS {
            let name = format!("{crows}-crow{}", if crows == 1 { "" } else { "s" });
            cases.push(case(width, height, ClockFont::Classic, crows, None, &name));
        }
    }
    cases.push(case(800, 480, ClockFont::Matrix, 4, None, "matrix"));
    cases.push(case(800, 480, ClockFont::Sans, 4, None, "sans"));
    cases.push(Case {
        dated: true,
        ..case(800, 480, ClockFont::Serif, 4, None, "serif-dated")
    });
    let classic = ClockFont::Classic;
    cases.push(case(1024, 768, classic, 4, Some(Change::Rain(180)), "rain"));
    cases.push(Case {
        tolerance: ClockCrowWaterTolerance::Hardy,
        ..case(1024, 768, classic, 4, Some(Change::Rain(180)), "rain-hardy")
    });
    cases.push(case(
        800,
        480,
        classic,
        4,
        Some(Change::AfterRain),
        "after-rain",
    ));
    cases.push(case(1024, 768, classic, 4, Some(Change::Ones(600)), "ones"));
    cases.push(case(
        480,
        800,
        classic,
        4,
        Some(Change::Falling(540)),
        "falling",
    ));
    cases
}

/// Each case's visit renders through the raster and the vector adapters,
/// its crows drawn while they are there and every one gone by the cap. A
/// small sample always runs; with `SPACEWARS_CROW_FLOCK_ARTIFACTS` set, every
/// fourth step is written as a frame, with a viewer that plays the matrix.
#[test]
fn crow_flocks_render_and_export_a_visual_matrix() {
    let output = std::env::var_os("SPACEWARS_CROW_FLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    let date = ClockDate::new(2026, 11, 27).unwrap();
    let mut manifest = Vec::new();
    for case in cases() {
        let directory = output.as_ref().map(|path| path.join(&case.name));
        if let Some(path) = &directory {
            std::fs::create_dir_all(path).unwrap();
        }
        let viewport = Viewport::new(case.width as f32, case.height as f32);
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                rain_amount: ClockRainAmount::Heavy,
                crow_water_tolerance: case.tolerance,
                fonts: ClockFontSettings {
                    selected: case.font,
                    ..Default::default()
                },
                time_format: if case.dated {
                    ClockTimeFormat::TwelveHour
                } else {
                    ClockTimeFormat::TwentyFourHour
                },
                show_date: case.dated,
                ..Default::default()
            },
            42,
        );
        let reading = |tick: u64| {
            let ones = matches!(case.change, Some(Change::Ones(at)) if tick >= at);
            let (hour, minute) = if ones { (11, 11) } else { (12, 34) };
            ClockReading::new(hour, minute, (tick / 60 % 60) as u8)
                .unwrap()
                .with_date(date)
        };
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(0))],
            Duration::ZERO,
        );
        if let Some(Change::AfterRain) = case.change {
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Rain)],
                Duration::ZERO,
            );
            for _ in 0..AFTER_RAIN {
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
        }
        assert!(state.preview_crows(case.crows), "{}", case.name);
        let mut samples = Vec::new();
        for tick in 0..=CAP {
            let mut actions = Vec::new();
            let turns = matches!(case.change, Some(Change::Ones(at)) if tick == at);
            if tick % 60 == 0 || turns {
                actions.push(ClockAction::set_reading(reading(tick)));
            }
            match case.change {
                Some(Change::Rain(at)) if tick == at => {
                    actions.push(ClockAction::preview_event(ClockEventKind::Rain));
                }
                Some(Change::Falling(at)) if tick == at => {
                    actions.push(ClockAction::preview_event(ClockEventKind::Falling));
                }
                _ => {}
            }
            if !actions.is_empty() {
                ClockScenario::step(&mut state, &actions, Duration::ZERO);
            }
            let crows = state.crow_states();
            let export = directory.is_some() && tick % FRAME_STEP == 0;
            if tick % 120 == 0 || export {
                let frame = ClockScenario::render_frame(&state);
                let drawn = frame
                    .layers
                    .iter()
                    .filter(|layer| layer.z == CREATURE_LAYER)
                    .any(|layer| !layer.primitives.is_empty());
                assert_eq!(drawn, !crows.is_empty(), "{} tick {tick}", case.name);
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(path) = directory.as_ref().filter(|_| export) {
                    let index = tick / FRAME_STEP;
                    write_png(&path.join(format!("frame-{index:04}.png")), &pixels);
                }
            }
            if tick % FRAME_STEP == 0 {
                let crows: Vec<_> = crows
                    .iter()
                    .map(|crow| serde_json::json!([crow.member, crow.phase.as_str()]))
                    .collect();
                samples.push(serde_json::json!(crows));
            }
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(state.crow_states().is_empty(), "{}", case.name);
        manifest.push(serde_json::json!({
            "name": case.name,
            "width": case.width,
            "height": case.height,
            "samples": samples,
        }));
    }
    if let Some(path) = &output {
        std::fs::write(
            path.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("index.html"), include_str!("flock.html")).unwrap();
    }
}
