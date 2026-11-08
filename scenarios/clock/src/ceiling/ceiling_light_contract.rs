use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, grass::PlantPlayback};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};

fn fixture() -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        4242,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    s
}
fn seconds(s: &mut ClockState, n: u64) {
    for _ in 0..n * 60 {
        ClockScenario::step(s, &[], Duration::from_nanos(16_666_667));
    }
}
#[test]
fn ceiling_light_contract_exact_timing_pause_preview_and_duplicate() {
    let mut c = Ceiling::default();
    assert!(c.start(7));
    assert!(!c.advance(OPENING_DURATION - Duration::from_nanos(1)));
    assert_eq!(c.phase, Phase::Opening);
    assert!(c.advance(Duration::from_nanos(1)));
    assert_eq!(c.phase, Phase::Open);
    assert!(!c.start(7));
    c.advance(Duration::ZERO);
    assert_eq!(c.elapsed, Duration::ZERO);
    c.advance(OPEN_DURATION - Duration::from_nanos(1));
    assert_eq!(c.phase, Phase::Open);
    c.advance(Duration::from_nanos(1));
    assert_eq!(c.phase, Phase::Closing);
    c.advance(CLOSING_DURATION / 2);
    assert_eq!(c.fraction(), 0.5);
    c.advance(CLOSING_DURATION / 2);
    assert_eq!(c.phase, Phase::Closed);
    assert_eq!(c.deliveries, 1);
    let mut s = fixture();
    s.set_plant_playback(PlantPlayback::Preview16);
    seconds(&mut s, 6);
    assert_eq!(s.ceiling.phase, Phase::Open);
    let elapsed = s.ceiling.elapsed;
    let plants = s.plants.plants.observe();
    for _ in 0..10 {
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::Grass)],
            Duration::ZERO,
        );
    }
    assert_eq!(s.ceiling.elapsed, elapsed);
    assert_eq!(s.ceiling.sequence, 1);
    assert_eq!(s.plants.plants.observe(), plants);
    assert!(s.ceiling.phase == Phase::Open);
}
#[test]
fn ceiling_light_contract_cancellation_disable_resize_and_committed_retention() {
    let mut s = fixture();
    let mut settings = s.settings();
    settings.events.grass = false;
    ClockScenario::step(&mut s, &[ClockAction::configure(settings)], Duration::ZERO);
    seconds(&mut s, 6);
    assert!(s.soil.is_none());
    s.cancel_ceiling();
    s.cancel_ceiling();
    assert_eq!(s.ceiling.cancellations, 1);
    let mut s = fixture();
    seconds(&mut s, 6);
    let soil = s.soil.as_ref().unwrap().accounting();
    let organisms = s.grass_state().organisms;
    s.cancel_ceiling();
    s.cancel_ceiling();
    assert_eq!(s.ceiling.cancellations, 1);
    assert_eq!(s.soil.as_ref().unwrap().accounting(), soil);
    assert_eq!(s.grass_state().organisms, organisms);
    assert!(s.plants.apertures.is_empty());
    let mut s = fixture();
    s.set_aspect_ratio(0.75);
    seconds(&mut s, 6);
    assert!(s.soil.is_none());
    assert_eq!(s.ceiling.phase, Phase::Closed);
}
#[test]
fn ceiling_light_contract_coexistence_reading_and_incompatible_replacement() {
    let mut s = fixture();
    seconds(&mut s, 6);
    let biology = s.plants.plants.observe().tick;
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::preview_event(ClockEventKind::ColorCycle),
            ClockAction::set_reading(ClockReading::new(12, 35, 0).unwrap()),
        ],
        Duration::ZERO,
    );
    assert_eq!(s.ceiling.phase, Phase::Open);
    assert_eq!(s.reading().unwrap().minute(), 35);
    seconds(&mut s, 2);
    assert_eq!(s.plants.plants.observe().tick, biology + 2);
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Meltdown)],
        Duration::ZERO,
    );
    assert_eq!(s.ceiling.phase, Phase::Closed);
    assert!(s.plants.apertures.is_empty());
    assert_eq!(s.ceiling.deliveries, 1);
}
#[test]
fn ceiling_light_contract_reference_paid_growth_and_minutes_of_decline() {
    let mut s = fixture();
    let mut history = Vec::new();
    let (mut first_shoot, mut first_stress, mut first_damage, mut first_loss) =
        (None, None, None, None);
    let mut prior_living = 0;
    for second in 1..=665 {
        seconds(&mut s, 1);
        let o = s.plants.plants.observe();
        let g = s.grass_state();
        if g.shoots > 0 {
            first_shoot.get_or_insert(second);
        }
        if g.stressed_tissues > 0 {
            first_stress.get_or_insert(second);
        }
        // The reference sequence is about the lit grass patch. A retained mat
        // shares the runtime from the closing boundary onward, so every
        // observation here names grass explicitly.
        let grass = || {
            o.tissues
                .iter()
                .filter(|t| t.role != engine_core::plants::Role::Moss)
        };
        if grass().any(|t| t.health < 100) {
            first_damage.get_or_insert(second);
        }
        if g.living_tissues < prior_living {
            first_loss.get_or_insert(second);
        }
        prior_living = g.living_tissues;
        assert_eq!(
            g.ceiling_planned_material,
            g.ceiling_pending_material as u64
                + g.ceiling_committed_material
                + g.ceiling_cancelled_material
        );
        if [5, 10, 60, 180, 300, 305, 335, 365, 425, 485, 545, 605, 665].contains(&second) {
            history.push(serde_json::json!({"second":second,"phase":g.ceiling_phase,
                "living":g.living_tissues,"shoots":g.shoots,"mature":g.mature_blades,
                "stressed":g.stressed_tissues,"health_sum":grass().map(|t| u64::from(t.health)).sum::<u64>(),
                "mat":g.moss_tissues,
                "reserves":g.reserves,"intake_per_tick":g.intake,"upkeep_per_tick":g.maintenance}));
        }
        if second == 300 {
            assert!(g.shoots > 0 && g.intake.iter().any(|v| *v > 0));
            assert!(g.development.iter().all(|v| *v > 0) && g.maintenance.iter().all(|v| *v > 0));
        }
        if second == 425 {
            assert!(
                g.shoots > 0 && g.stressed_tissues > 0,
                "decline must preserve stressed living grass across minutes"
            );
        }
        if second == 485 {
            assert!(
                g.shoots > 0 && grass().any(|t| t.health < 100),
                "damage must be gradual before tissue loss"
            );
        }
        if second == 545 {
            assert_eq!(
                g.shoots, 0,
                "reference grass has lost shoots by three minutes of aftermath"
            );
        }
    }
    let evidence = serde_json::json!({"seed":4242,"playback":1,"opening_seconds":5,"hold_seconds":300,
        "closing_seconds":60,"grace_seconds":60,"damage_per_second":1,
        "first_shoot":first_shoot,"first_stress":first_stress,"first_damage":first_damage,"first_loss":first_loss,"history":history});
    println!("ceiling-reference {evidence}");
    if let Some(path) = std::env::var_os("SPACEWARS_CEILING_ARTIFACTS") {
        std::fs::write(
            std::path::Path::new(&path).join("ceiling-reference.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
    }
    assert!(first_shoot.is_some_and(|s| s <= 10));
    assert!(first_damage.is_some_and(|s| s > 365));
    assert_eq!(
        first_loss,
        Some(483),
        "measured loss after gradual closing and two minutes of decline"
    );
    assert_eq!(s.ceiling.phase, Phase::Closed);
    assert_eq!(s.ceiling.deliveries, 1);
}

#[test]
fn ceiling_light_contract_all_events_and_pending_conservation() {
    for kind in ClockEventKind::ALL {
        let mut s = fixture();
        seconds(&mut s, 6);
        let before = s.plants.plants.observe().tick;
        ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
        if compatible_event(kind) {
            assert_eq!(s.ceiling.phase, Phase::Open, "{kind:?}");
        } else {
            assert_eq!(s.ceiling.phase, Phase::Closed, "{kind:?}");
        }
        seconds(&mut s, 1);
        assert_eq!(
            s.plants.plants.observe().tick,
            before + 1,
            "single biology step: {kind:?}"
        );
        let g = s.grass_state();
        assert_eq!(
            g.ceiling_planned_material,
            g.ceiling_pending_material as u64
                + g.ceiling_committed_material
                + g.ceiling_cancelled_material
        );
        assert_eq!(g.soil_added, g.soil_retained as u64 + g.soil_exported);
    }
    for cancel in [true, false] {
        let mut s = fixture();
        let planned = s.ceiling.planned_material;
        assert!(planned > 0 && s.soil.is_none());
        if cancel {
            s.cancel_ceiling();
            s.cancel_ceiling();
        } else {
            let mut settings = s.settings();
            settings.events.grass = false;
            ClockScenario::step(&mut s, &[ClockAction::configure(settings)], Duration::ZERO);
        }
        seconds(&mut s, 6);
        assert!(s.soil.is_none());
        assert_eq!(s.ceiling.cancelled_material, planned);
        assert_eq!(s.ceiling.committed_material, 0);
    }
}

#[test]
fn ceiling_light_contract_equal_time_playback_with_fixed_aperture() {
    let mut normal = fixture();
    seconds(&mut normal, 6);
    let mut fast = fixture();
    seconds(&mut fast, 6);
    fast.set_plant_playback(PlantPlayback::Preview16);
    seconds(&mut normal, 160);
    seconds(&mut fast, 10);
    assert_eq!(normal.plants.plants, fast.plants.plants);
    assert_eq!(normal.plants.exposure, fast.plants.exposure);
    assert!(normal.ceiling.elapsed > fast.ceiling.elapsed);
    assert_eq!(normal.ceiling.phase, Phase::Open);
    assert_eq!(fast.ceiling.phase, Phase::Open);
}

#[test]
fn ceiling_light_contract_shoot_geometry_is_shared_with_beam_queries() {
    use crate::grass::residents::{OpticalScene, tissue_polygons};
    use engine_core::lighting::{LightId, Receiver};
    let mut s = fixture();
    seconds(&mut s, 60);
    let soil = s.soil.as_ref().unwrap();
    let o = s.plants.plants.observe();
    let polygons = tissue_polygons(&o, soil);
    assert!(!polygons.is_empty());
    let (id, polygon) = &polygons[0];
    let id = *id;
    let tip = polygon[2];
    let source = s.ceiling.sources(Layout::new(s.aspect_ratio()))[0];
    let receivers = [
        Receiver {
            position: tip,
            surface: Some(id),
        },
        Receiver {
            position: (polygon[0] + polygon[1]) * 0.5 - engine_core::Vec2::Y,
            surface: None,
        },
    ];
    let samples = OpticalScene {
        footprints: &[],
        polygons: &polygons,
        geometry: None,
        transparent: &[],
    }
    .query_light(
        s.simulation_tick(),
        0,
        std::slice::from_ref(&source),
        &receivers,
    )
    .unwrap();
    assert_eq!(
        samples[0].local, 950.0,
        "own tip endpoint must not self-shadow"
    );
    assert_eq!(
        samples[1].local, 0.0,
        "actual blade triangle must shadow the same column below"
    );
    assert_eq!(samples[1].first_blocker, Some(id));
    let moved: Vec<_> = polygons
        .into_iter()
        .map(|(id, p)| {
            (
                id,
                p.into_iter()
                    .map(|p| p + engine_core::Vec2::new(100.0, 0.0))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        OpticalScene {
            footprints: &[],
            polygons: &moved,
            geometry: None,
            transparent: &[],
        }
        .query_light(
            s.simulation_tick(),
            0,
            std::slice::from_ref(&source),
            &receivers
        )
        .unwrap()[1]
            .local,
        950.0
    );
    assert_ne!(id, LightId(0, 0));
}

#[test]
fn ceiling_light_contract_readings_fonts_and_layouts_keep_paid_growth() {
    // The cheap geometry contract below proves the corridor clears every font,
    // reading and aspect. This paid-growth sweep samples that matrix: every
    // font and the seven ordinary hours at the default aspect, then the
    // narrow/wide layouts with the default font and a leading-zero transition.
    let default_font = crate::ClockFontSettings::default().selected;
    for aspect in [0.25, 0.6, 0.75, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        let fonts: &[crate::ClockFont] = if aspect == 5.0 / 3.0 {
            &crate::ClockFont::ALL
        } else {
            std::slice::from_ref(&default_font)
        };
        for &font in fonts {
            let hours: &[u8] = if aspect == 5.0 / 3.0 && font == default_font {
                &[0, 8, 10, 12, 13, 20, 23]
            } else {
                &[0, 12]
            };
            for &hour in hours {
                let mut s = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: aspect,
                        event_profile: ClockEventProfile::Off,
                        fonts: crate::ClockFontSettings {
                            selected: font,
                            rotate: false,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    4242,
                );
                ClockScenario::step(
                    &mut s,
                    &[
                        ClockAction::set_reading(ClockReading::new(hour, 34, 0).unwrap()),
                        ClockAction::trigger_event(ClockEventKind::Grass),
                    ],
                    Duration::ZERO,
                );
                seconds(&mut s, 180);
                let g = s.grass_state();
                assert_eq!(s.active_font(), font);
                assert_eq!(g.optical_refusals, 0);
                println!(
                    "reading-growth aspect={aspect} font={font:?} hour={hour} shoots={} development={:?}",
                    g.shoots, g.development
                );
                assert!(
                    g.shoots > 0 && g.development.iter().all(|v| *v > 0),
                    "aspect={aspect} font={font:?} hour={hour}: {g:?}"
                );
                ClockScenario::step(
                    &mut s,
                    &[ClockAction::set_reading(
                        ClockReading::new((hour + 1) % 24, 0, 0).unwrap(),
                    )],
                    Duration::ZERO,
                );
                seconds(&mut s, 124);
                let g = s.grass_state();
                assert!(
                    g.shoots > 0 && g.development.iter().all(|v| *v > 0),
                    "transition aspect={aspect} font={font:?} hour={hour}: {g:?}"
                );
                assert_eq!(s.ceiling.phase, Phase::Open);
                assert_eq!(s.ceiling.deliveries, 1);
            }
        }
    }
}

#[test]
fn ceiling_light_contract_margin_and_digit_occlusion_share_geometry() {
    use crate::grass::residents::{OpticalScene, optical_polygons};
    use engine_core::{Vec2, lighting::Receiver};
    for aspect in [0.25, 0.6, 0.75, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        for font in crate::ClockFont::ALL {
            for hour in [0, 8, 12, 20, 23] {
                let mut s = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: aspect,
                        event_profile: ClockEventProfile::Off,
                        fonts: crate::ClockFontSettings {
                            selected: font,
                            rotate: false,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    4242,
                );
                ClockScenario::step(
                    &mut s,
                    &[ClockAction::set_reading(
                        ClockReading::new(hour, 34, 0).unwrap(),
                    )],
                    Duration::ZERO,
                );
                let layout = Layout::new(aspect);
                let mut ceiling = Ceiling::default();
                ceiling.start(4242);
                ceiling.advance(OPENING_DURATION);
                let sources = ceiling.sources(layout);
                let (span_left, span_right) = Ceiling::span(layout);
                assert!(!sources.is_empty() && sources.len() <= crate::ceiling::MAX_OPENINGS);
                for opening in &sources {
                    assert!(opening.left < opening.right);
                    assert!(opening.left >= span_left && opening.right <= span_right);
                }
                // The lighting publication refuses overlapping openings, so the
                // drawn geometry must already be disjoint at every layout.
                for pair in sources.windows(2) {
                    assert!(pair[0].right < pair[1].left, "openings stay disjoint");
                }
                let polygons = crate::render::plant_face_polygons(&s);
                assert!(!polygons.is_empty());
                let digit_x =
                    polygons[0].iter().map(|p| p.x).sum::<f32>() / polygons[0].len() as f32;
                // Floor beyond every opening, which no hole may light.
                let outside = (0..=2000)
                    .map(|i| span_left + (span_right - span_left) * i as f32 / 2000.0)
                    .find(|x| sources.iter().all(|a| !(a.left..=a.right).contains(x)))
                    .expect("partitioned slots leave unlit floor");
                let mut receivers: Vec<_> = sources
                    .iter()
                    .map(|a| Receiver {
                        position: Vec2::new((a.left + a.right) * 0.5, layout.floor_y + 10.0),
                        surface: None,
                    })
                    .collect();
                receivers.push(Receiver {
                    position: Vec2::new(outside, layout.floor_y + 10.0),
                    surface: None,
                });
                let quads = optical_polygons(polygons);
                let occluded = OpticalScene {
                    footprints: &[],
                    polygons: &quads,
                    geometry: None,
                    transparent: &[],
                };
                let samples = occluded.query_light(0, 50, &sources, &receivers).unwrap();
                for (index, sample) in samples.iter().take(sources.len()).enumerate() {
                    assert_eq!(
                        sample.local, 950.0,
                        "margin {aspect} {font:?} {hour} opening {index}"
                    );
                    assert_eq!(sample.directional, 50.0);
                }
                assert_eq!(samples[sources.len()].local, 0.0, "outside every opening");
                // A diagnostic opening over the whole floor must still be
                // blocked by the actual digit silhouettes, then light this
                // receiver when they are removed.
                let wide = [engine_core::lighting::ApertureSource {
                    left: span_left,
                    right: span_right,
                    ..sources[0]
                }];
                let under_digit = [Receiver {
                    position: Vec2::new(digit_x, layout.floor_y + 10.0),
                    surface: None,
                }];
                let shadow = occluded.query_light(0, 50, &wide, &under_digit).unwrap();
                assert_eq!(shadow[0].local, 0.0);
                assert!(shadow[0].first_blocker.is_some());
                let clear = OpticalScene {
                    footprints: &[],
                    polygons: &[],
                    geometry: None,
                    transparent: &[],
                }
                .query_light(0, 50, &wide, &under_digit)
                .unwrap();
                assert_eq!(clear[0].local, 950.0);
            }
        }
    }
}
