use super::*;
use crate::client_scenarios::RenderBackend;
use crate::thruster_visual_tests::{raster, svg, write_png};
use engine_common::{ClockArenaOwner, ClockArenaRemap, ClockArenaUnavailable, ClockEventProfile};
use engine_core::Vec2;
use scenario_clock::{
    EVENT_CATALOG,
    arena_fixture::{FixtureBall, FixtureStart},
};

/// A run at `viewport` that starts with `fixture`, after two seconds.
fn settled(viewport: Viewport, fixture: FixtureStart) -> ClockClientScenario {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            arena_fixture: fixture,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::from_millis(16));
    }
    ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    }
}

/// Both render adapters draw each of the fixtures' balls where it rests, at
/// its size.
#[test]
fn both_adapters_draw_the_fixture_where_it_rests() {
    let output = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let (fixture, plain) = (
            settled(viewport, FixtureStart::TwoOwners),
            settled(viewport, FixtureStart::None),
        );
        let scale = viewport.height / 480.0;
        // Each ball's center in pixels, and its radius.
        let balls: Vec<_> = fixture
            .state
            .arena_fixtures()
            .into_iter()
            .flat_map(|owner| {
                owner.balls.into_iter().map(move |ball| {
                    (
                        viewport.width * 0.5 + ball.position.x * scale,
                        viewport.height * 0.5 - ball.position.y * scale,
                        owner.radius * scale,
                    )
                })
            })
            .collect();
        assert_eq!(balls.len(), 5);
        let frames = fixture.render_frames(RenderBackend::Raster, viewport);
        assert_eq!(
            frames,
            fixture.render_frames(RenderBackend::Vector, viewport)
        );
        // The vector adapter draws the balls in a color the plain run does not
        // have, as one path with a circle for each.
        let fills = |scenario: &ClockClientScenario| -> Vec<(slint::Color, String)> {
            crate::render::scene_primitives_from_frames(
                &scenario.render_frames(RenderBackend::Vector, viewport),
                viewport,
            )
            .into_iter()
            .filter_map(|shape| match shape.fill {
                slint::Brush::SolidColor(color) => Some((color, shape.commands.to_string())),
                _ => None,
            })
            .collect()
        };
        let plain_colors: Vec<_> = fills(&plain).into_iter().map(|(color, _)| color).collect();
        let added: Vec<_> = fills(&fixture)
            .into_iter()
            .filter(|(color, _)| !plain_colors.contains(color))
            .collect();
        assert_eq!(added.len(), 1, "{viewport:?}");
        assert_eq!(added[0].1.matches('M').count(), balls.len(), "{viewport:?}");
        // The raster adapter paints every pixel whose center lies within a
        // ball. It rounds each row of a disc outward to whole pixels, which can
        // paint a pixel centered up to about a pixel and a half beyond a ball,
        // never two.
        let (pixels, background) = (
            raster(&frames[0], viewport),
            raster(
                &plain.render_frames(RenderBackend::Raster, viewport)[0],
                viewport,
            ),
        );
        let width = pixels.width() as usize;
        let mut discs = vec![(0, 0); balls.len()];
        for (index, (now, before)) in pixels
            .as_slice()
            .iter()
            .zip(background.as_slice())
            .enumerate()
        {
            let (x, y) = ((index % width) as f32 + 0.5, (index / width) as f32 + 0.5);
            let (nearest, reach) = balls
                .iter()
                .map(|&(cx, cy, radius)| (x - cx).hypot(y - cy) - radius)
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(
                now == before || reach <= 2.0,
                "{viewport:?}: ({x}, {y}) changed {reach} beyond ball {nearest}"
            );
            if reach <= 0.0 {
                discs[nearest].0 += usize::from(now != before);
                discs[nearest].1 += 1;
            }
        }
        for (ball, (changed, inside)) in balls.iter().zip(discs) {
            assert!(
                inside > 0 && changed == inside,
                "{viewport:?} {ball:?}: {changed} of {inside}"
            );
        }
        if let Some(output) = &output {
            write_png(&output.join(format!("arena-fixture-{name}.png")), &pixels);
            std::fs::write(
                output.join(format!("arena-fixture-{name}.svg")),
                svg(&frames[0], viewport),
            )
            .unwrap();
        }
    }
}

/// Clock's state names the arena's residents, and gives each event the arena
/// would refuse its reason, as a resize the residents do not fit leaves them.
#[test]
fn clock_state_publishes_the_arena_and_why_it_refuses_an_event() {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: Viewport::new(800.0, 480.0).aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    // Two balls near the right edge, which a square view could only take by
    // bringing them into each other.
    let height = state.default_arena_fixture()[0].position.y;
    let crowded = [300.0, 330.0].map(|x| FixtureBall {
        position: Vec2::new(x, height),
        velocity: Vec2::ZERO,
    });
    assert_eq!(state.admit_arena_fixture(&crowded), Ok(0));
    let mut scenario = ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    };
    for _ in 0..60 {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let arena = clock.arena.as_ref().expect("the arena");
    assert_eq!(arena.claims.len(), 1);
    assert_eq!(arena.claims[0].owner, ClockArenaOwner::Fixture { index: 0 });
    assert_eq!(arena.claims[0].population.bodies, 2);
    assert!(arena.last_step.is_some());
    assert!(
        clock
            .events
            .iter()
            .all(|event| event.blocked_by_arena.is_none())
    );

    scenario.set_viewport(Viewport::new(480.0, 480.0));
    let clock = scenario.clock_state().unwrap();
    let arena = clock.arena.as_ref().expect("the arena");
    assert!(arena.accounting.layout_incompatible);
    assert_eq!(arena.accounting.last_remap, Some(ClockArenaRemap::Declined));
    for event in &clock.events {
        assert_eq!(
            event.blocked_by_arena,
            EVENT_CATALOG[event.kind as usize]
                .uses_floor()
                .then_some(ClockArenaUnavailable::LayoutIncompatible),
            "{:?}",
            event.kind
        );
    }
    assert_eq!(
        spacewars_control::ClockState::from_json(&clock.to_json().unwrap()).unwrap(),
        clock
    );
}
