//! Controlled measurements of actual open-wing geometry and production controls.
use clap::Parser;
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderShape, ColliderSpec,
    CollisionGroups, PhysicsId, PhysicsWorld,
};
use scenario_spacewars::surface_sortie::{
    SurfaceSortieAction,
    flight_calibration::{CALIBRATION_DT, CalibrationStart, FlightCalibration},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    fs,
    path::PathBuf,
};

#[derive(Parser)]
struct Args {
    /// Fresh output directory. Existing evidence is never overwritten.
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

#[derive(Clone, Serialize)]
struct Case {
    name: String,
    start: CalibrationStart,
    maneuver: &'static str,
    ticks: u32,
    turn_ticks: u32,
    turn: f32,
}

impl Case {
    fn control(&self, tick: u32) -> SurfaceSortieAction {
        SurfaceSortieAction {
            horizontal: if tick < self.turn_ticks {
                self.turn
            } else {
                0.0
            },
            primary_held: self.maneuver == "thrust",
            brake_held: self.maneuver == "brake"
                || (self.maneuver == "turn-release-brake" && tick >= self.turn_ticks),
            ..Default::default()
        }
    }
}

fn cases(seed: u64) -> Vec<Case> {
    let start = CalibrationStart {
        seed,
        speed: 0.0,
        velocity_bearing: 0.0,
        heading: 0.0,
        altitude: 150.0,
        gravity: false,
        planet_spin: 0.0,
    };
    let mut cases = Vec::new();
    for gravity in [false, true] {
        for speed in [5., 10., 30., 50., 70., 100., 140.] {
            for (heading_name, heading) in [("up", 0.), ("right", -FRAC_PI_2), ("down", PI)] {
                cases.push(Case {
                    name: format!(
                        "brake-{}-{speed}-{heading_name}",
                        if gravity { "gravity" } else { "zero-g" }
                    ),
                    start: CalibrationStart {
                        speed,
                        gravity,
                        heading,
                        ..start
                    },
                    maneuver: "brake",
                    ticks: 300,
                    turn_ticks: 0,
                    turn: 0.0,
                });
            }
        }
    }
    for (name, velocity_bearing, planet_spin) in [
        ("inward", -FRAC_PI_2, 0.0),
        ("outward", FRAC_PI_2, 0.0),
        ("rotating-frame", 0.0, 0.015),
    ] {
        cases.push(Case {
            name: format!("brake-gravity-70-{name}"),
            start: CalibrationStart {
                speed: 70.,
                gravity: true,
                velocity_bearing,
                planet_spin,
                ..start
            },
            maneuver: "brake",
            ticks: 300,
            turn_ticks: 0,
            turn: 0.0,
        });
    }
    for speed in [0., 30., 70.] {
        for maneuver in ["turn-held", "turn-release-coast", "turn-release-brake"] {
            cases.push(Case {
                name: format!("{maneuver}-{speed}"),
                start: CalibrationStart { speed, ..start },
                maneuver,
                ticks: 180,
                turn_ticks: if maneuver == "turn-held" { 180 } else { 60 },
                turn: 1.0,
            });
        }
    }
    cases.push(Case {
        name: "turn-held-left-0".into(),
        start,
        maneuver: "turn-held",
        ticks: 180,
        turn_ticks: 180,
        turn: -1.0,
    });
    for gravity in [false, true] {
        for maneuver in ["coast", "thrust"] {
            cases.push(Case {
                name: format!("{maneuver}-{}", if gravity { "gravity" } else { "zero-g" }),
                start: CalibrationStart {
                    speed: if maneuver == "coast" { 70. } else { 0. },
                    gravity,
                    ..start
                },
                maneuver,
                ticks: 180,
                turn_ticks: 0,
                turn: 0.0,
            });
        }
    }
    cases
}

fn footprint(rig: &FlightCalibration) -> Value {
    let specs = rig.geometry();
    let mut vertices = Vec::new();
    let mut circles = Vec::new();
    for s in &specs {
        match &s.shape {
            ColliderShape::ConvexPolygon { vertices: vs } => vertices.extend(
                vs.iter()
                    .map(|v| s.local_position + v.rotate_radians(s.local_angle)),
            ),
            ColliderShape::Ball { radius } => circles.push((s.local_position, *radius)),
            _ => panic!("unexpected ship geometry; update measurement explicitly"),
        }
    }
    let radius = vertices
        .iter()
        .map(|v| v.length())
        .chain(circles.iter().map(|(p, r)| p.length() + r))
        .fold(0., f32::max);
    let widths: Vec<_> = (0..=12).map(|i| {
        let angle = i as f32 * PI / 12.;
        let mut low = Vec2::new(f32::INFINITY,f32::INFINITY);
        let mut high = -low;
        for (p,r) in vertices.iter().map(|p|(*p,0.)).chain(circles.iter().copied()) {
            let p = p.rotate_radians(angle);
            low.x = low.x.min(p.x-r); low.y = low.y.min(p.y-r);
            high.x = high.x.max(p.x+r); high.y = high.y.max(p.y+r);
        }
        json!({"angle_degrees":i*15,"min":low,"max":high,"width":high.x-low.x,"height":high.y-low.y})
    }).collect();
    // Native overlap checks use every real collider, with generic walls set to
    // ALL groups. They diagnose static fits, not dynamic route feasibility.
    let mut gap_checks = Vec::new();
    for (angle, bounds) in [(0., &widths[0]), (FRAC_PI_2, &widths[6])] {
        let center_offset = -0.5
            * (bounds["min"]["x"].as_f64().unwrap() + bounds["max"]["x"].as_f64().unwrap()) as f32;
        for gap in [8., 9., 10., 10.8, 10.95, 11.05, 11.2, 12., 14., 16.] {
            let mut world = PhysicsWorld::new(Default::default());
            let actor = PhysicsId::new(8_000_000);
            let colliders: Vec<_> = specs
                .iter()
                .enumerate()
                .map(|(i, s)| ColliderSpec {
                    id: ColliderId::new(actor, ColliderRole::PRIMARY, i as u16),
                    collision_groups: CollisionGroups::ALL,
                    solver_groups: CollisionGroups::ALL,
                    ..s.clone()
                })
                .collect();
            assert!(world.insert_body(
                BodyId::new(actor, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    angle,
                    ..Default::default()
                },
                &colliders
            ));
            for (i, sign) in [(1, -1.), (2, 1.)] {
                let e = PhysicsId::new(8_000_000 + i);
                assert!(world.insert_body(
                    BodyId::new(e, BodyRole::PRIMARY),
                    BodySpec {
                        kind: BodyKind::Fixed,
                        position: Vec2::new(sign * (gap / 2. + 2.), 0.),
                        ..Default::default()
                    },
                    &[ColliderSpec::cuboid(
                        ColliderId::new(e, ColliderRole::PRIMARY, 0),
                        2.,
                        100.
                    )]
                ));
            }
            world.step(CALIBRATION_DT.as_secs_f32());
            let fits = |offset| {
                colliders.iter().all(|s| {
                    world.collider_fits_at(
                        s.id,
                        s.local_position.rotate_radians(angle) + Vec2::new(offset, 0.),
                        angle + s.local_angle,
                    ) == Some(true)
                })
            };
            let centered = fits(center_offset);
            // For parallel infinite-height walls this independently agrees with
            // the support extent, away from exact tangencies.
            assert_eq!(centered, gap > bounds["width"].as_f64().unwrap() as f32);
            gap_checks.push(json!({"gap":gap,"angle_degrees":angle.to_degrees(),
                "fits_origin_centered":fits(0.),"fits_bounds_centered":centered,
                "bounds_centering_offset_x":center_offset}));
        }
    }
    let s = rig.sample();
    json!({"hull":vertices,"feet":circles.iter().map(|(p,r)|json!({"center":p,"radius":r})).collect::<Vec<_>>(),
        "origin_radius":radius,"mass":s.mass,"local_center_of_mass":(s.center_of_mass-s.flight.ship.position).rotate_radians(-s.flight.ship.angle),
        "rotated_bounds":widths,"static_gap_checks":gap_checks})
}

fn run(case: &Case, out: &std::path::Path) -> Value {
    let mut rig = FlightCalibration::new(case.start).unwrap();
    let mut replay = FlightCalibration::new(case.start).unwrap();
    let bytes = rig.physics_snapshot();
    assert_eq!(bytes, replay.physics_snapshot());
    let snapshot = format!("snapshots/{}.bin", case.name);
    fs::write(out.join(&snapshot), &bytes).unwrap();
    let initial = rig.sample();
    let mut previous = initial.clone();
    let mut trace = Vec::new();
    let mut path = 0.;
    let mut com_path = 0.;
    let mut angle = 0.;
    let mut turn_at_release = None;
    let mut max_spin = 0_f32;
    let mut dwell = [0_u32; 3];
    let mut arrivals = [Value::Null, Value::Null, Value::Null];
    let mut quarter_turn = None;
    let mut half_turn = None;
    let mut first_invalid = Value::Null;
    let mut coast_spin_stopped = None;
    for tick in 0..=case.ticks {
        let s = if tick == 0 {
            initial.clone()
        } else {
            let c = case.control(tick - 1);
            let s = rig.step(c);
            assert_eq!(
                s,
                replay.step(c),
                "independent replay: {} tick {tick}",
                case.name
            );
            s
        };
        let time = tick as f32 * CALIBRATION_DT.as_secs_f32();
        let relative = s.flight.ship.velocity - s.flight.frame_velocity;
        let speed = relative.length();
        path += s
            .flight
            .ship
            .position
            .distance_to(previous.flight.ship.position);
        com_path += s.center_of_mass.distance_to(previous.center_of_mass);
        let da = s.flight.ship.angle - previous.flight.ship.angle;
        angle += da.sin().atan2(da.cos());
        max_spin = max_spin.max(s.flight.ship.spin.abs());
        if tick == case.turn_ticks {
            turn_at_release = Some(angle)
        }
        if angle.abs() >= FRAC_PI_2 && quarter_turn.is_none() {
            quarter_turn = Some(time)
        }
        if angle.abs() >= PI && half_turn.is_none() {
            half_turn = Some(time)
        }
        if case.turn_ticks > 0
            && tick > case.turn_ticks
            && s.flight.ship.spin.abs() <= 0.01
            && coast_spin_stopped.is_none()
        {
            coast_spin_stopped = Some(time - case.turn_ticks as f32 * CALIBRATION_DT.as_secs_f32())
        }
        let invalid = s.flight.contact
            || s.flight.health < initial.flight.health
            || !s.flight.alive
            || !s.flight.available
            || s.flight.landing.assist_strength > 0.0
            || s.flight.flight.sweep != 0.0;
        if invalid && first_invalid.is_null() {
            first_invalid = json!({"tick":tick,"contact":s.flight.contact,"damage":initial.flight.health-s.flight.health,
                "assist":s.flight.landing.assist_strength,"alive":s.flight.alive,"available":s.flight.available,"sweep":s.flight.flight.sweep});
        }
        for (i, threshold) in [0.5, 1., 2.].into_iter().enumerate() {
            if tick > 0 {
                dwell[i] = if speed <= threshold { dwell[i] + 1 } else { 0 };
            }
            if dwell[i] >= 15 && arrivals[i].is_null() {
                arrivals[i] = json!({"threshold":threshold,"first_entry_seconds":(tick+1-dwell[i]) as f32/60.,"dwell_completed_seconds":time,
                    "origin_path":path,"origin_displacement":s.flight.ship.position.distance_to(initial.flight.ship.position),"com_path":com_path});
            }
        }
        // Keep every physical tick; do not infer intermediate motion in QA.
        trace.push(json!({"t":time,"p":s.flight.ship.position-initial.flight.ship.position,
            "com":s.center_of_mass-initial.flight.ship.position,"v":s.flight.ship.velocity,"com_v":s.center_of_mass_velocity,
            "frame_v":s.flight.frame_velocity,"speed":speed,"angle":angle,"heading":s.flight.ship.angle,"spin":s.flight.ship.spin,
            "gravity":s.flight.gravity,"controls":s.flight.applied,"assist":s.flight.landing.assist_strength,"contact":s.flight.contact,"health":s.flight.health}));
        previous = s;
        if invalid {
            break;
        }
    }
    let metrics = json!({"valid_free_flight":first_invalid.is_null(),"first_invalid":first_invalid,
        "duration_seconds":trace.last().unwrap()["t"],"origin_path":path,"com_path":com_path,
        "final_relative_speed":trace.last().unwrap()["speed"],"final_spin":previous.flight.ship.spin,"peak_spin":max_spin,
        "turn_degrees":angle.to_degrees(),"quarter_turn_seconds":quarter_turn,"half_turn_seconds":half_turn,
        "release_extra_degrees":turn_at_release.filter(|_|case.turn_ticks>0&&case.turn_ticks<case.ticks).map(|a|(angle-a).to_degrees()),
        "release_spin_stop_seconds":coast_spin_stopped,"speed_thresholds_with_15_tick_dwell":arrivals});
    json!({"case":case,"initial":initial,"metrics":metrics,"trace":trace,"physics_snapshot":snapshot,"snapshot_sha256":format!("{:x}",Sha256::digest(&bytes))})
}

fn main() {
    let args = Args::parse();
    fs::create_dir(&args.out).expect("choose a fresh output directory");
    fs::create_dir(args.out.join("snapshots")).unwrap();
    let cases = cases(args.seed);
    let plan = json!({"version":1,"kind":"mechanics-calibration-not-policy-evaluation","seed":args.seed,"cases":cases,
        "dt_nanoseconds":16_666_667,"speed_measurement_thresholds":[0.5,1.,2.],"dwell_ticks":15,
        "stop_on":"first contact, damage, lost vehicle, landing assist or wing sweep",
        "notes":["Thresholds are measurement probes, not accepted task-success tolerances.",
        "Zero gravity and nonrotating planet isolate mechanics. Gravity-on uses the stationary practice planet, not the generated-match gravity envelope.",
        "All maneuvers use prescribed controls, not a native pilot. Every tick is compared to an independently reset replay."]});
    fs::write(
        args.out.join("plan.json"),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    let body = footprint(&FlightCalibration::new(cases[0].start).unwrap());
    let records: Vec<_> = cases
        .iter()
        .map(|c| {
            eprintln!("Measuring {}", c.name);
            run(c, &args.out)
        })
        .collect();
    let sources = [
        (
            "scenarios/spacewars/src/surface_sortie/flight_calibration.rs",
            include_bytes!("../src/surface_sortie/flight_calibration.rs").as_slice(),
        ),
        (
            "scenarios/spacewars/examples/flight_calibration_probe.rs",
            include_bytes!("flight_calibration_probe.rs").as_slice(),
        ),
        (
            "scenarios/spacewars/examples/flight_calibration_probe/viewer.html",
            include_bytes!("flight_calibration_probe/viewer.html").as_slice(),
        ),
    ]
    .map(|(path, bytes)| json!({"path":path,"sha256":format!("{:x}",Sha256::digest(bytes))}));
    let report = json!({"version":1,"plan":plan,"footprint":body,"runs":records,"sources":sources,
        "runtime_sha256":format!("{:x}",Sha256::digest(fs::read(std::env::current_exe().unwrap()).unwrap()))});
    fs::write(
        args.out.join("report.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    let summary = json!({"version":1,"plan":plan,"footprint":body,"sources":sources,"runtime_sha256":report["runtime_sha256"],
        "runs":records.iter().map(|r|json!({"case":r["case"],"metrics":r["metrics"]})).collect::<Vec<_>>()});
    fs::write(
        args.out.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    let html = include_str!("flight_calibration_probe/viewer.html").replace(
        "/*REPORT_DATA*/null",
        &serde_json::to_string(&report)
            .unwrap()
            .replace('<', "\\u003c"),
    );
    fs::write(args.out.join("index.html"), html).unwrap();
    println!(
        "Wrote {} independently replayed maneuvers to {}",
        records.len(),
        args.out.display()
    );
}
