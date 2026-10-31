//! Export a self-contained sensor gallery and reproducible physics snapshots.
//! cargo run -p scenario-spacewars --profile ci --example flight_sensor_probe -- --out target/flight-sensor-v1
use clap::Parser;
use engine_common::Scenario;
use engine_core::Vec2;
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainSpec},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderShape,
        ColliderSpec, CollisionGroups, PhysicsId, PhysicsWorld,
        polar::{PolarConfig, PolarObservation, PolarSensor},
    },
};
use engine_terrain::{Brush, EditMode, Terrain, TerrainEdit, TerrainGeometry, TerrainSurface};
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
#[path = "flight_sensor_probe/navigation.rs"]
mod navigation;

#[derive(Parser)]
struct Args {
    /// A fresh output directory; existing evidence is never overwritten.
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=100))]
    repeats: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

struct Scene {
    name: String,
    description: String,
    sequence: String,
    tick: u64,
    world: PhysicsWorld,
    origin: Vec2,
    angle: f32,
    goal: Vec2,
    objects: Vec<Value>,
    witnesses: Vec<Vec2>,
    physical_check: Value,
    ship_probe: bool,
    route: Vec<Vec2>,
    comparison_group: Option<String>,
}

fn scene(name: &str, description: &str) -> Scene {
    Scene {
        name: name.into(),
        description: description.into(),
        sequence: name.into(),
        tick: 1,
        world: PhysicsWorld::new(Default::default()),
        origin: Vec2::ZERO,
        angle: 0.,
        goal: Vec2::new(100., 0.),
        objects: Vec::new(),
        witnesses: Vec::new(),
        physical_check: Value::Null,
        ship_probe: false,
        route: Vec::new(),
        comparison_group: None,
    }
}

fn insert(s: &mut Scene, n: u64, position: Vec2, shape: ColliderShape, label: &str) {
    let entity = PhysicsId::new(n);
    let collider = ColliderSpec {
        shape,
        ..ColliderSpec::ball(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 1.)
    };
    assert!(s.world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[collider]
    ));
    s.objects
        .push(json!({"entity":n,"kind":label,"position":position,"velocity":Vec2::ZERO}));
}

fn block(s: &mut Scene, n: u64, p: Vec2, half: Vec2) {
    insert(
        s,
        n,
        p,
        ColliderShape::Cuboid {
            half_width: half.x,
            half_height: half.y,
        },
        "structure",
    );
    s.witnesses.push(p);
}

fn sample_index(o: &PolarObservation, p: Vec2) -> Option<usize> {
    let local = (p - o.origin).rotate_radians(-o.angle);
    let r = local.length();
    let band = o
        .config
        .radial_edges
        .windows(2)
        .position(|b| r >= b[0] && r < b[1])?;
    let a = local.y.atan2(local.x).rem_euclid(std::f32::consts::TAU);
    let sector =
        ((a / std::f32::consts::TAU * o.config.sectors as f32) as usize).min(o.config.sectors - 1);
    Some(band * o.config.sectors + sector)
}

fn variants() -> Vec<(&'static str, PolarConfig)> {
    let refined = vec![
        0., 2., 4., 6., 8., 11., 14., 18., 23., 29., 36., 44., 54., 66., 80., 96., 114., 134.,
        156., 180., 206., 234., 266., 300.,
    ];
    vec![
        (
            "32 sectors / 2×2 samples",
            PolarConfig {
                sectors: 32,
                ..Default::default()
            },
        ),
        ("64 sectors / 2×2 samples", PolarConfig::default()),
        (
            "64 sectors / 4×2 samples",
            PolarConfig {
                angular_samples: 4,
                ..Default::default()
            },
        ),
        (
            "64 sectors / 23 bands / 2×2",
            PolarConfig {
                radial_edges: refined.clone(),
                ..Default::default()
            },
        ),
        (
            "128 sectors / 23 bands / 2×2",
            PolarConfig {
                sectors: 128,
                radial_edges: refined,
                ..Default::default()
            },
        ),
    ]
}

fn evaluate(s: &Scene, args: &Args) -> Value {
    let bytes = s.world.snapshot_bytes().unwrap();
    let snapshot = format!("snapshots/{}.bin", s.name);
    fs::write(args.out.join(&snapshot), &bytes).unwrap();
    let rebuilt = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let outlines = s.world.polar_geometry_outlines(Default::default()).unwrap();
    let mut records = Vec::new();
    for (label, config) in variants() {
        let sensor = PolarSensor::new(config.clone()).unwrap();
        let observation = sensor
            .observe(&s.world, s.origin, s.angle, Default::default())
            .unwrap();
        assert_eq!(
            observation,
            sensor
                .observe(&rebuilt, s.origin, s.angle, Default::default())
                .unwrap(),
            "snapshot replay: {}",
            s.name
        );
        let misses: Vec<_> = s
            .witnesses
            .iter()
            .filter(|p| {
                sample_index(&observation, **p)
                    .is_some_and(|i| observation.cells[i].overlap_coverage == 0.)
            })
            .collect();
        assert!(
            misses.is_empty(),
            "solid witness missed: {} {misses:?}",
            s.name
        );
        let reference = PolarSensor::new(PolarConfig {
            angular_samples: 4,
            radial_samples: 4,
            ..config
        })
        .unwrap()
        .observe(&s.world, s.origin, s.angle, Default::default())
        .unwrap();
        // Independent, finer point locations audit the cell signal. These are
        // sampled estimates, NOT an exhaustive geometric coverage certificate.
        let mut blocked_free = 0.;
        let mut occupied = 0.;
        let mut missed = 0.;
        let mut mae = 0.;
        let mut area = 0.;
        for (i, (cell, truth)) in observation.cells.iter().zip(&reference.cells).enumerate() {
            let band = i / observation.config.sectors;
            let edges = &observation.config.radial_edges;
            let weight = (edges[band + 1].powi(2) - edges[band].powi(2)) as f64;
            area += weight;
            occupied += weight * f64::from(truth.sampled_occupancy);
            mae += weight * f64::from((cell.sampled_occupancy - truth.sampled_occupancy).abs());
            if cell.overlap_coverage > 0. {
                blocked_free += weight * f64::from(1. - truth.sampled_occupancy);
            } else {
                missed += weight * f64::from(truth.sampled_occupancy);
            }
        }
        assert_eq!(missed, 0., "occupied reference samples missed: {}", s.name);
        let mut timings = Vec::new();
        for _ in 0..args.repeats {
            let start = Instant::now();
            std::hint::black_box(
                sensor
                    .observe(&s.world, s.origin, s.angle, Default::default())
                    .unwrap(),
            );
            timings.push(start.elapsed().as_secs_f64() * 1000.);
        }
        timings.sort_by(f64::total_cmp);
        let ray_nearest = observation
            .rays
            .iter()
            .filter_map(|r| r.distance)
            .min_by(f32::total_cmp);
        let crossing: Option<Vec2> =
            serde_json::from_value(s.physical_check["crossing"].clone()).ok();
        let route_samples = s.physical_check["route_samples"].as_array();
        let clear_centers: Vec<usize> = route_samples
            .into_iter()
            .flatten()
            .filter(|p| p["hull_fits"] == true)
            .filter_map(|p| serde_json::from_value(p["position"].clone()).ok())
            .filter_map(|p| sample_index(&observation, p))
            .collect();
        let touched = clear_centers
            .iter()
            .filter(|i| observation.cells[**i].overlap_coverage > 0.)
            .count();
        let metrics = json!({
            "median_ms":timings[timings.len()/2],"min_ms":timings[0],"max_ms":timings.last(),
            "timing_samples_ms":timings,
            "native_queries":observation.native_queries,
            "cells":observation.cells.len(),
            "sampled_occupancy_mae":mae/area,
            "empty_reference_area_in_flagged_cells":blocked_free/area,
            "missed_occupied_reference_fraction":if occupied>0. { missed/occupied } else { 0. },
            "solid_witnesses":s.witnesses.len(),"missed_witnesses":misses.len(),
            "overlap_cells_with_zero_center_samples":observation.cells.iter().filter(|c| c.overlap_coverage>0. && c.sampled_occupancy==0.).count(),
            "ray_nearest_error":ray_nearest.zip(observation.nearest_distance).map(|(a,b)|a-b),
            "ring_missed_all":ray_nearest.is_none() && observation.nearest_distance.is_some(),
            "gap_center_cell_overlap":crossing.and_then(|p|sample_index(&observation,p)).map(|i|observation.cells[i].overlap_coverage),
            "clear_route_centers_in_range":clear_centers.len(),
            "clear_route_centers_in_touched_cells":touched,
            "clear_route_centers_in_touched_cells_fraction":if clear_centers.is_empty(){None}else{Some(touched as f64/clear_centers.len() as f64)},
        });
        records.push(json!({"label":label,"observation":observation,"metrics":metrics,
            "reference_occupancy":reference.cells.iter().map(|c|c.sampled_occupancy).collect::<Vec<_>>()}));
    }
    assert_eq!(
        bytes,
        s.world.snapshot_bytes().unwrap(),
        "sensor mutated the world"
    );
    let objects: Vec<_> = s
        .world
        .motions()
        .map(|m| {
            let label = s
                .objects
                .iter()
                .find(|o| o["entity"] == m.id.entity.value())
                .map(|o| o["kind"].clone());
            json!({"entity":m.id.entity.value(),"kind":label,"position":m.motion.position,
            "velocity":s.world.velocity_at_point(m.id,m.motion.position),
            "angle":m.motion.angle,"angular_velocity":m.motion.angular_velocity,
            "solid_bounds":s.world.body_solid_bounds(m.id)})
        })
        .collect();
    json!({"name":s.name,"description":s.description,"sequence":s.sequence,"tick":s.tick,
        "origin":s.origin,"angle":s.angle,"goal":s.goal,"probe_radius":4.,
        "ship_probe":s.ship_probe,"route":s.route,"comparison_group":s.comparison_group,
        "outlines":outlines,"objects":objects,"physical_check":s.physical_check,
        "snapshot":snapshot,"snapshot_sha256":format!("{:x}",Sha256::digest(&bytes)),
        "variants":records})
}

fn fixtures() -> Vec<Scene> {
    let mut scenes = Vec::new();
    let mut s = scene(
        "occluded-pair",
        "Two obstacles share a bearing, with free space between them. The map must retain the farther obstacle.",
    );
    block(&mut s, 1, Vec2::new(22., 0.), Vec2::new(2., 6.));
    block(&mut s, 2, Vec2::new(100., 0.), Vec2::new(5., 22.));
    scenes.push(s);
    for (name, distance) in [("thin-wall-near", 22.), ("thin-wall-far", 170.)] {
        let mut s = scene(
            name,
            "A 0.04-unit-thick obstacle lies on the angular seam, between center rays. Orange cells expose hits missed by point samples.",
        );
        block(&mut s, 1, Vec2::new(distance, 0.), Vec2::new(2., 0.02));
        scenes.push(s);
    }
    for (name, gap) in [("gap-too-narrow", 7.5), ("gap-passable", 9.)] {
        let mut s = scene(
            name,
            "A radius-4 inspection probe crosses this opening. Physical clearance is checked independently; colored cells are not a movement permission.",
        );
        for (n, sign) in [(1, -1.), (2, 1.)] {
            block(
                &mut s,
                n,
                Vec2::new(40., sign * (gap / 2. + 12.)),
                Vec2::new(2., 12.),
            );
        }
        s.world.step(1. / 60.);
        let clear = s
            .world
            .capsule_is_clear(Vec2::new(40., 0.), 0., 0., 4., CollisionGroups::ALL);
        assert_eq!(clear, gap > 8.);
        s.physical_check = json!({"gap_width":gap,"probe_diameter":8.,"probe_fits":clear,"crossing":Vec2::new(40.,0.)});
        scenes.push(s);
    }
    let mut s = scene(
        "sun-and-boundary",
        "The sun is a filled disk; the arena boundary is a hollow polyline. The enclosed interior must stay empty.",
    );
    insert(
        &mut s,
        1,
        Vec2::new(75., 15.),
        ColliderShape::Ball { radius: 22. },
        "sun",
    );
    let vertices = (0..=96)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 96.;
            Vec2::new(a.cos(), a.sin()) * 160.
        })
        .collect();
    insert(
        &mut s,
        2,
        Vec2::ZERO,
        ColliderShape::Polyline { vertices },
        "world-boundary",
    );
    s.witnesses.push(Vec2::new(75., 15.));
    scenes.push(s);
    for (i, angle) in [-0.035, 0., 0.035].into_iter().enumerate() {
        let mut s = scene(
            &format!("rotation-{i}"),
            "The observer rotates through the zero-angle seam. World geometry stays fixed.",
        );
        s.sequence = "observer-rotation".into();
        s.tick = i as u64 + 1;
        s.angle = angle;
        block(&mut s, 1, Vec2::new(30., 0.), Vec2::new(1., 5.));
        scenes.push(s);
    }
    let mut moving = scene(
        "moving-0",
        "The same physical structure translates and rotates between completed physics steps.",
    );
    block(&mut moving, 1, Vec2::new(50., -15.), Vec2::new(3., 12.));
    moving.sequence = "moving-structure".into();
    let moving_body = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    moving
        .world
        .set_body_kind(moving_body, BodyKind::KinematicPosition, false);
    for tick in 0..=30 {
        let t = tick as f32 / 15.;
        let p = Vec2::new(50. + t * 5., -15. + t * 15.);
        moving
            .world
            .set_next_kinematic_pose(moving_body, p, t * 0.3);
        moving.world.step(1. / 60.);
        if tick % 15 != 0 {
            continue;
        }
        scenes.push(Scene {
            name: format!("moving-{}", tick / 15),
            description: moving.description.clone(),
            sequence: moving.sequence.clone(),
            tick: tick + 1,
            world: moving.world.clone(),
            origin: Vec2::ZERO,
            angle: 0.,
            goal: Vec2::new(100., 0.),
            objects: vec![json!({"entity":1,"kind":"structure","position":p})],
            witnesses: vec![p],
            physical_check: Value::Null,
            ship_probe: false,
            route: Vec::new(),
            comparison_group: None,
        });
    }
    for s in &mut scenes {
        if s.sequence != "moving-structure" {
            s.world.step(1. / 60.);
        }
    }
    scenes
}

fn terrain_scenes(seed: u64) -> Vec<Scene> {
    let source = SurfaceSortieScenario::init_material_arena_surface_trial(
        seed,
        false,
        0.,
        TerrainSurface::Interpolated,
    );
    let mut field: Terrain = source.planet_terrain(0).unwrap().clone();
    let mut geometry = TerrainGeometry::with_surface(&field, TerrainSurface::Interpolated);
    let mut base = scene(
        "terrain-before",
        "Actual generated Spacewars terrain, using its interpolated compound collision shapes.",
    );
    let assembly = TerrainAssembly::insert(
        &mut base.world,
        PhysicsId::new(1),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &field,
        &geometry,
        TerrainSpec {
            surface: TerrainSurface::Interpolated,
            ..Default::default()
        },
    )
    .unwrap();
    let bounds = base.world.body_solid_bounds(assembly.body()).unwrap();
    base.origin = Vec2::new(bounds.1.x + 16., 0.);
    base.goal = base.origin + Vec2::new(-60., 40.);
    base.objects = vec![
        json!({"entity":1,"kind":"generated-planet-terrain","position":Vec2::ZERO,
        "nominal_extent":field.width() as f32*field.cell_size()/2.,"terrain_hash":field.hash()}),
    ];
    base.world.step(1. / 60.);
    let mut scenes = Vec::new();
    let take = |base: &Scene, name: &str, description: &str, tick: u64, field: &Terrain| Scene {
        name: name.into(),
        description: description.into(),
        sequence: "terrain-edit".into(),
        tick,
        world: base.world.clone(),
        origin: base.origin,
        angle: base.angle,
        goal: base.goal,
        objects: base.objects.clone(),
        witnesses: Vec::new(),
        physical_check: json!({"terrain_revision":field.revision(),"terrain_hash":field.hash()}),
        ship_probe: false,
        route: Vec::new(),
        comparison_group: None,
    };
    scenes.push(take(&base, "terrain-before", &base.description, 1, &field));
    let center = field.local_to_cell(Vec2::new(bounds.1.x - 4., 0.)).unwrap();
    let radius = (12. / field.cell_size()).ceil() as u32;
    let changed = field
        .apply(TerrainEdit {
            brush: Brush::Circle { center, radius },
            mode: EditMode::Remove,
        })
        .unwrap();
    assert!(changed.changed_cells > 0);
    geometry.refresh(&field);
    let mut assembly = assembly;
    assembly
        .synchronize(&mut base.world, &field, &geometry)
        .unwrap();
    base.world.step(1. / 60.);
    scenes.push(take(&base,"terrain-crater","A real TerrainEdit removes a crater; colliders are synchronized and stepped before reading.",2,&field));
    block(
        &mut base,
        2,
        Vec2::new(bounds.1.x + 3., 0.),
        Vec2::new(1., 16.),
    );
    base.world.step(1. / 60.);
    scenes.push(take(&base,"terrain-added-wall","An added solid wall spans the crater. It is an ordinary collider, standing in for future construction.",3,&field));
    base.world.remove_entity(PhysicsId::new(2));
    base.world.step(1. / 60.);
    scenes.push(take(
        &base,
        "terrain-wall-removed",
        "Removing the added wall restores the crater view after the next completed step.",
        4,
        &field,
    ));
    scenes
}

fn main() {
    let args = Args::parse();
    fs::create_dir(&args.out).expect("use a fresh output directory");
    fs::create_dir(args.out.join("snapshots")).unwrap();
    let mut scenes = fixtures();
    scenes.extend(terrain_scenes(args.seed));
    let ship = navigation::geometry(args.seed);
    scenes.extend(navigation::fixtures());
    let plan = json!({"version":2,"kind":"exploratory-sensor-calibration","seed":args.seed,"repeats":args.repeats,
        "scenes":scenes.iter().map(|s|json!({"name":s.name,"ship_probe":s.ship_probe,"comparison_group":s.comparison_group})).collect::<Vec<_>>(),
        "variants":variants().iter().map(|(label,config)|json!({"label":label,"config":config})).collect::<Vec<_>>(),
        "reference_samples_per_cell":[4,4],"route_check":"native hull and feet at fixed orientation; continuous translations between prescribed nodes; not flight qualification"});
    fs::write(
        args.out.join("plan.json"),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    for s in &mut scenes {
        if s.ship_probe {
            s.physical_check = navigation::inspect(s, &ship);
        }
    }
    let mut records = Vec::new();
    for s in &scenes {
        eprintln!("Inspecting {}", s.name);
        records.push(evaluate(s, &args));
    }
    let comparisons = navigation::comparisons(&records);
    let find = |name: &str| {
        records.iter().find(|r| r["name"] == name).unwrap()["variants"][1]["observation"]["cells"]
            .clone()
    };
    assert_ne!(find("terrain-before"), find("terrain-crater"));
    assert_ne!(find("terrain-crater"), find("terrain-added-wall"));
    assert_eq!(find("terrain-crater"), find("terrain-wall-removed"));
    // Exercise the same sensor against an ordinary, completed match state.
    let mut live = SurfaceSortieScenario::init_material_match(args.seed);
    SurfaceSortieScenario::step(&mut live, &[], Duration::from_nanos(16_666_667));
    let before = SurfaceSortieScenario::observe(&live).payload;
    let sensor = PolarSensor::new(Default::default()).unwrap();
    let live_record = live.flight_surroundings(0, &sensor).unwrap();
    let live_outlines = live.flight_sensor_outlines(0).unwrap();
    assert_eq!(before, SurfaceSortieScenario::observe(&live).payload);
    let summary:Vec<_>=records.iter().map(|r|json!({"name":r["name"],"sequence":r["sequence"],"physical_check":r["physical_check"],
        "variants":r["variants"].as_array().unwrap().iter().map(|v|json!({"label":v["label"],"metrics":v["metrics"]})).collect::<Vec<_>>()})).collect();
    let source_files = [
        (
            "crates/engine-rapier/src/world/polar.rs",
            include_bytes!("../../../crates/engine-rapier/src/world/polar.rs").as_slice(),
        ),
        (
            "scenarios/spacewars/src/surface_sortie/flight_sensing.rs",
            include_bytes!("../src/surface_sortie/flight_sensing.rs").as_slice(),
        ),
        (
            "scenarios/spacewars/examples/flight_sensor_probe.rs",
            include_bytes!("flight_sensor_probe.rs").as_slice(),
        ),
        (
            "scenarios/spacewars/examples/flight_sensor_probe/viewer.html",
            include_bytes!("flight_sensor_probe/viewer.html").as_slice(),
        ),
        (
            "scenarios/spacewars/examples/flight_sensor_probe/navigation.rs",
            include_bytes!("flight_sensor_probe/navigation.rs").as_slice(),
        ),
    ];
    let sources: Vec<_> = source_files
        .iter()
        .map(|(path, bytes)| json!({"path":path,"sha256":format!("{:x}",Sha256::digest(bytes))}))
        .collect();
    let runtime_sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(std::env::current_exe().unwrap()).unwrap())
    );
    let report = json!({"version":2,"seed":args.seed,"repeats":args.repeats,"scenes":records,"plan":plan,
        "comparisons":comparisons,"ship_outlines":navigation::outlines(&ship),
        "sources":sources,"runtime_sha256":runtime_sha256,"debug_assertions":cfg!(debug_assertions),
        "live_match":{"observation":live_record,"outlines":live_outlines},
        "notes":["Prototype, not a trained policy or navigation qualification.",
        "Overlap coverage is conservative subcell intersection, not occupied area. This v2 comparison uses a common 4×4 point reference per cell, not exhaustive truth; C0 used 8×8.",
        "Ship scenes use the actual open-wing hull and feet. Original fixtures retain their labelled radius-4 inspection disk. Circle outlines are tessellated only for display.",
        "Inspection paths use fixed-orientation native shape sweeps, not a planner or flight controller. Clear path samples inside touched cells quantify loss from treating entire cells as blocked, not network performance.",
        "Sensing timings exclude construction, replay, dense reference, outline export and rendering. Record the build profile with the run command."]});
    fs::write(
        args.out.join("report.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    fs::write(args.out.join("summary.json"),serde_json::to_vec_pretty(&json!({"version":2,"seed":args.seed,"repeats":args.repeats,"scenes":summary,"comparisons":comparisons,"plan":plan,"notes":report["notes"],"sources":report["sources"],"runtime_sha256":report["runtime_sha256"],"debug_assertions":report["debug_assertions"]})).unwrap()).unwrap();
    let html = include_str!("flight_sensor_probe/viewer.html").replace(
        "/*REPORT_DATA*/null",
        &serde_json::to_string(&report)
            .unwrap()
            .replace('<', "\\u003c"),
    );
    fs::write(args.out.join("index.html"), html).unwrap();
    println!(
        "Wrote {} scenes × {} configurations, snapshots, metrics and viewer to {}",
        scenes.len(),
        variants().len(),
        args.out.display()
    );
}
