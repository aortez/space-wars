use engine_core::Vec2;
use engine_rapier::world::{PhysicsId, RayCastOptions};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit};

use crate::{
    EditCause, FIXED_HZ, MiningTool, ORE, PendingEdit, ROCK, SPACELING_ID, TerrainLabState,
};

pub const DRILL_RANGE: f32 = 4.0;
pub const DRILL_RADIUS: f32 = 1.0;
pub const DRILL_DAMAGE: u8 = 20;
pub const DRILL_INTERVAL_TICKS: u32 = FIXED_HZ / 10;
const AIM_RADIANS_PER_SECOND: f32 = 2.4;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MiningControls {
    pub held: bool,
    /// Signed rotation of the character-relative aim, in [-1, 1].
    pub turn: f32,
    /// World-space stick direction. Zero retains the character-relative aim.
    pub aim: Vec2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MiningInventory {
    pub rock_cells: u64,
    pub ore_cells: u64,
}

impl MiningInventory {
    pub fn cells(self, material: MaterialId) -> u64 {
        match material {
            ROCK => self.rock_cells,
            ORE => self.ore_cells,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiningTarget {
    /// Stable owner of the cell and brush coordinates.
    pub body: PhysicsId,
    pub cell: CellCoord,
    pub material: MaterialId,
    pub durability: u8,
    pub hardness: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiningSnapshot {
    pub active: bool,
    pub origin: Vec2,
    pub end: Vec2,
    pub target: Option<MiningTarget>,
    /// The exact quantized edit shared by the cut preview and the next pulse.
    pub brush: Option<Brush>,
}

#[derive(Clone)]
pub(super) struct MiningState {
    pub tool: MiningTool,
    pub controls: MiningControls,
    pub angle: f32,
    pub pointer: Option<Vec2>,
    pub cooldown: u32,
}

impl Default for MiningState {
    fn default() -> Self {
        Self {
            tool: MiningTool::default(),
            controls: MiningControls::default(),
            angle: -std::f32::consts::FRAC_PI_2,
            pointer: None,
            cooldown: 0,
        }
    }
}

impl TerrainLabState {
    /// Lab recovery policy: all drill-removed material is collected. Report area
    /// so changing cell resolution does not change the value of a physical cut.
    pub fn recovered_area(&self, material: MaterialId) -> f64 {
        self.recovered.cells(material) as f64 * f64::from(self.terrain.cell_size()).powi(2)
    }

    /// Read the same post-step collision world used by character contacts.
    pub fn mining_snapshot(&self) -> MiningSnapshot {
        let profile = self.tool_profile();
        let character = self.spaceling_snapshot();
        let origin = character.motion.position;
        let direction = Vec2::from_radians(character.motion.angle + self.mining.angle);
        let hit = self.physics.cast_ray(
            origin,
            direction,
            RayCastOptions {
                max_distance: profile.range,
                exclude_entity: Some(SPACELING_ID),
                ..Default::default()
            },
        );
        let target = hit.and_then(|hit| {
            let body = hit.collider.entity;
            let (terrain, motion) = self.terrain_body(body)?;
            // A hit lies on a cell boundary. Step just inside its solid face
            // before quantizing, including on rotated planets and grazing rays.
            let inside = hit.point - hit.normal * (terrain.cell_size() * 0.001);
            let cell =
                terrain.local_to_cell((inside - motion.position).rotate_radians(-motion.angle))?;
            let material = terrain.cell(cell)?;
            if material.material == MaterialId::VOID {
                return None;
            }
            let hardness = terrain.material(material.material)?.hardness;
            Some(MiningTarget {
                body,
                cell,
                material: material.material,
                durability: material.durability,
                hardness,
            })
        });
        MiningSnapshot {
            active: self.mining.controls.held || self.mining.pointer.is_some(),
            origin,
            end: hit.map_or(origin + direction * profile.range, |hit| hit.point),
            target,
            brush: target.map(|target| {
                let (terrain, motion) = self
                    .terrain_body(target.body)
                    .expect("retained mining target");
                let cap_radius = (profile.cut_radius / terrain.cell_size()).floor() as u32;
                let half_span =
                    (profile.cut_half_width / terrain.cell_size()).floor() - cap_radius as f32;
                let across = Vec2::Y
                    .rotate_radians(character.motion.angle + self.mining.angle - motion.angle)
                    * half_span;
                let offset = CellCoord::new(across.x.round() as i32, across.y.round() as i32);
                Brush::Capsule {
                    start: CellCoord::new(target.cell.x - offset.x, target.cell.y - offset.y),
                    end: CellCoord::new(target.cell.x + offset.x, target.cell.y + offset.y),
                    radius: cap_radius,
                }
            }),
        }
    }

    pub(super) fn queue_mining(&mut self, dt: f32) {
        let character = self.spaceling_snapshot();
        let aim = self
            .mining
            .pointer
            .map_or(self.mining.controls.aim, |pointer| {
                pointer - character.motion.position
            });
        if aim.length_squared() > f32::EPSILON {
            self.mining.angle = aim.y.atan2(aim.x) - character.motion.angle;
        } else {
            self.mining.angle += self.mining.controls.turn * AIM_RADIANS_PER_SECOND * dt;
        }
        self.mining.angle = (self.mining.angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        // Cooldown continues on release, so tapping cannot accelerate mining.
        self.mining.cooldown = self.mining.cooldown.saturating_sub(1);
        let snapshot = self.mining_snapshot();
        if snapshot.active
            && self.mining.cooldown == 0
            && let Some(brush) = snapshot.brush
            && let Some(target) = snapshot.target
        {
            let profile = self.tool_profile();
            self.pending_edits.push(PendingEdit {
                body: target.body,
                edit: TerrainEdit {
                    brush,
                    mode: EditMode::Damage(profile.damage),
                },
                cause: EditCause::Mining,
            });
            self.mining.cooldown = profile.interval_ticks;
        }
    }
}

#[cfg(test)]
mod tests;
