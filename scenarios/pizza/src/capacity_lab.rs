//! Explicit, seeded normal-mode stress fixtures. Never selected by ordinary play.
use super::*;

pub const FIXTURE_VERSION: u32 = 1;

/// Healthy mixed-size bodies, separated initially into small, medium and large
/// bands. Counter-moving larger bodies start real impact/fragment cascades.
/// Small radii are within the normal fragment range, not renderer-only particles.
pub fn mixed_size_fixture(
    population: usize,
    limit: usize,
    bounds: PizzaBounds,
    seed: u64,
) -> Result<PizzaState, &'static str> {
    if !(64..=MAX_BALLS).contains(&population) || population > limit || limit > MAX_BALLS {
        return Err("fixture needs 64..=limit bodies and limit <= 5000");
    }
    let mut state = PizzaScenario::init(
        PizzaConfig {
            live_ball_limit: limit,
            desired_ball_count: 0,
            bounds,
            automatic_events: false,
            ..Default::default()
        },
        seed,
    );
    let large = (population / 150).clamp(8, 32);
    let medium = (population / 40).clamp(16, 128);
    let small = population - large - medium;
    add_band(&mut state, small, 0.02, 0.60, 0.002..=0.0035, 0.08)?;
    add_band(&mut state, medium, 0.64, 0.78, 0.005..=0.008, 0.6)?;
    add_band(&mut state, large, 0.82, 0.98, 0.012..=0.020, 3.0)?;
    Ok(state)
}

fn add_band(
    state: &mut PizzaState,
    count: usize,
    bottom: f32,
    top: f32,
    radii: std::ops::RangeInclusive<f32>,
    speed: f32,
) -> Result<(), &'static str> {
    let bounds = state.config.bounds;
    let width = bounds.width * 0.96;
    let height = bounds.height * (top - bottom);
    let cols = ((count as f32 * width / height).sqrt().ceil() as usize).max(2);
    let rows = count.div_ceil(cols);
    let cell_width = width / cols as f32;
    let cell_height = height / rows as f32;
    // Unsupported extreme viewports fail explicitly instead of shrinking balls.
    if cell_width.min(cell_height) <= 2.0 * radii.end() {
        return Err("viewport too narrow for the fixture's fixed radius bands");
    }
    for index in 0..count {
        let col = index % cols;
        let position = Vec2::new(
            bounds.width * 0.02 + (col as f32 + 0.5) * cell_width,
            bounds.height * bottom + (index / cols) as f32 * cell_height + cell_height * 0.5,
        );
        let radius = state.rng.random_range(radii.clone());
        let color = random_color(&mut state.rng);
        let mut ball = state.allocate_ball(position, radius, color);
        ball.velocity = Vec2::new(
            if col % 2 == 0 { speed } else { -speed },
            state.rng.random_range(-0.02..=0.02),
        );
        state.push_ball(ball);
    }
    Ok(())
}
