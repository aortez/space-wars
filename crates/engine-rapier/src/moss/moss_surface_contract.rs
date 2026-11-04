use super::*;
use crate::{
    grass::{GrassEnvironment, detach_invalid},
    plant_light::{PlantLight, dim_room, full_sun},
    root_uptake::RootUptake,
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_core::{
    Vec2,
    lighting::{DirectionalSource, LightingPublisher},
    plants::{
        Environment, Limit, Observation, Plan, Plants, Role, Tissue,
        grass::{self, Grass},
        moss::{self, Moss},
    },
};
use engine_terrain::{Brush, EditMode, Material, MaterialId, TerrainEdit, TerrainSurface};

const DIRT: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);

/// One support with its own terrain, canonical body and light publisher.
struct Bed {
    world: PhysicsWorld,
    terrain: Terrain,
    geometry: TerrainGeometry,
    assembly: TerrainAssembly,
    publisher: LightingPublisher,
}
impl Bed {
    fn new(width: u32, height: u32, material: fn(CellCoord) -> MaterialId, at: Vec2) -> Self {
        Self::with_surface(width, height, material, at, TerrainSurface::Blocks)
    }
    fn with_surface(
        width: u32,
        height: u32,
        material: fn(CellCoord) -> MaterialId,
        at: Vec2,
        surface: TerrainSurface,
    ) -> Self {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let terrain = Terrain::generate(
            width,
            height,
            1.0,
            vec![
                Material {
                    id: DIRT,
                    hardness: 10,
                },
                Material {
                    id: ROCK,
                    hardness: 40,
                },
            ],
            material,
        )
        .unwrap();
        let geometry = TerrainGeometry::with_surface(&terrain, surface);
        let assembly = TerrainAssembly::insert(
            &mut world,
            PhysicsId::new(1),
            BodySpec {
                kind: BodyKind::Fixed,
                position: at,
                ..Default::default()
            },
            &terrain,
            &geometry,
            TerrainSpec {
                surface,
                ..TerrainSpec::default()
            },
        )
        .unwrap();
        world.step(1.0 / 60.0);
        Self {
            world,
            terrain,
            geometry,
            assembly,
            publisher: LightingPublisher::default(),
        }
    }
    /// Dirt below y = 4 and a two-by-two rock pillar standing on it at x 0..2.
    fn garden() -> Self {
        Self::new(
            16,
            8,
            |c| {
                if c.y <= 3 {
                    DIRT
                } else if c.x <= 1 && c.y <= 5 {
                    ROCK
                } else {
                    MaterialId::VOID
                }
            },
            Vec2::ZERO,
        )
    }
    /// A free-standing rock with a stepped, rounded top inside a one-cell air
    /// margin, so moss has room on every face.
    fn rock() -> Self {
        Self::new(
            7,
            5,
            |c| {
                let body = (1..=5).contains(&c.x) && (1..=3).contains(&c.y);
                let corner = c.y == 3 && (c.x == 1 || c.x == 5);
                if body && !corner {
                    ROCK
                } else {
                    MaterialId::VOID
                }
            },
            Vec2::new(12.5, -2.0),
        )
    }
    fn roots(&self) -> RootUptake<'_> {
        RootUptake {
            world: &self.world,
            assembly: &self.assembly,
            terrain: &self.terrain,
            geometry: &self.geometry,
            dirt: DIRT,
            rates: [1000; 2],
            availability: [1000; 2],
            tick_ms: 1000,
        }
    }
    /// One complete host boundary: publish light for this support's whole
    /// tissue set once, plan every policy against it, then tick once.
    fn tick(
        &mut self,
        plants: &mut Plants,
        grass: &[Grass],
        moss: &[Moss],
        source: DirectionalSource,
    ) -> (Vec<Plan>, Vec<(Tissue, engine_core::lighting::Exposure)>) {
        let Self {
            world,
            terrain,
            geometry,
            assembly,
            publisher,
        } = self;
        let roots = RootUptake {
            world,
            assembly,
            terrain,
            geometry,
            dirt: DIRT,
            rates: [1000; 2],
            availability: [1000; 2],
            tick_ms: 1000,
        };
        let o = plants.observe();
        let light = PlantLight::publish(
            publisher,
            o.tick,
            o.tick,
            source,
            &roots,
            &o.tissues,
            u64::MAX,
        )
        .unwrap();
        assert!(light.work.rays <= 256);
        let samples = o
            .tissues
            .iter()
            .filter(|t| t.role == Role::Moss && t.health > 0)
            .map(|t| (t.clone(), light.diagnostic(t, o.tick).unwrap().clone()))
            .collect();
        let env = GrassEnvironment {
            roots: &roots,
            physiology: light.environment(o.tick).unwrap(),
        };
        let mut plans = Vec::new();
        let mut intents = Vec::new();
        for plan in grass
            .iter()
            .map(|g| g.plan(&o, &env))
            .chain(moss.iter().map(|m| m.plan(&o, &env)))
        {
            assert!(plan.decisions.len() <= 4 * o.tissues.len());
            intents.extend(plan.intents.iter().cloned());
            plans.push(plan);
        }
        plants.tick(o.tick, false, 1, &intents, &env).unwrap();
        (plans, samples)
    }
    fn pose(&mut self, position: Vec2, angle: f32) {
        assert!(
            self.world
                .set_pose(self.assembly.body(), position, angle, true)
        );
        self.world.step(1.0 / 60.0);
    }
}

fn dark() -> DirectionalSource {
    DirectionalSource {
        strength: 0.0,
        ..full_sun(Vec2::Y)
    }
}
fn living(o: &Observation, role: Role) -> usize {
    o.tissues
        .iter()
        .filter(|t| t.role == role && t.health > 0)
        .count()
}
fn assert_conserved(before: &Observation, after: &Observation) {
    for i in 0..3 {
        assert_eq!(
            before.tissues.iter().map(|t| t.stores[i]).sum::<u64>() + after.ledger.intake[i],
            after.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                + after.ledger.maintenance[i]
                + after.ledger.growth[i]
                + after.ledger.loss[i]
        );
    }
}
fn placed(bed: &Bed, origin: [i32; 2]) -> (Plants, Moss) {
    let mut plants = Plants::new(grass::physiology(), 42).unwrap();
    let moss = Moss::new(
        moss::Traits::default(),
        origin,
        bed.assembly.plant_support(),
    )
    .unwrap();
    let roots = bed.roots();
    moss.place(
        &mut plants,
        &GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        },
    )
    .unwrap();
    (plants, moss)
}

#[test]
fn surface_contact_accepts_any_solid_and_keeps_roots_on_dirt() {
    let bed = Bed::garden();
    let roots = bed.roots();
    let (plants, _) = placed(&bed, [0, 6]);
    let tissue = plants.observe().tissues[0].clone();
    let at = |cell: [i32; 2], role: Role| Tissue {
        cell,
        role,
        ..tissue.clone()
    };
    for (cell, reason) in [
        ([8, 4], ContactReason::Eligible),
        // Pillar top: rock contact only, with the nearest dirt three cells below.
        ([0, 6], ContactReason::Eligible),
        // Pillar side and its convex corner.
        ([2, 5], ContactReason::Eligible),
        ([2, 6], ContactReason::Eligible),
        ([8, 6], ContactReason::Air),
        ([0, 7], ContactReason::Air),
        ([8, 3], ContactReason::Enclosed),
        ([0, 4], ContactReason::Enclosed),
    ] {
        assert_eq!(roots.contact(&at(cell, Role::Moss)), reason, "{cell:?}");
    }
    assert_eq!(
        roots.contact(&Tissue {
            health: 0,
            ..at([8, 4], Role::Moss)
        }),
        ContactReason::Dead
    );
    assert_eq!(
        roots.contact(&at([8, 4], Role::Shoot)),
        ContactReason::NotAbsorbing
    );
    // Ordinary roots still reject rock.
    assert_eq!(
        roots.contact(&at([0, 4], Role::Root)),
        ContactReason::Ineligible
    );
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    assert!(env.space_available([0, 6], Role::Moss, 1));
    assert!(!env.space_available([8, 6], Role::Moss, 1));
    assert!(!env.space_available([-1, 4], Role::Moss, 1));
    assert!(!env.space_available([8, 3], Role::Moss, 1));
    let exposure = roots.exposure(&tissue);
    assert!(exposure.surface_contact && !exposure.dirt_contact);

    let mut plants = plants;
    for _ in 0..3 {
        plants
            .tick(plants.observe().tick, false, 1, &[], &roots)
            .unwrap();
    }
    let o = plants.observe();
    assert!(o.ledger.roots.is_empty());
    let surface = &o.ledger.surfaces[&tissue.id];
    assert!(surface.eligible);
    assert_eq!(surface.offered, [2, 2, 0]);
    assert_eq!(o.ledger.intake[..2], [2, 2]);
}

#[test]
fn dim_moss_spreads_across_dirt_and_rock_by_paid_contiguous_growth() {
    let run = || {
        let mut bed = Bed::garden();
        let (mut plants, moss) = placed(&bed, [3, 4]);
        let mut history = Vec::new();
        for _ in 0..240 {
            let before = plants.observe();
            let (plans, samples) = bed.tick(
                &mut plants,
                &[],
                std::slice::from_ref(&moss),
                dim_room(Vec2::Y),
            );
            let after = plants.observe();
            assert_conserved(&before, &after);
            assert!(plans[0].intents.len() <= 1);
            for (t, exposure) in samples {
                assert_eq!(exposure.intensity, 50.0, "unexpected shade at {:?}", t.cell);
            }
            history.push((after, plans));
        }
        history
    };
    let history = run();
    assert_eq!(history, run());
    let end = &history.last().unwrap().0;
    // Fourteen dirt-surface cells plus the pillar's side, corner and top.
    assert_eq!(living(end, Role::Moss), 18);
    assert!(end.tissues.iter().all(|t| t.health == 100));
    let filled = history
        .iter()
        .position(|(o, _)| o.tissues.len() == 18)
        .unwrap()
        + 1;
    println!("dim moss filled 18 garden surface cells at second {filled}");
    for cell in [[0, 6], [1, 6], [2, 6], [2, 5]] {
        assert!(end.tissues.iter().any(|t| t.cell == cell), "{cell:?}");
    }
    let growth: u64 = history.iter().map(|(o, _)| o.ledger.growth[0]).sum();
    assert_eq!(growth, 17 * 20);
    for (a, b) in &end.connections {
        let a = end.tissues.iter().find(|t| t.id == *a).unwrap();
        let b = end.tissues.iter().find(|t| t.id == *b).unwrap();
        assert_eq!(
            (a.cell[0] - b.cell[0]).abs() + (a.cell[1] - b.cell[1]).abs(),
            1
        );
    }
    assert_eq!(end.connections.len(), 17);
    // Space refusals, not resources, stop the patch once the surface is full.
    let last = &history.last().unwrap().1[0];
    assert!(
        last.decisions
            .iter()
            .all(|d| matches!(d.limit, Limit::Occupied | Limit::Space))
            || last.waiting.is_some()
    );
}

#[test]
fn dim_light_supports_moss_while_the_same_soil_starves_grass() {
    let outcome = |source: DirectionalSource| {
        let mut bed = Bed::garden();
        let (mut plants, moss) = placed(&bed, [3, 4]);
        let grass = Grass::new(
            grass::Traits::default(),
            [8, 3],
            [0, 1],
            bed.assembly.plant_support(),
        )
        .unwrap();
        let roots = bed.roots();
        grass
            .plant(
                &mut plants,
                &GrassEnvironment {
                    roots: &roots,
                    physiology: &roots,
                },
            )
            .unwrap();
        let mut unmet_energy = 0;
        for _ in 0..200 {
            let before = plants.observe();
            bed.tick(
                &mut plants,
                std::slice::from_ref(&grass),
                std::slice::from_ref(&moss),
                source,
            );
            let after = plants.observe();
            assert_conserved(&before, &after);
            unmet_energy += after.ledger.unmet[2];
        }
        let o = plants.observe();
        let species = |policy| {
            o.tissues
                .iter()
                .filter(|t| t.policy == policy && t.health > 0)
                .count()
        };
        (
            species(grass::POLICY),
            species(moss::POLICY),
            unmet_energy,
            o,
        )
    };
    let (sun_grass, sun_moss, _, sun) = outcome(full_sun(Vec2::Y));
    let (dim_grass, dim_moss, dim_unmet, dim) = outcome(dim_room(Vec2::Y));
    let (dark_grass, dark_moss, _, dark) = outcome(dark());
    println!(
        "200s living tissue: sun grass={sun_grass} moss={sun_moss}; dim grass={dim_grass} moss={dim_moss}; dark grass={dark_grass} moss={dark_moss}"
    );
    assert!(sun_grass > 8 && sun_moss > 4);
    assert!(sun.tissues.iter().all(|t| t.health == 100));
    // Dim light: grass shoots are below their threshold and earn no energy,
    // while the same exposure pays for moss upkeep and growth.
    assert_eq!(dim_grass, 0);
    assert!(dim_unmet > 0);
    assert!(dim_moss > 4);
    assert!(
        dim.tissues
            .iter()
            .filter(|t| t.policy == moss::POLICY)
            .all(|t| t.health == 100)
    );
    // Neither species is immune to darkness.
    assert_eq!((dark_grass, dark_moss), (0, 0));
    assert!(dark.tissues.iter().all(|t| t.health == 0));
}

#[test]
fn moving_and_rotating_support_carries_moss_and_its_light() {
    let mut bed = Bed::rock();
    let (mut plants, moss) = placed(&bed, [3, 4]);
    for _ in 0..160 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            full_sun(Vec2::Y),
        );
    }
    let grown = plants.observe();
    let lit: Vec<_> = grown
        .tissues
        .iter()
        .filter(|t| t.health > 0 && t.cell[1] == 4)
        .map(|t| t.id)
        .collect();
    assert!(lit.len() >= 5, "top row not colonized: {grown:?}");
    // Edge cells purchased beneath the rock are shaded by their own support,
    // and shaded parents do not spend on spreading further under it.
    let under: Vec<_> = grown
        .tissues
        .iter()
        .filter(|t| t.cell[1] == 0 && (1..=5).contains(&t.cell[0]))
        .collect();
    assert!(!under.is_empty());
    assert!(under.iter().all(|t| t.shade_ticks > 0 && t.health < 100));
    assert!(under.iter().all(|t| t.cell[0] == 1 || t.cell[0] == 5));

    bed.pose(Vec2::new(14.0, 1.5), 0.0);
    let (_, samples) = bed.tick(
        &mut plants,
        &[],
        std::slice::from_ref(&moss),
        full_sun(Vec2::Y),
    );
    for (t, exposure) in &samples {
        if lit.contains(&t.id) {
            assert_eq!(exposure.intensity, 1000.0);
        }
        assert_eq!(bed.roots().contact(t), ContactReason::Eligible);
    }
    let moved = plants.observe();
    for id in &lit {
        let before = grown.tissues.iter().find(|t| t.id == *id).unwrap();
        let after = moved.tissues.iter().find(|t| t.id == *id).unwrap();
        assert_eq!((after.cell, after.support), (before.cell, before.support));
        assert!(moved.ledger.surfaces[id].eligible);
    }

    // Upside down, the former top row is beneath the rock it is attached to.
    bed.pose(Vec2::new(14.0, 1.5), std::f32::consts::PI);
    let (_, samples) = bed.tick(
        &mut plants,
        &[],
        std::slice::from_ref(&moss),
        full_sun(Vec2::Y),
    );
    for (t, exposure) in &samples {
        if lit.contains(&t.id) {
            assert_eq!(exposure.intensity, 0.0, "{:?}", t.cell);
            assert!(exposure.first_blocker.is_some());
        }
        assert_eq!(bed.roots().contact(t), ContactReason::Eligible);
    }
    let config = plants.observe().config;
    let deadline = config.grace_ticks + 100 / u32::from(config.damage_per_tick) + 2;
    for _ in 0..deadline {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            full_sun(Vec2::Y),
        );
    }
    let flipped = plants.observe();
    for id in &lit {
        let t = flipped.tissues.iter().find(|t| t.id == *id).unwrap();
        assert_eq!(t.health, 0, "inverted moss survived: {t:?}");
    }
    // The newly upward faces stay alive; nothing is transferred or revived.
    assert!(
        flipped
            .tissues
            .iter()
            .any(|t| t.health > 0 && !lit.contains(&t.id))
    );
}

#[test]
fn breakage_detaches_unsupported_moss_and_support_loss_removes_the_patch() {
    let mut bed = Bed::rock();
    let (mut plants, moss) = placed(&bed, [3, 4]);
    for _ in 0..160 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            full_sun(Vec2::Y),
        );
    }
    let grown = plants.observe();
    let right: Vec<_> = grown
        .tissues
        .iter()
        .filter(|t| t.cell[0] == 6 && (1..=3).contains(&t.cell[1]))
        .map(|t| t.id)
        .collect();
    assert!(!right.is_empty(), "right face not colonized: {grown:?}");

    bed.terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(5, 1),
                end: CellCoord::new(5, 3),
                radius: 0,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    // Unsynchronized geometry fails closed until the host refreshes it.
    assert_eq!(
        bed.roots().contact(&grown.tissues[0]),
        ContactReason::StaleGeometry
    );
    bed.geometry.refresh(&bed.terrain);
    let Bed {
        world,
        terrain,
        geometry,
        assembly,
        ..
    } = &mut bed;
    assembly.synchronize(world, terrain, geometry).unwrap();
    world.step(1.0 / 60.0);

    let roots = bed.roots();
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    let unsupported: Vec<_> = grown
        .tissues
        .iter()
        .filter(|t| !env.space_available(t.cell, t.role, t.size))
        .cloned()
        .collect();
    assert!(
        right
            .iter()
            .all(|id| unsupported.iter().any(|t| t.id == *id))
    );
    assert_eq!(
        detach_invalid(&mut plants, &env).unwrap(),
        unsupported.len()
    );
    let broken = plants.observe();
    // Detached shares are discarded, never copied onto surviving cells.
    for i in 0..3 {
        let before: u64 = grown.tissues.iter().map(|t| t.stores[i]).sum();
        let after: u64 = broken.tissues.iter().map(|t| t.stores[i]).sum();
        assert_eq!(before - after, broken.ledger.loss[i] - grown.ledger.loss[i]);
        assert!(before > after);
    }
    let survivors: Vec<_> = broken.tissues.iter().map(|t| t.id).collect();
    assert!(
        survivors
            .iter()
            .all(|id| grown.tissues.iter().any(|t| t.id == *id))
    );

    // Newly exposed rock is ordinary surface: survivors pay to recolonize it.
    for _ in 0..80 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            full_sun(Vec2::Y),
        );
    }
    let regrown = plants.observe();
    for cell in [[5, 1], [5, 2]] {
        let t = regrown
            .tissues
            .iter()
            .find(|t| t.cell == cell)
            .expect("exposed rock was not recolonized");
        assert!(t.health > 0 && !survivors.contains(&t.id));
    }

    let stores: u64 = regrown.tissues.iter().map(|t| t.stores[0]).sum();
    let loss = regrown.ledger.loss[0];
    bed.world.remove_entity(bed.assembly.body().entity);
    let roots = bed.roots();
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    assert_eq!(
        detach_invalid(&mut plants, &env).unwrap(),
        regrown.tissues.len()
    );
    let gone = plants.observe();
    assert!(gone.tissues.is_empty());
    assert_eq!(gone.ledger.loss[0], loss + stores);
    assert!(moss.plan(&gone, &env).decisions.is_empty());
}

#[test]
fn split_remnants_recover_and_regrow_only_by_paying() {
    let mut bed = Bed::garden();
    let (mut plants, moss) = placed(&bed, [8, 4]);
    for _ in 0..120 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            dim_room(Vec2::Y),
        );
    }
    let o = plants.observe();
    let bridge = o.tissues.iter().find(|t| t.cell == [8, 4]).unwrap().id;
    let bruised = o.tissues.iter().find(|t| t.cell == [11, 4]).unwrap().id;
    let total: u64 = o.tissues.iter().map(|t| t.stores[0]).sum();
    plants.damage(bridge, 100).unwrap();
    plants.damage(bruised, 40).unwrap();
    let split = plants.observe();
    let dead_share = o.tissues.iter().find(|t| t.id == bridge).unwrap().stores[0];
    assert_eq!(
        split.tissues.iter().map(|t| t.stores[0]).sum::<u64>(),
        total - dead_share
    );
    let plan = moss.plan(&split, &bed.roots());
    assert!(
        plan.decisions
            .iter()
            .any(|d| d.parent == bruised && d.limit == Limit::Damage)
    );
    // Remove the dead bridge as debris; both remnants may pay to reclaim it.
    plants.remove(bridge).unwrap();
    let mut regrown = None;
    for _ in 0..80 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            dim_room(Vec2::Y),
        );
        let o = plants.observe();
        if let Some(t) = o.tissues.iter().find(|t| t.cell == [8, 4]) {
            regrown = Some(t.clone());
            break;
        }
    }
    let regrown = regrown.expect("a remnant must reclaim the cleared cell");
    assert_ne!(regrown.id, bridge);
    assert_eq!(regrown.organism, split.tissues[0].organism);
    assert_eq!(plants.observe().ledger.growth, [20; 3]);
    for _ in 0..10 {
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            dim_room(Vec2::Y),
        );
    }
    let healed = plants.observe();

    assert_eq!(
        healed
            .tissues
            .iter()
            .find(|t| t.id == bruised)
            .unwrap()
            .health,
        100
    );
}

#[test]
fn contour_surfaces_carry_moss_around_a_round_rock() {
    let mut bed = Bed::with_surface(
        11,
        11,
        |c| {
            let (dx, dy) = (c.x - 5, c.y - 5);
            if dx * dx + dy * dy <= 9 {
                ROCK
            } else {
                MaterialId::VOID
            }
        },
        Vec2::ZERO,
        TerrainSurface::Contour,
    );
    let (mut plants, moss) = placed(&bed, [5, 9]);
    for _ in 0..200 {
        let before = plants.observe();
        bed.tick(
            &mut plants,
            &[],
            std::slice::from_ref(&moss),
            full_sun(Vec2::Y),
        );
        assert_conserved(&before, &plants.observe());
    }
    let o = plants.observe();
    let roots = bed.roots();
    let living: Vec<_> = o.tissues.iter().filter(|t| t.health > 0).collect();
    for t in &living {
        assert_eq!(roots.contact(t), ContactReason::Eligible, "{:?}", t.cell);
    }
    // The patch follows the curve down both flanks, beyond the flat crown.
    assert!(living.iter().any(|t| t.cell[0] <= 2 && t.cell[1] <= 6));
    assert!(living.iter().any(|t| t.cell[0] >= 8 && t.cell[1] <= 6));
    assert!(living.len() >= 12, "{living:?}");
}
