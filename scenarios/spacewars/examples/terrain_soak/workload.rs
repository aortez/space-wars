use clap::ValueEnum;
use engine_common::{Action, Scenario};
use engine_core::{
    SpacewarsConfig, Vec2,
    rng::{SpacewarsRng, random_unit_f32, seeded_rng},
};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, Terrain, TerrainEdit};
use scenario_spacewars::{DebrisState, SpacewarsAction, SpacewarsScenario, SpacewarsState};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Case {
    Cannon,
    Excavation,
    Fragments,
    MultiPlanet,
}

impl Case {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cannon => "cannon",
            Self::Excavation => "excavation",
            Self::Fragments => "fragments",
            Self::MultiPlanet => "multi-planet",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Cannon => "One planet; an aimed shell every two seconds, throughout the run.",
            Self::Excavation => {
                "One planet; thin seeded cuts every ten seconds, plus sustained shells."
            }
            Self::Fragments => {
                "One planet; a grid cut at five seconds, then sustained shells into surviving pieces."
            }
            Self::MultiPlanet => {
                "Six material planets; a seeded cut every fifteen seconds, plus sustained shells."
            }
        }
    }
}

pub struct Workload {
    pub case: Case,
    rng: SpacewarsRng,
    pub injected_shells: u64,
    pub cuts: u64,
    pub released_victories: u64,
}

impl Workload {
    pub fn new(case: Case, seed: u64) -> (Self, SpacewarsState) {
        let state = if case == Case::MultiPlanet {
            let mut state = SpacewarsScenario::init(
                SpacewarsConfig {
                    universe_radius: 1500,
                    asteroid_probability_per_sec: 0.0,
                    use_starfield: false,
                    ..SpacewarsConfig::default()
                },
                seed,
            );
            for index in 0..state.planets.len() {
                state
                    .enable_planet_terrain(index)
                    .expect("bounded material planet");
            }
            state
        } else {
            SpacewarsScenario::init_terrain_fixture(seed)
        };
        (
            Self {
                case,
                rng: seeded_rng(seed ^ 0x534f414b),
                injected_shells: 0,
                cuts: 0,
                released_victories: 0,
            },
            state,
        )
    }

    /// Scheduled interventions are separate from gameplay time and are logged.
    /// No actor is made immortal and no terrain/fragments are reset or culled.
    pub fn prepare(&mut self, state: &mut SpacewarsState) -> Vec<String> {
        let mut events = Vec::new();
        if state.winner.take().is_some() {
            self.released_victories += 1;
            if self.released_victories == 1 {
                events.push(
                    "Bypassing match victory pauses so the endurance simulation keeps advancing"
                        .into(),
                );
            }
        }
        let tick = state.tick;
        if tick == 300 && self.case == Case::Fragments {
            let side = state.planet_terrain(0).unwrap().width() as i32;
            for line in (12..side).step_by(12) {
                for (start, end) in [
                    (CellCoord::new(line, 0), CellCoord::new(line, side)),
                    (CellCoord::new(0, line), CellCoord::new(side, line)),
                ] {
                    state
                        .queue_planet_edit(
                            0,
                            TerrainEdit {
                                brush: Brush::Capsule {
                                    start,
                                    end,
                                    radius: 0,
                                },
                                mode: EditMode::Remove,
                            },
                        )
                        .unwrap();
                    self.cuts += 1;
                }
            }
            events.push("Queued grid cut across planet 1 (12-cell spacing)".into());
        }
        let cut_period = match self.case {
            Case::Excavation => Some(600),
            Case::MultiPlanet => Some(900),
            _ => None,
        };
        if cut_period.is_some_and(|period| tick >= 300 && (tick - 300).is_multiple_of(period)) {
            let candidates = (0..state.planets.len())
                .filter(|&i| {
                    state
                        .planet_terrain(i)
                        .is_some_and(|f| f.cells().iter().any(|c| c.material != MaterialId::VOID))
                })
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                let index = candidates[self.pick(candidates.len())];
                let field = state.planet_terrain(index).unwrap();
                let center = occupied_cell(field, random_unit_f32(&mut self.rng)).unwrap();
                let angle = random_unit_f32(&mut self.rng) * std::f32::consts::TAU;
                let length = field.width().max(field.height()) as f32;
                let dx = (angle.cos() * length) as i32;
                let dy = (angle.sin() * length) as i32;
                state
                    .queue_planet_edit(
                        index,
                        TerrainEdit {
                            brush: Brush::Capsule {
                                start: CellCoord::new(center.x - dx, center.y - dy),
                                end: CellCoord::new(center.x + dx, center.y + dy),
                                radius: 1,
                            },
                            mode: EditMode::Remove,
                        },
                    )
                    .unwrap();
                self.cuts += 1;
                events.push(format!(
                    "Queued thin cut across planet {} through cell {},{}",
                    index + 1,
                    center.x,
                    center.y
                ));
            }
        }
        if tick >= 60 && (tick - 60).is_multiple_of(120) {
            let targets = (0..state.planets.len())
                .filter_map(|i| {
                    state.planet_terrain(i).map(|field| {
                        let planet = state.planets[i];
                        let offset = state
                            .sun
                            .map_or(Vec2::ZERO, |sun| planet.position - sun.position);
                        (
                            format!("planet {}", i + 1),
                            field,
                            planet.position,
                            planet.wrapper_angle,
                            Vec2::new(-offset.y, offset.x) * planet.orbit_omega,
                        )
                    })
                })
                .chain(state.terrain_fragments().filter_map(|fragment| {
                    state.terrain_fragment_motion(fragment.id()).map(|motion| {
                        (
                            format!("fragment {}", fragment.id().value()),
                            fragment.terrain(),
                            motion.position,
                            motion.angle,
                            motion.linear_velocity,
                        )
                    })
                }))
                .filter(|(_, field, ..)| {
                    field.cells().iter().any(|c| c.material != MaterialId::VOID)
                })
                .collect::<Vec<_>>();
            if !targets.is_empty() {
                let (name, field, position, angle, velocity) = &targets[self.pick(targets.len())];
                let cell = occupied_cell(field, random_unit_f32(&mut self.rng)).unwrap();
                let target = *position + field.cell_center(cell).rotate_radians(*angle);
                let theta = random_unit_f32(&mut self.rng) * std::f32::consts::TAU;
                let direction = Vec2::new(theta.cos(), theta.sin());
                let distance =
                    (field.width() as f32).hypot(field.height() as f32) * field.cell_size() + 12.0;
                let shell = DebrisState::new_shell(
                    0,
                    tick,
                    target - direction * distance,
                    *velocity + direction * 400.0,
                    theta,
                );
                let message = format!("Injected shell aimed at {name}, cell {},{}", cell.x, cell.y);
                state.debris.push(shell);
                self.injected_shells += 1;
                events.push(message);
            } else {
                events.push("No material remains for the scheduled shell".into());
            }
        }
        events
    }

    pub fn actions(&self) -> [Action; 2] {
        [
            SpacewarsAction::set_cannon(0, true),
            SpacewarsAction::set_laser(1, true),
        ]
    }

    fn pick(&mut self, count: usize) -> usize {
        (random_unit_f32(&mut self.rng) * count as f32) as usize
    }
}

fn occupied_cell(field: &Terrain, fraction: f32) -> Option<CellCoord> {
    let count = field
        .cells()
        .iter()
        .filter(|c| c.material != MaterialId::VOID)
        .count();
    let nth = (fraction * count as f32) as usize;
    let index = field
        .cells()
        .iter()
        .enumerate()
        .filter(|(_, c)| c.material != MaterialId::VOID)
        .nth(nth)?
        .0 as u32;
    Some(CellCoord::new(
        (index % field.width()) as i32,
        (index / field.width()) as i32,
    ))
}
