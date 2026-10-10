//! Repeatable cannon encounter using the ordinary Spacewars material lifecycle.
use super::*;

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    Quarry,
    Curved,
    Roof,
}

impl SpacewarsScenario {
    /// A sloping quarry lip on a rotating planet. Fire ship zero's ordinary cannon to
    /// remove its pier; ship one begins over the lip to exercise actor support.
    pub fn init_destruction_fixture(seed: u64, mode: engine_common::DirtMode) -> SpacewarsState {
        Self::destruction_fixture(seed, mode, Scene::Quarry)
    }

    /// An uncut, uneven circular planet. Sustained ordinary cannon fire must
    /// create its own overhangs under changing radial gravity.
    pub fn init_curved_destruction_fixture(
        seed: u64,
        mode: engine_common::DirtMode,
    ) -> SpacewarsState {
        Self::destruction_fixture(seed, mode, Scene::Curved)
    }

    /// A cave roof on a rotating planet. The ordinary cannon thins its underside;
    /// the shared strength rule decides whether the surviving span cracks.
    pub fn init_roof_fixture(seed: u64, mode: engine_common::DirtMode) -> SpacewarsState {
        Self::destruction_fixture(seed, mode, Scene::Roof)
    }

    fn destruction_fixture(
        seed: u64,
        mode: engine_common::DirtMode,
        scene: Scene,
    ) -> SpacewarsState {
        let curved = scene == Scene::Curved;
        let roof = scene == Scene::Roof;
        let mut state = Self::init_terrain_fixture(seed);
        state.terrain.legacy_services = false;
        state.terrain.surface = TerrainSurface::Interpolated;
        let planet = state.terrain.planets.get_mut(&0).unwrap();
        planet.field = Terrain::generate(
            145,
            145,
            1.0,
            vec![Material {
                id: ROCK,
                hardness: 100,
            }],
            |c| {
                let x = c.x - 72;
                let y = c.y - 72;
                let filled = if roof {
                    x * x + y * y <= 35 * 35
                        || ((-22..=-8).contains(&x) && (0..=59).contains(&y))
                        || ((7..=22).contains(&x) && (0..=59).contains(&y))
                        || ((-7..=6).contains(&x)
                            && ((if x == -7 || x == 6 { 58 } else { 52 })..=59).contains(&y))
                } else if curved {
                    let angle = (y as f32).atan2(x as f32);
                    let radius = 58.0 + (angle * 5.0 + (seed % 1000) as f32 * 0.006).sin() * 8.0;
                    (x * x + y * y) as f32 <= radius * radius
                } else {
                    let underside = 57 + ((x + 9).max(0) / 4) % 2;
                    let bank_top = 60 - (x + 10).pow(2) / 32;
                    x * x + y * y <= 40 * 40
                        || ((-22..=-10).contains(&x) && (0..=bank_top).contains(&y))
                        || ((-9..=6).contains(&x) && (underside..underside + 4).contains(&y))
                        || (x == 6 && (0..=57).contains(&y))
                };
                if filled { ROCK } else { MaterialId::VOID }
            },
        )
        .expect("bounded quarry field");
        state.physics.world.remove_entity(physics::planet_entity(0));
        planet.geometry = TerrainGeometry::with_surface(&planet.field, state.terrain.surface);
        planet.assembly = TerrainAssembly::insert(
            &mut state.physics.world,
            physics::planet_entity(0),
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: state.planets[0].position,
                ..Default::default()
            },
            &planet.field,
            &planet.geometry,
            engine_rapier::terrain::TerrainSpec {
                surface: state.terrain.surface,
                ..physics::terrain_spec()
            },
        )
        .expect("quarry assembly");
        planet.hash = planet.field.hash();
        planet.footing = [CellCoord::new(72, 72); 3];
        planet.supported = true;
        state.physics.disable_spaceport(0);
        state.physics.material_queries_dirty = true;
        let center = state.planets[0].position;
        state.ships[0].position = center
            + if roof {
                Vec2::new(-2.0, 42.0)
            } else {
                Vec2::new(if curved { 60.0 } else { 45.0 }, 54.0)
            };
        state.ships[0].rotation_radians = core::f32::consts::FRAC_PI_2;
        state.ships[0].direction = -Vec2::X;
        if roof {
            state.ships[0].rotation_radians = 0.0;
            state.ships[0].direction = Vec2::Y;
        }
        state.ships[0].velocity = Vec2::ZERO;
        state.ships[1].position = center
            + if roof {
                Vec2::new(2.0, 65.0)
            } else if curved {
                Vec2::new(20.0, 60.0)
            } else {
                Vec2::new(-5.0, 65.0)
            };
        state.ships[1].rotation_radians = 0.0;
        state.ships[1].direction = Vec2::Y;
        state.ships[1].velocity = Vec2::ZERO;
        state
            .enable_loose_terrain(surface_sortie::dirt_config(mode))
            .expect("quarry dirt");
        state
    }
}
