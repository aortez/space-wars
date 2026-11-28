//! Inputs and action-head terms for one decision in a replay.
use engine_common::brain_controls::BodyProfile;
use engine_core::Vec2;
use engine_rapier::world::polar::{
    PolarSensor,
    policy::{BAND_EDGES_BODY_LENGTHS, DISTANCE_SCALE},
};
use scenario_clock::duck_training::{DuckControls, DuckEpisode};
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};
use spacewars_ai::c3::{C3Genome, DECISION_TICKS};
use std::{f32::consts::TAU, sync::OnceLock};

pub(super) struct Decision {
    pub tick: u64,
    pub outputs: [f32; 4],
    pub applied: DuckControls,
    contributions: [[f32; 64]; 4],
    biases: [f32; 4],
    /// Each cell has four equal-area samples. Two counts in 0..=4 fit in one byte.
    cells: Box<[u8]>,
    goal: Vec2,
}

impl Decision {
    pub fn read(
        episode: &DuckEpisode,
        sensor: &PolarSensor,
        genome: &C3Genome,
        tick: u64,
    ) -> Result<Self, String> {
        if tick != episode.progress().tick
            || !tick.is_multiple_of(u64::from(DECISION_TICKS))
            || !episode.controls_active()
        {
            return Err(format!("No creature decision at tick {tick}"));
        }
        let observation = episode.observation(sensor)?;
        let trace = genome.inspect(&observation).map_err(|e| e.to_string())?;
        let applied = DuckControls::decode(&trace.outputs, &BodyProfile::duck()).controls;
        let cells = observation
            .geometry
            .cells
            .iter()
            .map(|[sampled, overlap]| {
                let sampled = (sampled * 4.).round() as u8;
                let overlap = (overlap * 4.).round() as u8;
                sampled * 5 + overlap
            })
            .collect();
        // The goal is relative to the center of mass. The polar sensor is at the body origin.
        let motion = observation.self_goal;
        let goal = Vec2::new(motion[0] + motion[8], motion[1] + motion[9]);
        Ok(Self {
            tick,
            outputs: trace.outputs,
            applied,
            contributions: trace.contributions,
            biases: trace.biases,
            cells,
            goal,
        })
    }

    pub fn contributions(&self, action: usize) -> &[f32; 64] {
        &self.contributions[action]
    }

    pub fn bias(&self, action: usize) -> f32 {
        self.biases[action]
    }

    pub fn polar_image(&self) -> Image {
        let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(SIDE as u32, SIDE as u32);
        let pixels = buffer.make_mut_slice();
        for (pixel, sample) in pixels.iter_mut().zip(map_pixels()) {
            *pixel = match sample {
                Some((index, grid)) => cell_color(self.cells[usize::from(*index)], *grid),
                None => Rgb8Pixel::new(9, 19, 30),
            };
        }
        let center = Vec2::splat(CENTER);
        let direction = Vec2::new(self.goal.x, -self.goal.y);
        if direction.length() > 0. {
            let unit = direction.normalized();
            let length = direction.length().min(1.) * RADIUS;
            let tip = center + unit * length;
            let head = 12.0_f32.min(length * 0.5);
            let wing = Vec2::new(-unit.y, unit.x) * (head * 0.55);
            let base = tip - unit * head;
            line(pixels, center, tip);
            line(pixels, tip, base + wing);
            line(pixels, tip, base - wing);
        }
        let actor = Rgb8Pixel::new(238, 249, 250);
        for y in -2..=2 {
            for x in -2..=2 {
                if x * x + y * y <= 4 {
                    pixels[((CENTER as i32 + y) as usize) * SIDE + (CENTER as i32 + x) as usize] =
                        actor;
                }
            }
        }
        Image::from_rgb8(buffer)
    }
}

const SECTORS: usize = 64;
const SIDE: usize = 193;
const CENTER: f32 = (SIDE / 2) as f32;
const RADIUS: f32 = CENTER - 3.;

/// Map the fixed display raster to the observation's unequal radial bands once.
/// Sector zero starts at +x and bearings increase toward local +y (screen up).
fn map_pixels() -> &'static [Option<(u16, bool)>] {
    static PIXELS: OnceLock<Vec<Option<(u16, bool)>>> = OnceLock::new();
    PIXELS.get_or_init(|| {
        (0..SIDE * SIDE)
            .map(|i| {
                let local = Vec2::new((i % SIDE) as f32 - CENTER, CENTER - (i / SIDE) as f32);
                let range = local.length() / RADIUS * DISTANCE_SCALE;
                if range > DISTANCE_SCALE {
                    return None;
                }
                let band = BAND_EDGES_BODY_LENGTHS[1..].partition_point(|outer| *outer < range);
                let turn = local.y.atan2(local.x).rem_euclid(TAU) / TAU;
                let sector = (turn * SECTORS as f32) as usize % SECTORS;
                let outer = BAND_EDGES_BODY_LENGTHS[band + 1];
                let ring = ((band + 1).is_multiple_of(4) || outer == DISTANCE_SCALE)
                    && (outer - range) * RADIUS / DISTANCE_SCALE < 0.8;
                Some((
                    (band * SECTORS + sector) as u16,
                    ring || local.x == 0. || local.y == 0.,
                ))
            })
            .collect()
    })
}

fn cell_color(code: u8, grid: bool) -> Rgb8Pixel {
    let sampled = code / 5;
    let overlap = code % 5;
    let line = if grid { 22 } else { 0 };
    Rgb8Pixel::new(
        11 + sampled * 9 + line,
        28 + sampled * 32 + overlap * 13 + line,
        40 + sampled * 26 + overlap * 18 + line,
    )
}

fn line(pixels: &mut [Rgb8Pixel], from: Vec2, to: Vec2) {
    let delta = to - from;
    let steps = delta.x.abs().max(delta.y.abs()).ceil().max(1.) as usize;
    for i in 0..=steps {
        let point = from + delta * (i as f32 / steps as f32);
        let index = point.y.round() as usize * SIDE + point.x.round() as usize;
        for offset in [0, 1, SIDE] {
            pixels[index + offset] = Rgb8Pixel::new(255, 193, 89);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polar_pixels_preserve_sensor_radius_and_bearing() {
        let index = |dx: i32, dy: i32| {
            let sample = (CENTER as i32 + dy) as usize * SIDE + (CENTER as i32 + dx) as usize;
            map_pixels()[sample]
                .map(|(cell, _)| (usize::from(cell) / SECTORS, usize::from(cell) % SECTORS))
        };
        // Forty image pixels are 16.13 body lengths, between 14.25 and 16.75.
        assert_eq!(index(40, 0), Some((16, 0)));
        assert_eq!(index(0, -40), Some((16, 16)));
        assert_eq!(index(-40, 0), Some((16, 32)));
        assert_eq!(index(0, 40), Some((16, 48)));
        assert_eq!(index(95, 0), None);
    }
    #[test]
    fn first_clock_decision_uses_live_controls_and_points_to_the_far_wall() {
        use scenario_clock::duck_training::{DuckCase, DuckDriver};
        let genome = C3Genome::random(1, 1);
        for entrance_right in [false, true] {
            let case = DuckCase {
                entrance_right,
                ..DuckCase::seeded(0)
            };
            let mut episode = DuckEpisode::new(case, DuckDriver::External).unwrap();
            let sensor = episode.sensor().unwrap();
            assert!(Decision::read(&episode, &sensor, &genome, 0).is_err());
            while !episode.controls_active()
                || !episode
                    .progress()
                    .tick
                    .is_multiple_of(u64::from(DECISION_TICKS))
            {
                assert!(episode.step(Default::default()).outcome.is_none());
            }
            let tick = episode.progress().tick;
            let decision = Decision::read(&episode, &sensor, &genome, tick).unwrap();
            let actions = genome
                .infer(&episode.observation(&sensor).unwrap())
                .unwrap();
            assert_eq!(
                decision.applied,
                DuckControls::decode(&actions, &BodyProfile::duck()).controls
            );
            assert_eq!(decision.outputs, actions);
            assert!(Decision::read(&episode, &sensor, &genome, tick + 1).is_err());

            let image = decision.polar_image().to_rgb8().unwrap();
            let arrow = Rgb8Pixel::new(255, 193, 89);
            let mut count = 0;
            for (i, pixel) in image.as_slice().iter().enumerate() {
                if *pixel == arrow {
                    assert_eq!((i % SIDE) < SIDE / 2, entrance_right);
                    count += 1;
                }
            }
            assert!(count > 20);
        }
    }
}
