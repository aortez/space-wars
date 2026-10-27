use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState,
    floor::responsive::{FloorShape, ResponsiveFloor},
    layout::Layout,
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use engine_core::plants::{Environment, Role};
use engine_rapier::{
    root_uptake::{ContactReason, MaterialUptake},
    world::*,
};
use std::time::Duration;

fn fixture() -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        215,
    );
    ClockScenario::step(
        &mut s,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    s.admit_soil();
    s.set_plant_conditions(1000, [1000; 2]);
    s.plant_grass(10).unwrap();
    s
}
fn seconds(s: &mut ClockState, n: usize) {
    for _ in 0..n * 60 {
        ClockScenario::step(s, &[], Duration::from_nanos(16_666_667));
    }
}
fn roots(soil: &SoilHabitat) -> MaterialUptake<'_> {
    MaterialUptake {
        terrain: soil.terrain(),
        geometry: soil.geometry(),
        dirt: CLOCK_DIRT,
        support: soil.material_support(),
        rates: [1000; 2],
        availability: [1000; 2],
        tick_ms: 1000,
    }
}

#[test]
fn clock_grass_contract_paid_persistent_biology_and_zero_duration() {
    let mut s = fixture();
    let initial = s.plants().plants.observe();
    assert_eq!(initial.tissues.len(), 1);
    let identity = (
        initial.tissues[0].id,
        initial.tissues[0].organism,
        initial.tissues[0].support,
    );
    ClockScenario::step(&mut s, &[], Duration::ZERO);
    assert_eq!(s.plants().plants.observe(), initial);
    seconds(&mut s, 10);
    let grown = s.plants().plants.observe();
    assert_eq!(grown.tick, 10);
    assert!(grown.tissues.len() > 1);
    assert!(grown.ledger.intake[0] > 0);
    assert!(grown.ledger.maintenance.iter().any(|x| *x > 0));
    assert!(grown.tissues.iter().any(|t| t.role == Role::Shoot));
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        Duration::ZERO,
    );
    seconds(&mut s, 80);
    ClockScenario::step(
        &mut s,
        &[ClockAction::set_reading(
            ClockReading::new(12, 35, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    let after = s.plants().plants.observe();
    let root = after.tissues.iter().find(|t| t.id == identity.0).unwrap();
    assert_eq!((root.id, root.organism, root.support), identity);
    assert_eq!(after.tick, 90);
    assert!(
        after
            .tissues
            .iter()
            .any(|t| t.invested.iter().any(|v| *v > 0))
    );
    assert_eq!(s.plants().admitted_at[&identity.1], 0);
}

#[test]
fn clock_grass_contract_equal_biological_time_preserves_paid_accounting() {
    let mut normal = fixture();
    let mut fast = fixture();
    fast.set_plant_playback(PlantPlayback::Preview16);
    seconds(&mut normal, 16);
    seconds(&mut fast, 1);
    assert_eq!(normal.plants().plants, fast.plants().plants);
    assert_eq!(normal.plants().pending_ns(), fast.plants().pending_ns());
    assert_eq!(normal.plants().exposure, fast.plants().exposure);
    assert_eq!(normal.plants().dropped_biology_ms, 0);
    assert_eq!(fast.plants().dropped_biology_ms, 0);
    assert_eq!(normal.schedule.tick, 960);
    assert_eq!(fast.schedule.tick, 60);
    // Catch-up is bounded and its discarded biological time is explicit.
    ClockScenario::step(&mut fast, &[], Duration::from_secs(100));
    assert_eq!(fast.plants().plants.observe().tick, 32);
    assert!(fast.plants().dropped_biology_ms > 0);
}

#[test]
fn clock_grass_contract_moving_dirt_keeps_contact_and_real_loss_stops_supply() {
    let mut s = fixture();
    let layout = Layout::new(s.aspect_ratio());
    let root = s.plants().plants.observe().tissues[0].clone();
    let soil = s.soil.as_mut().unwrap();
    let identity = soil.material_support();
    let old = soil.world_point(root.cell).unwrap();
    let mut floor = ResponsiveFloor::new(FloorShape::clock(layout), 0.0);
    floor.opening = 0.6;
    soil.set_surfaces(
        surfaces::responsive(layout, &floor),
        SoilSupportState::Responsive,
    );
    let moved = soil.world_point(root.cell).unwrap();
    assert!((moved - old).length() > 0.1);
    assert_eq!(soil.material_support(), identity);
    assert_eq!(roots(soil).contact(&root), ContactReason::Eligible);
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    floor.insert_panels(&mut world);
    soil.bind_current(&mut world);
    assert!(!world.current_cuboid_is_clear(
        moved,
        0.0,
        engine_core::Vec2::new(0.1, 0.1),
        CollisionGroups::ALL,
        &[]
    ));
    let before = soil.accounting();
    soil.set_surfaces(Vec::new(), SoilSupportState::Drain);
    assert_eq!(roots(soil).contact(&root), ContactReason::Eligible);
    let falling_start = soil.world_point(root.cell).unwrap();
    soil.advance_loss(Duration::from_millis(250));
    assert!(soil.world_point(root.cell).unwrap().y < falling_start.y);
    assert_eq!(roots(soil).contact(&root), ContactReason::Eligible);
    assert_eq!(soil.accounting(), before);
    let falling = soil.world_point(root.cell).unwrap();
    soil.set_surfaces(
        surfaces::ordinary(crate::floor::FloorGeometry::closed(layout)),
        SoilSupportState::Ordinary,
    );
    assert_eq!(soil.world_point(root.cell), Some(falling));
    for _ in 0..12 {
        soil.advance_loss(Duration::from_millis(250));
    }
    soil.bind_current(&mut world);
    assert_eq!(roots(soil).contact(&root), ContactReason::Air);
    assert!(!roots(soil).exposure(&root).dirt_contact);
    assert_eq!(soil.accounting().exported, before.added);
    s.plants.revalidate(s.soil.as_ref().unwrap());
    assert!(s.plants().plants.observe().tissues.is_empty());
    ClockScenario::step(&mut s, &[], Duration::ZERO);
    assert!(s.plants().plants.observe().tissues.is_empty());
    assert_eq!(s.soil().unwrap().accounting().retained, 0);
}

#[test]
fn clock_grass_contract_developed_shoots_do_not_return_after_drain_export() {
    fn visible_blades(s: &ClockState) -> usize {
        ClockScenario::render_frame(s)
            .layers
            .iter()
            .flat_map(|l| &l.primitives)
            .filter(|p| {
                matches!(p, engine_common::RenderPrimitive::Polygon(p)
                if p.fill.as_ref().is_some_and(|f|
                    f.color == engine_common::RenderColor::rgb(0.22, 0.65, 0.16)))
            })
            .count()
    }

    let mut s = fixture();
    // Grow the drained patch alone before adding an unchanged bank resident.
    s.plants = ClockPlants::new(215);
    s.set_plant_conditions(1000, [1000; 2]);
    s.plant_grass(200).unwrap();
    for _ in 0..240 {
        s.plants.advance(
            Duration::from_secs(1),
            s.soil.as_ref().unwrap(),
            None,
            &[],
            vec![],
        );
    }
    s.plant_grass(10).unwrap();
    let before = s.plants().plants.observe();
    let lost: Vec<_> = before
        .tissues
        .iter()
        .filter(|t| t.cell[0] > 100)
        .cloned()
        .collect();
    let bank: Vec<_> = before
        .tissues
        .iter()
        .filter(|t| t.cell[0] < 100)
        .cloned()
        .collect();
    assert!(
        lost.iter()
            .any(|t| t.role == Role::Shoot && (t.size > 1 || t.development[0] > 0)),
        "{lost:?}"
    );
    assert!(!bank.is_empty());
    assert!(visible_blades(&s) > 0);
    let layout = Layout::new(s.aspect_ratio());
    let soil = s.soil.as_mut().unwrap();
    let accounting = soil.accounting();
    let shoot = lost.iter().find(|t| t.role == Role::Shoot).unwrap();
    let original = soil.world_point(shoot.cell).unwrap();
    soil.set_surfaces(
        surfaces::ordinary(crate::floor::test_drain(layout).geometry()),
        SoilSupportState::Drain,
    );
    soil.advance_loss(Duration::from_millis(250));
    let falling = soil.world_point(shoot.cell).unwrap();
    assert!(falling.y < original.y);
    soil.set_surfaces(
        surfaces::ordinary(crate::floor::FloorGeometry::closed(layout)),
        SoilSupportState::Ordinary,
    );
    assert_eq!(soil.world_point(shoot.cell), Some(falling));
    s.plants.revalidate(s.soil.as_ref().unwrap());
    assert_eq!(s.plants().plants.observe(), before);
    for _ in 0..12 {
        s.soil
            .as_mut()
            .unwrap()
            .advance_loss(Duration::from_millis(250));
        s.plants.revalidate(s.soil.as_ref().unwrap());
    }
    let after = s.plants().plants.observe();
    assert_eq!(visible_blades(&s), 0);
    // No restored transform can publish the old blades above bare floor.
    for t in &lost {
        assert_eq!(s.soil().unwrap().world_point(t.cell), None);
        assert!(!after.tissues.iter().any(|a| a.id == t.id));
    }
    for t in &bank {
        assert_eq!(after.tissues.iter().find(|a| a.id == t.id), Some(t));
    }
    for resource in 0..3 {
        let reserves: u64 = lost.iter().map(|t| t.stores[resource]).sum();
        assert_eq!(
            after.ledger.loss[resource] - before.ledger.loss[resource],
            reserves
        );
    }
    let exported = s.soil().unwrap().accounting();
    assert!(exported.exported > 0 && exported.retained > 0);
    assert_eq!(exported.added, accounting.added);
    assert_eq!(exported.added, exported.retained as u64 + exported.exported);
    seconds(&mut s, 5);
    assert!(
        s.plants()
            .plants
            .observe()
            .tissues
            .iter()
            .all(|t| !lost.iter().any(|old| old.id == t.id))
    );
    assert_eq!(s.soil().unwrap().accounting(), exported);
    let refill = s.admit_soil();
    assert_eq!(refill.sites.len() as u64, exported.exported);
    assert!(
        s.plants()
            .plants
            .observe()
            .tissues
            .iter()
            .all(|t| !lost.iter().any(|old| old.id == t.id))
    );
    let new_seed = s.plant_grass(200).unwrap();
    assert!(!lost.iter().any(|t| t.id == new_seed));
    let current = s.plants().plants.observe();
    assert_eq!(
        current
            .tissues
            .iter()
            .find(|t| t.id == new_seed)
            .unwrap()
            .stores,
        SEED_STARTING_RESERVES
    );
}

#[test]
fn clock_grass_contract_all_floor_adapters_and_shared_arena_preserve_material_identity() {
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Explosion,
        ClockEventKind::Rain,
        ClockEventKind::Meltdown,
        ClockEventKind::Aquarium,
        ClockEventKind::Duck,
    ] {
        for visit_first in [false, true] {
            let mut s = fixture();
            let identity = s.soil().unwrap().material_support();
            if visit_first {
                ClockScenario::step(
                    &mut s,
                    &[ClockAction::toggle_player_duck(1)],
                    Duration::ZERO,
                );
            }
            ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
            if !visit_first {
                ClockScenario::step(
                    &mut s,
                    &[ClockAction::toggle_player_duck(1)],
                    Duration::ZERO,
                );
            }
            seconds(&mut s, 2);
            let soil = s.soil().unwrap();
            assert_eq!(
                soil.material_support(),
                identity,
                "{kind:?} visit_first={visit_first}"
            );
            assert_ne!(soil.support_state(), SoilSupportState::AwaitingAdapter);
            let a = soil.accounting();
            assert_eq!(a.added, a.retained as u64 + a.exported);
            assert_eq!(a.added, 1200);
            if let Some(world) = surfaces::world(&s.active_event, s.duck_visit.as_deref()) {
                assert!(
                    world.contains_entity(PhysicsId::new(SOIL_PHYSICS_ID)),
                    "{kind:?}"
                );
                assert!(
                    world
                        .collider_ids()
                        .filter(|id| id.entity.value() == SOIL_PHYSICS_ID)
                        .count()
                        <= MAX_SOIL_CHUNK_COLLIDERS
                );
            }
            assert_eq!(s.plants().plants.observe().tick, 2);
            if let Some(root) = s
                .plants()
                .plants
                .observe()
                .tissues
                .iter()
                .find(|t| t.id == 1)
            {
                assert_eq!(root.support, Some(identity));
            }
        }
    }
}

#[test]
fn clock_grass_contract_dim_light_and_shortage_are_real_biology() {
    let mut dim = fixture();
    let mut bright = fixture();
    assert_eq!(
        ClockScenario::init(ClockConfig::default(), 215)
            .plants()
            .intensity,
        50
    );
    dim.set_plant_conditions(50, [1000; 2]);
    seconds(&mut bright, 60);
    seconds(&mut dim, 60);
    assert_ne!(bright.plants().plants, dim.plants().plants);
    let mut dark = fixture();
    dark.set_plant_conditions(0, [0; 2]);
    seconds(&mut dark, 240);
    assert!(
        dark.plants()
            .plants
            .observe()
            .tissues
            .iter()
            .all(|t| t.health == 0)
    );
    assert!(
        bright
            .plants()
            .plants
            .observe()
            .tissues
            .iter()
            .any(|t| t.health > 0)
    );
    assert!(bright.plants().plants.observe().tissues.len() <= MAX_PLANT_TISSUES);
}

#[test]
fn clock_grass_contract_favorable_establishment_timing() {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            favorable_grass_fixture: true,
            ..Default::default()
        },
        215,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
            ClockAction::preview_event(ClockEventKind::Grass),
        ],
        Duration::from_nanos(16_666_667),
    );
    seconds(&mut s, 5);
    assert_eq!(s.grass_state().seeded, SEED_BATCH);
    let mut visible = None;
    let mut established = None;
    let mut history = Vec::new();
    for second in 1..=500 {
        seconds(&mut s, 1);
        let o = s.plants().plants.observe();
        history.push(serde_json::json!({"second":second,"state":s.grass_state()}));
        if visible.is_none()
            && o.tissues
                .iter()
                .any(|t| t.role == Role::Shoot && t.health > 0)
        {
            visible = Some(second);
        }
        if o.tissues
            .iter()
            .filter(|t| t.role == Role::Shoot && t.health > 0 && t.size >= 3)
            .count()
            >= 3
        {
            established = Some(second);
            break;
        }
    }
    println!(
        "favorable-v1 seed=215 column=10 intensity=1000 first-visible={visible:?} three-mature={established:?}"
    );
    assert!(visible.is_some_and(|s| s <= 5));
    // The batch's seeds are separate patches, each planning inside its own
    // origin and radius. One retained policy used to plan every seed from the
    // first seed's origin, which truncated the outer two and slowed the whole
    // batch; the measured window widened downward when that was corrected.
    assert!(established.is_some_and(|s| (120..=200).contains(&s)));
    if let Some(path) = std::env::var_os("SPACEWARS_CLOCK_GRASS_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("favorable-timing.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
            "fixture":"favorable-v1","seed":215,"renderer":"deterministic","playback":1,
            "first_visible_seconds":visible,"three_mature_seconds":established,"history":history}))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn clock_grass_contract_introduction_commit_cancel_repeat_disable_and_restart() {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        215,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
            ClockAction::preview_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    assert!(s.soil().is_none());
    s.cancel_ceiling();
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        Duration::ZERO,
    );
    assert!(s.soil().is_none());
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Grass)],
        Duration::from_nanos(16_666_667),
    );
    seconds(&mut s, 5);
    assert_eq!(s.grass_state().seeded, SEED_BATCH);
    let before = s.plants().plants.observe();
    let material = s.soil().unwrap().accounting();
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        Duration::ZERO,
    );
    assert_eq!(s.plants().plants.observe(), before);
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Grass)],
        Duration::from_nanos(16_666_667),
    );
    assert!(s.grass_state().refusal.is_some());
    assert_eq!(s.plants().plants.observe(), before);
    assert_eq!(s.soil().unwrap().accounting(), material);
    let mut settings = s.settings();
    settings.events.grass = false;
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::configure(settings),
            ClockAction::preview_event(ClockEventKind::Grass),
        ],
        Duration::from_nanos(16_666_667),
    );
    assert_eq!(s.plants().plants.observe(), before);
    assert_eq!(s.soil().unwrap().accounting(), material);
    assert_eq!(
        s.grass_state().refusal.as_deref(),
        Some("Future grass seeding is disabled")
    );
    let fresh = ClockScenario::init(ClockConfig::default(), 215);
    assert!(fresh.soil().is_none());
    assert!(fresh.plants().plants.observe().tissues.is_empty());
}

#[test]
fn clock_grass_contract_equal_time_through_maturation_and_death() {
    let mut normal = fixture();
    let mut fast = fixture();
    fast.set_plant_playback(PlantPlayback::Preview16);
    for _ in 0..20 {
        seconds(&mut normal, 16);
        seconds(&mut fast, 1);
        assert_eq!(normal.plants().plants, fast.plants().plants);
        assert_eq!(normal.plants().exposure, fast.plants().exposure);
    }
    assert!(
        normal
            .plants()
            .plants
            .observe()
            .tissues
            .iter()
            .any(|t| t.role == Role::Shoot && t.size >= 3 && t.health > 0)
    );
    normal.set_plant_conditions(0, [0; 2]);
    fast.set_plant_conditions(0, [0; 2]);
    for _ in 0..20 {
        seconds(&mut normal, 16);
        seconds(&mut fast, 1);
        assert_eq!(normal.plants().plants, fast.plants().plants);
    }
    assert!(normal.plants().plants.observe().tissues.is_empty());
    assert_eq!(normal.plants().dropped_biology_ms, 0);
    assert_eq!(fast.plants().dropped_biology_ms, 0);
}

#[test]
fn clock_grass_contract_actor_crushing_retires_only_overlapping_shoots() {
    let mut s = fixture();
    seconds(&mut s, 10);
    let before = s.plants().plants.observe();
    let shoot = before
        .tissues
        .iter()
        .find(|t| t.role == Role::Shoot)
        .unwrap();
    let root = before
        .tissues
        .iter()
        .find(|t| t.role == Role::Root)
        .unwrap();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let actor = PhysicsId::new(8765);
    world.insert_body(
        BodyId::new(actor, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Dynamic,
            position: s.soil().unwrap().world_point(shoot.cell).unwrap(),
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(actor, ColliderRole::PRIMARY, 0),
            0.2,
            0.2,
        )],
    );
    let soil = s.soil.as_ref().unwrap();
    s.plants.crush(soil, &world);
    let after = s.plants.plants.observe();
    assert!(!after.tissues.iter().any(|t| t.id == shoot.id));
    assert!(
        after
            .tissues
            .iter()
            .any(|t| t.id == root.id && t.health == root.health)
    );
    assert_eq!(after.tick, before.tick);
    assert_eq!(after.ledger.intake, before.ledger.intake);
    assert!(
        after
            .connections
            .iter()
            .all(|(a, b)| *a != shoot.id && *b != shoot.id)
    );
    assert_eq!(s.soil().unwrap().accounting().exported, 0);
}

#[test]
fn clock_grass_contract_connected_shade_dies_while_sunlit_shoot_survives() {
    let mut s = fixture();
    let soil = s.soil.as_ref().unwrap();
    for _ in 0..320 {
        s.plants
            .advance(Duration::from_secs(1), soil, None, &[], vec![]);
    }
    let before = s.plants.plants.observe();
    let mut shoots: Vec<_> = before
        .tissues
        .iter()
        .filter(|t| t.role == Role::Shoot && t.health > 0)
        .collect();
    shoots.sort_by_key(|t| t.cell[0]);
    let shaded = shoots[0];
    let sunlit = shoots[shoots.len() - 1];
    assert_ne!(shaded.cell[0], sunlit.cell[0]);
    assert_eq!(shaded.organism, sunlit.organism);
    let shade = OpaqueFootprint {
        id: engine_core::lighting::LightId(8765, 0),
        center: soil.world_point(shaded.cell).unwrap() + engine_core::Vec2::Y * 8.0,
        radius: 0.9,
    };
    for _ in 0..180 {
        s.plants
            .advance(Duration::from_secs(1), soil, None, &[], vec![shade]);
    }
    let after = s.plants.plants.observe();
    assert!(
        !after
            .tissues
            .iter()
            .any(|t| t.id == shaded.id && t.health > 0)
    );
    assert!(
        after
            .tissues
            .iter()
            .any(|t| t.id == sunlit.id && t.health > 0)
    );
}

#[test]
fn clock_grass_contract_effect_shade_uses_rendered_clipped_faces() {
    use engine_common::RenderPrimitive;
    for kind in [
        ClockEventKind::CellSwap,
        ClockEventKind::DigitSlide,
        ClockEventKind::Marquee,
        ClockEventKind::Falling,
    ] {
        let mut s = fixture();
        ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
        for tick in 0..90 {
            if [0, 15, 45, 89].contains(&tick) {
                let full = ClockScenario::render_frame(&s);
                let actual: Vec<Vec<_>> = full
                    .layers
                    .iter()
                    .filter(|l| l.z == 4)
                    .flat_map(|l| &l.primitives)
                    .filter_map(|p| {
                        let RenderPrimitive::Polygon(p) = p else {
                            return None;
                        };
                        p.fill.as_ref().filter(|f| f.color.a > 0.0)?;
                        Some(
                            p.points
                                .iter()
                                .map(|p| engine_core::Vec2::new(p.x, p.y))
                                .collect(),
                        )
                    })
                    .collect();
                let optics = crate::render::plant_face_polygons(&s);
                assert_eq!(optics, actual, "{kind:?} tick={tick}");
                assert!(
                    optics.len() <= engine_rapier::world::MAX_LIGHT_FOOTPRINTS - MAX_PLANT_TISSUES
                );
            }
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        }
    }
}

#[test]
fn clock_grass_contract_crow_silhouette_damage_respects_pause_and_roots() {
    let mut s = fixture();
    seconds(&mut s, 10);
    let before = s.plants.plants.observe();
    let shoot = before
        .tissues
        .iter()
        .find(|t| t.role == Role::Shoot)
        .unwrap();
    let center = s.soil().unwrap().world_point(shoot.cell).unwrap();
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Crow)],
        Duration::ZERO,
    );
    s.crow_visit.as_mut().unwrap().position = center - engine_core::Vec2::Y * 3.0;
    let polygons = crate::render::plant_actor_polygons(&s);
    assert!(!polygons.is_empty());
    assert!(polygons.len() <= 160);
    ClockScenario::step(&mut s, &[], Duration::ZERO);
    assert_eq!(s.plants.plants.observe(), before);
    s.plants.crush_polygons(s.soil.as_ref().unwrap(), &polygons);
    let after = s.plants.plants.observe();
    assert!(!after.tissues.iter().any(|t| t.id == shoot.id));
    for root in before.tissues.iter().filter(|t| t.role == Role::Root) {
        assert!(
            after
                .tissues
                .iter()
                .any(|t| t.id == root.id && t.health == root.health)
        );
    }
    assert_eq!(after.tick, before.tick);
}

#[test]
fn clock_grass_contract_resize_preserves_coordinates_and_accounts_lost_edges() {
    let mut s = fixture();
    let identity = s.soil().unwrap().material_support();
    let root = s.plants.plants.observe().tissues[0].clone();
    let location = s.soil().unwrap().world_point(root.cell).unwrap();
    let initial = s.soil().unwrap().accounting();
    s.set_aspect_ratio(4.0);
    assert_eq!(s.soil().unwrap().world_point(root.cell), Some(location));
    assert_eq!(s.soil().unwrap().accounting(), initial);
    s.set_aspect_ratio(0.25);
    // Existing coordinates are not reindexed or recharged by layout changes.
    assert_eq!(s.soil().unwrap().material_support(), identity);
    assert_eq!(s.soil().unwrap().accounting(), initial);
    for _ in 0..150 {
        ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
    }
    let narrow = s.soil().unwrap().accounting();
    assert!(narrow.exported > 0 && narrow.retained > 0);
    assert_eq!(narrow.added, narrow.retained as u64 + narrow.exported);
    assert!(
        !s.plants
            .plants
            .observe()
            .tissues
            .iter()
            .any(|t| t.id == root.id)
    );
    s.set_aspect_ratio(4.0);
    ClockScenario::step(&mut s, &[], Duration::ZERO);
    assert_eq!(s.soil().unwrap().accounting(), narrow);
    assert!(s.plants.plants.observe().tissues.is_empty());
    // Admission is explicit, bounded to the original retained coordinate field.
    let refill = s.admit_soil();
    assert_eq!(refill.sites.len() as u64, narrow.exported);
    assert_eq!(s.soil().unwrap().material_support(), identity);
    assert!(s.plants.plants.observe().tissues.is_empty());
}

#[test]
fn clock_grass_contract_floor_transitions_cover_entire_events_and_lab_modes() {
    use crate::ClockWaterLab;
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Explosion,
        ClockEventKind::Rain,
        ClockEventKind::Meltdown,
        ClockEventKind::Aquarium,
        ClockEventKind::Duck,
    ] {
        let mut s = fixture();
        let identity = s.soil().unwrap().material_support();
        ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
        for tick in 0..4000 {
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
            if tick % 120 == 0 {
                let soil = s.soil().unwrap();
                let a = soil.accounting();
                assert_eq!(soil.material_support(), identity);
                assert_eq!(a.added, a.retained as u64 + a.exported);
                assert!(a.retained <= MAX_SOIL_CELLS);
                assert_ne!(soil.support_state(), SoilSupportState::AwaitingAdapter);
            }
        }
        assert!(
            s.active_event.is_none(),
            "{kind:?} must complete its full transition"
        );
        ClockScenario::step(&mut s, &[], Duration::ZERO);
        let before = s.soil().unwrap().accounting();
        ClockScenario::step(&mut s, &[], Duration::ZERO);
        assert_eq!(s.soil().unwrap().accounting(), before);
    }
    for mode in [
        ClockWaterLab::Cascade,
        ClockWaterLab::Displacement,
        ClockWaterLab::DisplacementControl,
        ClockWaterLab::Floating,
        ClockWaterLab::FloatingControl,
        ClockWaterLab::Sinking,
        ClockWaterLab::Rotating,
        ClockWaterLab::RotatingControl,
        ClockWaterLab::Multiple,
        ClockWaterLab::MultipleControl,
        ClockWaterLab::Spilling,
        ClockWaterLab::SpillingControl,
    ] {
        let mut s = ClockScenario::init(
            ClockConfig {
                event_profile: ClockEventProfile::Off,
                water_lab: mode,
                ..Default::default()
            },
            215,
        );
        s.admit_soil();
        let identity = s.soil().unwrap().material_support();
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
            Duration::ZERO,
        );
        for _ in 0..120 {
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        }
        let soil = s.soil().unwrap();
        assert_eq!(soil.material_support(), identity);
        let a = soil.accounting();
        assert_eq!(a.added, a.retained as u64 + a.exported);
        assert_ne!(soil.support_state(), SoilSupportState::AwaitingAdapter);
        s.finish_event();
        ClockScenario::step(&mut s, &[], Duration::ZERO);
        assert_eq!(s.soil().unwrap().accounting().added, a.added);
    }
}

#[test]
fn clock_grass_contract_inspection_reveals_paid_underground_spread_without_mutation() {
    let mut ordinary = fixture();
    let mut inspected = fixture();
    inspected.config.grass_inspection = true;
    assert!(ordinary.grass_state().inspection.is_none());
    let first = inspected.grass_state().inspection.unwrap();
    assert_eq!(first.tissues.len(), 1);
    assert_eq!(first.tissues[0].role, "root");
    assert_eq!(first.tissues[0].cell, [10, 2]);
    seconds(&mut ordinary, 320);
    seconds(&mut inspected, 320);
    assert_eq!(ordinary.plants.plants, inspected.plants.plants);
    assert_eq!(
        ordinary.soil().unwrap().accounting(),
        inspected.soil().unwrap().accounting()
    );
    let before = inspected.plants.plants.observe();
    let accounting = inspected.soil().unwrap().accounting();
    let view = inspected.grass_state().inspection.unwrap();
    assert!(view.tissues.iter().any(|t| t.role == "root"));
    assert!(view.tissues.iter().any(|t| t.role == "runner"));
    assert!(
        view.tissues
            .iter()
            .any(|t| t.role != "shoot" && t.cell != first.tissues[0].cell)
    );
    for t in &view.tissues {
        let actual = before.tissues.iter().find(|a| a.id == t.id).unwrap();
        assert_eq!(t.cell, actual.cell);
        assert_eq!(t.size, actual.size);
        assert_eq!(t.development, actual.development);
        assert_eq!(t.invested, actual.invested);
        assert_eq!(t.support, actual.support.map(|s| [s.body, s.generation]));
        if t.role != "shoot" {
            assert!(view.soil_cells.contains(&t.cell));
        }
    }
    for edge in &view.connections {
        assert!(before.connections.contains(&(edge[0], edge[1])));
    }
    let frame = ClockScenario::render_frame(&inspected);
    assert_eq!(
        frame
            .layers
            .iter()
            .find(|l| l.z == 42)
            .unwrap()
            .primitives
            .len(),
        view.tissues.len()
    );
    assert!(
        frame
            .layers
            .iter()
            .any(|l| l.z == 41 && !l.primitives.is_empty())
    );
    assert_eq!(inspected.plants.plants.observe(), before);
    assert_eq!(inspected.soil().unwrap().accounting(), accounting);
    let without = ClockScenario::render_frame(&ordinary);
    assert!(!without.layers.iter().any(|l| (40..=43).contains(&l.z)));
    // Every normal Clock primitive, including readable digits, is unchanged.
    for layer in &without.layers {
        assert_eq!(frame.layers.iter().find(|l| l.z == layer.z), Some(layer));
    }
    for aspect in [0.25, 0.6, 800.0 / 480.0, 4.0] {
        let layout = Layout::new(aspect);
        let panel = crate::render::grass_inspection::bounds(layout);
        assert!(panel.0.x > layout.bounds_min.x && panel.1.x < layout.bounds_max.x);
        assert!(panel.0.y > layout.floor_y && panel.1.y < layout.face_origin.y);
    }
    if let Some(path) = std::env::var_os("SPACEWARS_CLOCK_GRASS_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("root-runner-inspection.json"),serde_json::to_vec_pretty(&serde_json::json!({"initial":first,"developed":view,"frame":frame,"tick":before.tick})).unwrap()).unwrap();
    }
}

#[test]
fn ceiling_light_contract_independent_shortages_recover_before_death() {
    for resource in 0..3 {
        let mut s = fixture();
        seconds(&mut s, 120);
        let mut availability = [1000; 2];
        let intensity = if resource == 2 {
            250
        } else {
            availability[resource] = 0;
            1000
        };
        s.set_plant_conditions(intensity, availability);
        let mut damaged = None;
        let mut unmet = false;
        for second in 1..=240 {
            seconds(&mut s, 1);
            let o = s.plants.plants.observe();
            unmet |= o.ledger.unmet[resource] > 0;
            assert!(
                o.tissues.iter().all(|t| t.shade_ticks == 0),
                "resource shortage must be independent of shade"
            );
            if let Some(t) = o.tissues.iter().find(|t| t.health > 0 && t.health < 100) {
                damaged = Some((second, t.id, t.health));
                break;
            }
        }
        let (second, id, health) =
            damaged.expect("sustained independent shortage must damage paid tissue");
        assert!(unmet);
        println!(
            "independent shortage resource={resource} first-damage-after={second}s health={health}"
        );
        s.set_plant_conditions(1000, [1000; 2]);
        seconds(&mut s, 20);
        let o = s.plants.plants.observe();
        let recovered = o
            .tissues
            .iter()
            .find(|t| t.id == id)
            .expect("recovery must retain tissue identity");
        assert!(recovered.health > health);
        assert_eq!(recovered.shade_ticks, 0);
        assert_eq!(recovered.shortage_ticks, 0);
    }
}
