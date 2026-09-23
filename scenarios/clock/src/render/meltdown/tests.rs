use super::*;

mod edge_lab;

fn column(index: usize, bed: f64, height: f64) -> Column {
    Column {
        left: index as f64 * 3.0,
        width: 3.0,
        bed,
        bed_edges: [bed; 2],
        surface: bed + height,
        volume: height * 3.0,
        displaced: 0.0,
        velocity: 0.0,
    }
}

#[test]
fn shared_surface_edges_preserve_area_and_do_not_bridge_dry_spots_or_steps() {
    for columns in [
        vec![
            column(0, 0.0, 4.0),
            column(1, 0.0, 12.0),
            column(2, 0.0, 3.0),
            column(3, 0.0, 9.0),
        ],
        vec![
            column(0, 0.0, 4.0),
            column(1, 0.0, 0.0),
            column(2, 0.0, 3.0),
            column(3, 2.0, 9.0),
        ],
        vec![column(0, -200.0, 4.0)],
    ] {
        let mut area = 0.0;
        for (i, c) in columns.iter().copied().enumerate() {
            if !visible(c) {
                continue;
            }
            let previous = i.checked_sub(1).map(|i| columns[i]);
            let next = columns.get(i + 1).copied();
            let edge = surface_edges(c, previous, next);
            assert!(edge.iter().all(|&y| y >= c.bed && y.is_finite()));
            area += ((edge[0] + edge[1]) * 0.5 - c.bed) * c.width;
            if let Some(next) = next {
                if next.bed == c.bed && visible(next) {
                    assert_eq!(
                        edge[1],
                        surface_edges(next, Some(c), columns.get(i + 2).copied())[0]
                    );
                } else {
                    assert_eq!(edge[1], c.surface);
                }
            }
        }
        assert!((area - columns.iter().map(|c| c.volume).sum::<f64>()).abs() < 1e-8);
    }
}

pub(super) fn area(points: &[Vec2]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let anchor = points[0];
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| {
            let a = *a - anchor;
            let b = *b - anchor;
            a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64
        })
        .sum::<f64>()
        .abs()
        * 0.5
}

#[test]
fn accelerating_ribbons_thin_and_preserve_volume_before_clipping() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        for volume in [0.01, 1.0, 100.0] {
            for velocity in [
                Vec2::ZERO,
                Vec2::new(100.0, 0.0),
                Vec2::new(0.0, -200.0),
                Vec2::new(60.0, -900.0),
            ] {
                let p = Parcel {
                    position: Vec2::ZERO,
                    velocity,
                    volume,
                    duration: dt,
                    horizontal_bounds: None,
                };
                let points = ribbon(p);
                assert!(
                    (area(&points) - volume).abs() < volume * 2e-3,
                    "{p:?}: rendered={}",
                    area(&points)
                );
                if velocity.y < 0.0 {
                    assert!((points[2] - points[1]).length() <= (points[3] - points[0]).length());
                }
            }
        }
    }
}

#[test]
fn channel_clipping_keeps_convex_ribbons_inside_banks_without_shearing_them() {
    for x in [-30.0, -10.0, 0.0, 10.0, 30.0] {
        for vx in [-100.0, 0.0, 100.0] {
            let points = ribbon(Parcel {
                position: Vec2::new(x, -20.0),
                velocity: Vec2::new(vx, -100.0),
                volume: 10.0,
                duration: 1.0 / 60.0,
                horizontal_bounds: Some([-10.0, 10.0]),
            });
            let clipped: Vec<_> = clip_channel(points, Some([-10.0, 10.0]))
                .into_iter()
                .map(|p| Vec2::new(p.x, p.y))
                .collect();
            assert!(clipped.iter().all(|p| p.x.abs() <= 10.0 && p.y.is_finite()));
            assert!(clipped.len() <= 6);
            assert!(area(&clipped) <= area(&points) + 1e-4);
            for (i, a) in clipped.iter().enumerate() {
                let b = clipped[(i + 1) % clipped.len()];
                let c = clipped[(i + 2) % clipped.len()];
                let ab = b - *a;
                let bc = c - b;
                assert!(ab.x * bc.y - ab.y * bc.x >= -1e-4);
            }
            if x.abs() == 30.0 {
                assert!(clipped.is_empty());
            }
        }
    }
}

#[test]
fn compact_splash_outlines_preserve_volume_and_clip_at_their_own_walls() {
    for volume in [0.01, 1.0, 100.0] {
        for x in [-60.0, 0.0, 20.0] {
            let p = Parcel {
                position: Vec2::new(x, 0.0),
                velocity: Vec2::ZERO,
                volume,
                duration: 1.0 / 60.0,
                horizontal_bounds: Some([-60.0, 20.0]),
            };
            let points = drop_outline(p);
            assert!((area(&points) - volume).abs() < volume * 1e-4);
            let clipped: Vec<_> = clip_channel(points, p.horizontal_bounds)
                .into_iter()
                .map(|p| Vec2::new(p.x, p.y))
                .collect();
            assert!(clipped.iter().all(|p| (-60.0..=20.0).contains(&p.x)));
            assert!(area(&clipped) <= volume * 1.0001);
        }
    }
}

#[test]
fn a_block_keeps_its_digit_color_and_square_outline_until_impact() {
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use std::time::Duration;
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Meltdown),
        ],
        Duration::ZERO,
    );
    let before = ClockScenario::render_frame(&state);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    event.tick = 100;
    // Hold poses fixed to isolate time-based changes in shape and color.
    assert_eq!(ClockScenario::render_frame(&state), before);
}

#[test]
fn reformation_is_bottom_up_monotone_and_complete_before_cleanup() {
    for row in 0..=8 {
        assert_eq!(reform_progress(0, row), 0.0);
        assert_eq!(reform_progress(REFORMING_TICKS - 1, row), 1.0);
        let mut previous = 0.0;
        for tick in 0..REFORMING_TICKS {
            let p = reform_progress(tick, row);
            assert!(p >= previous && p <= 1.0);
            previous = p;
        }
    }
    assert!(reform_progress(30, 0) > 0.5);
    assert_eq!(reform_progress(30, 8), 0.0);
}
