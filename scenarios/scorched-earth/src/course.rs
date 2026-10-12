use super::*;
use engine_rapier::terrain::{PreparedRelease, TerrainBodyMut};
use engine_terrain::{Brush, EditMode, TerrainEdit};

pub(super) const START_X: f32 = -27.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrivingCourse {
    Flat,
    Step,
    Slope,
    Trench,
    Rubble,
}

impl DrivingCourse {
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::Step,
        Self::Slope,
        Self::Trench,
        Self::Rubble,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::Step => "Step",
            Self::Slope => "Slope",
            Self::Trench => "Trench",
            Self::Rubble => "Rubble",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Flat => "Flat baseline",
            Self::Step => "0.5-unit step",
            Self::Slope => "20-degree slope",
            Self::Trench => "2-wide, 2-deep trench",
            Self::Rubble => "Released dirt pile",
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            Self::Flat => Some(Self::Step),
            Self::Step => Some(Self::Slope),
            Self::Slope => Some(Self::Trench),
            Self::Trench => Some(Self::Rubble),
            Self::Rubble => None,
        }
    }

    /// The chassis must clear the obstacle by at least its half-width.
    pub fn finish_x(self) -> f32 {
        -8.0
    }

    pub(super) fn floor(self, x: f32) -> f32 {
        match self {
            Self::Step if x >= -20.0 => -4.5,
            Self::Slope => -5.0 + (x + 20.0).clamp(0.0, 8.0) * 20.0_f32.to_radians().tan(),
            Self::Trench if (-18.0..-16.0).contains(&x) => -7.0,
            _ => -5.0,
        }
    }

    pub(super) fn terrain(self) -> Terrain {
        Terrain::generate(
            193,
            129,
            CELL_SIZE,
            vec![Material {
                id: DIRT,
                hardness: 100,
            }],
            |c| {
                let x = (c.x as f32 + 0.5 - 193.0 * 0.5) * CELL_SIZE;
                let y = (c.y as f32 + 0.5 - 129.0 * 0.5) * CELL_SIZE;
                let pile = self == Self::Rubble
                    && (-20.0..=-12.0).contains(&x)
                    && y <= -3.0 - (x + 16.0).abs() * 0.35;
                if y <= self.floor(x) || pile {
                    DIRT
                } else {
                    MaterialId::VOID
                }
            },
        )
        .expect("bounded driving course")
    }
}

impl ScorchedState {
    pub(super) fn release_course_rubble(&mut self) {
        let body = &mut self.terrain[0];
        let edits: Vec<_> = (0..body.terrain.height())
            .flat_map(|y| {
                (0..body.terrain.width()).map(move |x| engine_terrain::CellCoord {
                    x: x as i32,
                    y: y as i32,
                })
            })
            .filter(|&c| {
                body.terrain.cell_center(c).y > -5.0
                    && body
                        .terrain
                        .cell(c)
                        .is_some_and(|cell| cell.material != MaterialId::VOID)
            })
            .map(|center| TerrainEdit {
                brush: Brush::Circle { center, radius: 0 },
                mode: EditMode::Remove,
            })
            .collect();
        let plan = PreparedRelease::new(&body.terrain, &edits).expect("bounded rubble release");
        // Keep the pool limit and the whole source when it cannot admit the pile.
        if !self.loose.release_capacity(plan.grain_count()).fits() {
            self.status = "Rubble course needs more loose-dirt capacity; pile retained.";
            return;
        }
        let release = self
            .loose
            .commit(
                &mut self.physics,
                TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                },
                plan,
                &mut self.next_id,
            )
            .expect("admitted rubble release");
        body.hash = body.terrain.hash();
        body.edited_chunks = release.dirty_chunks;
        self.terrain.extend(release.fragments);
    }
}

#[cfg(test)]
mod tests;
