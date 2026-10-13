//! Controlled aiming fixtures and same-state ordinary-combat comparisons.
//! Stationary fixtures fix tanks after settling; relocation comparisons clone
//! live duels. Both retain the actual shell, damage, and terrain simulation.
use super::*;
pub use crate::bot::comparison::compare_relocation;

#[derive(Debug, Clone, Copy)]
pub enum AimCase {
    Flat,
    Uphill,
    Downhill,
    Ridge,
}

pub fn stationary_case(shape: GrainShape, case: AimCase, policy: CpuPolicy) -> ScorchedState {
    let floor = |x: f32| match case {
        AimCase::Uphill if x > 0.0 => 1.0,
        AimCase::Downhill if x < 0.0 => 1.0,
        AimCase::Ridge if x.abs() < 3.0 => 12.0,
        _ => -5.0,
    };
    let mut state = ScorchedState::new(
        ScorchedConfig {
            shape,
            demo: true,
            cpu_driving: false,
            cpu_policy: [policy; 2],
            ..Default::default()
        },
        42,
    );
    state.physics.remove_entity(GROUND);
    let terrain = Terrain::generate(
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
            if y <= floor(x) {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    state.initial_cells = terrain
        .cells()
        .iter()
        .filter(|c| c.material != MaterialId::VOID)
        .count() as u64;
    let geometry = TerrainGeometry::with_surface(&terrain, TerrainSurface::Interpolated);
    let assembly = TerrainAssembly::insert(
        &mut state.physics,
        GROUND,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec {
            surface: TerrainSurface::Interpolated,
            ..Default::default()
        },
    )
    .unwrap();
    state.terrain = vec![TerrainFragment {
        id: GROUND,
        hash: terrain.hash(),
        terrain,
        geometry,
        assembly,
        edited_chunks: Vec::new(),
    }];
    for (i, x) in [-20.0, 20.0].into_iter().enumerate() {
        let position = Vec2::new(x, floor(x) + 1.5);
        state
            .physics
            .set_pose(state.tanks[i].body, position, 0.0, true);
        state
            .physics
            .set_velocity(state.tanks[i].body, Vec2::ZERO, 0.0, true);
        state.tanks[i].next_shot = u64::MAX;
        state.driving[i] = driving::TankDrive::new(position);
    }
    for _ in 0..90 {
        state.advance(0.0);
    }
    for tank in state.tanks {
        state
            .physics
            .set_body_kind(tank.body, BodyKind::Fixed, true);
    }
    state.tanks[0].next_shot = state.tick;
    state
}
