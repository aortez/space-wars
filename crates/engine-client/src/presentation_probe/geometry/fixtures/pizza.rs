//! Normal-path event captures: no benchmark bodies, forced poses or disabled damage.
use super::*;
use engine_common::Scenario;
use scenario_pizza::{
    MAX_SPAWN_RATE, PizzaAction, PizzaBounds, PizzaConfig, PizzaEventDirection, PizzaEventKind,
    PizzaEventState, PizzaScenario, PizzaState,
};

const DT: Duration = Duration::from_nanos(16_666_667);

fn run(viewport: Viewport, direction: PizzaEventDirection, population: usize) -> PizzaState {
    let request = PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode();
    // Seed selection uses the public admission path, and is independent of the
    // subsequent warmup's ball RNG. The chosen direction is never overwritten.
    let seed = (0..128).find(|seed| {
        let mut state = PizzaScenario::init(PizzaConfig {
            desired_ball_count: 0, automatic_events: false, ..Default::default()
        }, *seed);
        PizzaScenario::step(&mut state, std::slice::from_ref(&request), DT);
        matches!(state.event_state(), PizzaEventState::Active { direction: d, .. } if d == direction)
    }).unwrap();
    let mut state = PizzaScenario::init(
        PizzaConfig {
            desired_ball_count: population,
            ball_spawn_rate: MAX_SPAWN_RATE,
            bounds: PizzaBounds::from_aspect_ratio(viewport.aspect_ratio()),
            automatic_events: false,
            ..Default::default()
        },
        seed,
    );
    for _ in 0..300 {
        PizzaScenario::step(&mut state, &[], DT);
    }
    PizzaScenario::step(&mut state, &[request], Duration::ZERO);
    state
}

pub(super) fn build(
    direction: &str,
    viewport: Viewport,
) -> Result<Fixture, Box<dyn std::error::Error>> {
    let kind = match direction {
        "left" => PizzaEventDirection::Left,
        "right" => PizzaEventDirection::Right,
        "top" => PizzaEventDirection::Top,
        "bottom" => PizzaEventDirection::Bottom,
        _ => return Err(format!("unknown Pizza event direction {direction}").into()),
    };
    let mut state = run(viewport, kind, 120);
    for _ in 0..360 {
        PizzaScenario::step(&mut state, &[], DT);
    }
    eprintln!(
        "pizza_event_fixture={}",
        serde_json::json!({
            "case":format!("pizza-event-{direction}"), "seed":state.seed(),
            "tick":state.tick, "event":state.event_state(), "balls":state.balls.len(),
        })
    );
    let frames = vec![PizzaScenario::render_frame(&state)];
    Ok(Fixture {
        name: format!("pizza-event-{direction}"),
        seed: state.seed(),
        checksum: checksum(&serde_json::to_vec(&frames)?),
        frames,
        tick: state.tick as u32,
        objects: state.balls.len(),
        omitted_text: 0,
        sequence: Vec::new(),
        meshes: Vec::new(),
        producer: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        raster::{RasterOptions, RasterRenderer},
        thruster_visual_tests::write_png,
    };

    // Opt-in full physical runs for tuning/captures; focused field tests cover
    // correctness on every PR without adding this visual sweep to its runtime.
    #[test]
    #[ignore = "normal-path event calibration and screenshot export"]
    fn pizza_event_calibration_and_captures() {
        let output =
            std::env::var_os("SPACEWARS_PIZZA_EVENT_ARTIFACTS").map(std::path::PathBuf::from);
        if let Some(path) = &output {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut records = Vec::new();
        for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
            let viewport = Viewport::new(width as f32, height as f32);
            for direction in PizzaEventDirection::ALL {
                for population in [24, 120] {
                    let mut state = run(viewport, direction, population);
                    let mut contacts = 0;
                    let mut replacements = 0;
                    let initial_last_id = state.balls.iter().map(|b| b.id).max().unwrap();
                    for elapsed in 0..=960 {
                        if [0, 60, 120, 360, 780, 840, 900, 960].contains(&elapsed) {
                            let frame = PizzaScenario::render_frame(&state);
                            let pixels = RasterRenderer::new()
                                .image_from_frames_with_layout(
                                    std::slice::from_ref(&frame),
                                    viewport,
                                    FrameLayout::EqualHorizontal,
                                    RasterOptions::default(),
                                )
                                .to_rgb8()
                                .unwrap();
                            let (mass, progress) =
                                state.balls.iter().fold((0.0, 0.0), |(mass, progress), b| {
                                    assert!(b.position.x.is_finite() && b.position.y.is_finite());
                                    assert!(b.velocity.x.is_finite() && b.velocity.y.is_finite());
                                    let coordinate = match direction {
                                        PizzaEventDirection::Left => {
                                            1.0 - b.position.x / state.config.bounds.width
                                        }
                                        PizzaEventDirection::Right => {
                                            b.position.x / state.config.bounds.width
                                        }
                                        PizzaEventDirection::Top => {
                                            b.position.y / state.config.bounds.height
                                        }
                                        PizzaEventDirection::Bottom => {
                                            1.0 - b.position.y / state.config.bounds.height
                                        }
                                    };
                                    let weight = b.radius * b.radius;
                                    (mass + weight, progress + coordinate * weight)
                                });
                            let name = format!(
                                "pizza-{width}x{height}-{direction:?}-{population}-{elapsed:03}"
                            )
                            .to_lowercase();
                            if let Some(path) = &output {
                                write_png(&path.join(format!("{name}.png")), &pixels);
                            }
                            records.push(serde_json::json!({
                                "name":name, "seed":state.seed(), "tick":state.tick, "elapsed":elapsed,
                                "event":state.event_state(), "balls":state.balls.len(),
                                "mass_weighted_edge_progress":progress/mass, "contact_samples":contacts,
                                "new_bodies_since_admission":replacements,
                                "min_radius":state.balls.iter().map(|b|b.radius).reduce(f32::min),
                                "max_radius":state.balls.iter().map(|b|b.radius).reduce(f32::max),
                            }));
                        }
                        if elapsed < 960 {
                            PizzaScenario::step(&mut state, &[], DT);
                            contacts += state.last_step_metrics.rapier.contacts;
                            replacements = state
                                .balls
                                .iter()
                                .map(|b| b.id)
                                .max()
                                .unwrap_or(0)
                                .saturating_sub(initial_last_id);
                        }
                    }
                    assert!(contacts > 0);
                    assert!(matches!(state.event_state(), PizzaEventState::Idle { .. }));
                }
            }
        }
        if let Some(path) = &output {
            std::fs::write(
                path.join("calibration.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
    }
}
