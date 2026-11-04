use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_garden_lab::{Command, GardenLabScenario, GardenLabState};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::{ClientInput, GameKey};
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "garden-lab",
    launcher_category: crate::client_scenarios::LauncherCategory::Extra,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        headless_benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Garden Lab: up/down chooses a control; Space or gamepad bottom face activates it. Left/right moves the opaque shade. Controls include pause/step, speed, light, nutrients, water, shade, inspection, cutting, fresh planting, tissue limits 1–128 and spread 0–8 cells. Limits apply in place; full supplies do not guarantee affordable growth. R restarts; Esc/Start opens host pause. Green living, amber stressed, grey dead; cyan lines show real connections. A pink outline identifies the selected tissue. Physiology ticks are 1000ms, default viewing is 1x, maximum 16x, run limit 480 ticks. Moss controls place deliberate patches on dirt and a movable rock, turn the rock, break its edge, remove it or scrape a strip; olive moss tolerates the dim room, while darkness and the rock's shaded underside still kill it. Light also cycles to dark.",
    create,
};

pub(crate) struct GardenLabClientScenario {
    pub(crate) state: GardenLabState,
    held: std::cell::Cell<[bool; 5]>,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(GardenLabClientScenario {
        state: GardenLabScenario::init((), seed),
        held: std::cell::Cell::new([false; 5]),
    }))
}

impl ClientScenario for GardenLabClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        GardenLabScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        GardenLabScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        let buttons = input.nes_controller_buttons(0);
        use engine_nes::ControllerButtons as B;
        let now = [
            input.is_pressed(GameKey::NesUp) || buttons.contains(B::UP),
            input.is_pressed(GameKey::NesDown) || buttons.contains(B::DOWN),
            input.is_pressed(GameKey::P1Laser) || buttons.contains(B::B),
            input.is_pressed(GameKey::NesLeft) || buttons.contains(B::LEFT),
            input.is_pressed(GameKey::NesRight) || buttons.contains(B::RIGHT),
        ];
        let old = self.held.replace(now);
        [
            Command::Previous,
            Command::Next,
            Command::Activate,
            Command::Left,
            Command::Right,
        ]
        .into_iter()
        .enumerate()
        .filter(|(i, _)| now[*i] && !old[*i])
        .map(|(_, c)| c.action())
        .collect()
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![GardenLabScenario::render_frame(&self.state)]
    }

    fn runtime_diagnostics(&self) -> String {
        use engine_core::plants::{Exposure, Observation, Role, Tissue};
        let s = &self.state;
        let o = s.plants.observe();
        // Grass records keep their original fields; moss adds its last light
        // sample, surface supply, current contact and, on the rock, world point.
        let record = |o: &Observation,
                      t: &Tissue,
                      on_rock: bool,
                      exposure: Option<&Exposure>,
                      world: Option<[f32; 2]>| {
            let mut v = serde_json::json!({
                "id":t.id,"organism":t.organism,"role":format!("{:?}",t.role),
                "cell":t.cell,"health":t.health,"shade":t.shade_ticks,"shortage":t.shortage_ticks,
                "stores":t.stores,"size":t.size,"development":t.development,"invested":t.invested,
                "next_cost":if t.role == Role::Shoot && t.size < 3 { std::array::from_fn::<_,3,_>(|i|u64::from(t.development[i] < o.config.development_per_size[i])) } else { [0;3] },"policy":t.policy,"allocation":t.allocation,"support":format!("{:?}",t.support),
            });
            if t.role == Role::Moss {
                let surface = o.ledger.surfaces.get(&t.id).map(|r| {
                    serde_json::json!({"eligible":r.eligible,"offered":r.offered,"accepted":r.accepted,"overflow":r.overflow})
                });
                v["light"] = serde_json::json!(exposure.map(|e| e.light));
                v["surface"] = serde_json::json!(surface);
                v["contact"] =
                    serde_json::json!(s.surface_contact(on_rock, t).map(|c| format!("{c:?}")));
                v["world"] = serde_json::json!(world);
            }
            v
        };
        let tissues: Vec<_> = o
            .tissues
            .iter()
            .map(|t| record(&o, t, false, s.exposure.get(&t.id), None))
            .collect();
        let mut rock_tissues = Vec::new();
        let rock = s.rock.as_ref().map(|r| {
            let ro = r.plants.observe();
            let motion = s.world().motion(r.body());
            rock_tissues = ro
                .tissues
                .iter()
                .map(|t| {
                    let world = motion.map(|m| {
                        let local = r
                            .terrain()
                            .cell_center(engine_terrain::CellCoord::new(t.cell[0], t.cell[1]));
                        let v = m.position + local.rotate_radians(m.angle);
                        [v.x, v.y]
                    });
                    record(&ro, t, true, r.exposure.get(&t.id), world)
                })
                .collect();
            serde_json::json!({
                "removed":r.removed,"broken":r.broken,"quarter_turns":r.quarter_turns,
                "angle":motion.map(|m| m.angle),"position":motion.map(|m| [m.position.x, m.position.y]),
                "tick":ro.tick,"tissues":rock_tissues,"connections":ro.connections,
                "intake":ro.ledger.intake,"maintenance":ro.ledger.maintenance,
                "growth":ro.ledger.growth,"loss":ro.ledger.loss,"unmet":ro.ledger.unmet,
                "decisions":format!("{:?}",r.plan),
                "work":[r.work.rays,r.work.footprint_tests,r.work.collider_records],
            })
        });
        // Selection walks bed tissue, then standing rock tissue.
        let standing = if s.standing_rock().is_some() {
            &rock_tissues[..]
        } else {
            &[]
        };
        let combined: Vec<_> = tissues.iter().chain(standing).collect();
        let selected = combined
            .get(s.selected % combined.len().max(1))
            .map(|v| (*v).clone());
        format!(
            "garden_lab={}",
            serde_json::json!({
                "biological_state":format!("{:?}",o),"current_decisions":format!("{:?}",s.current_plan()),
                "run_state":if o.tick >= scenario_garden_lab::MAX_TICKS { "complete" } else if s.paused { "lab_paused" } else { "running" },
                "tissue_limit":s.grass.traits.max_tissues,"spread_radius":s.grass.traits.radius,
                "pending_ns":s.pending_ns(),"growth_summary":s.growth_summary(),"admission_summary":s.admission_summary(),"admission_tick":o.ledger.tick,"admission":format!("{:?}",o.ledger.results),
                "configuration":format!("{:?}",o.config),"profile":format!("{:?}",s.grass.traits),"tick":o.tick,"seed":o.seed,"paused":s.paused,"speed":s.speed,
                "intensity":s.intensity,"availability":s.availability,"shade":s.shade,
                "shade_x":s.shade_x,"inspect":s.inspect,"control":s.control,
                "tissues":tissues,"selected_tissue":selected,"connections":o.connections,
                "intake":o.ledger.intake,"maintenance":o.ledger.maintenance,
                "growth":o.ledger.growth,"loss":o.ledger.loss,"unmet":o.ledger.unmet,
                "moss":{
                    "placed":s.moss.is_some(),"summary":s.moss_summary(),
                    "decisions":format!("{:?}",s.moss_plan),"rock":rock,
                },
            })
        )
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thruster_visual_tests::{raster, svg, write_png};
    #[test]
    fn garden_render_contract_keyboard_and_controller_limits_are_edge_triggered() {
        use crate::input::{GamepadInput, GamepadSeatInput};
        use std::{cell::RefCell, rc::Rc};
        for controller in [false, true] {
            let pads = Rc::new(RefCell::new(GamepadInput::default()));
            let mut input = ClientInput::new(Rc::clone(&pads));
            let mut client = GardenLabClientScenario {
                state: GardenLabScenario::init((), 42),
                held: std::cell::Cell::new([false; 5]),
            };
            client.state.paused = true;
            // Up wraps through the moss controls to the tissue-increase row.
            for _ in 0..scenario_garden_lab::CONTROLS.len() - 14 {
                if controller {
                    pads.borrow_mut().set_seat(
                        0,
                        GamepadSeatInput {
                            connected: true,
                            dpad_up: true,
                            ..Default::default()
                        },
                    );
                } else {
                    input.press(GameKey::NesUp);
                }
                let a = client.map_input(&mut input, false);
                client.step(&a, Duration::ZERO);
                assert!(client.map_input(&mut input, false).is_empty());
                pads.borrow_mut().set_seat(0, GamepadSeatInput::default());
                input.release(GameKey::NesUp);
                client.map_input(&mut input, false);
            }
            assert_eq!(client.state.control, 14);
            let before = client.state.plants.clone();
            if controller {
                pads.borrow_mut().set_seat(
                    0,
                    GamepadSeatInput {
                        connected: true,
                        south: true,
                        ..Default::default()
                    },
                );
            } else {
                input.press(GameKey::P1Laser);
            }
            let a = client.map_input(&mut input, false);
            client.step(&a, Duration::ZERO);
            assert_eq!(client.state.grass.traits.max_tissues, 33);
            assert_eq!(client.state.plants, before);
            assert!(client.map_input(&mut input, false).is_empty());
        }
    }
    #[test]
    fn garden_render_contract_moss_patches_rock_and_lifecycle_colors() {
        let output =
            std::env::var_os("SPACEWARS_GARDEN_RENDER_ARTIFACTS").map(std::path::PathBuf::from);
        if let Some(dir) = &output {
            std::fs::create_dir_all(dir).unwrap();
        }
        let near = |p: &slint::Rgb8Pixel, c: [u8; 3]| {
            (i16::from(p.r) - i16::from(c[0])).abs() < 12
                && (i16::from(p.g) - i16::from(c[1])).abs() < 12
                && (i16::from(p.b) - i16::from(c[2])).abs() < 12
        };
        let capture = |name: &str, s: &GardenLabState| -> Vec<[usize; 3]> {
            let frame = GardenLabScenario::render_frame(s);
            let mut counts = Vec::new();
            for (w, h) in [(1280.0, 720.0), (800.0, 480.0)] {
                let viewport = Viewport::new(w, h);
                let pixels = raster(&frame, viewport);
                let count = |c: [u8; 3]| pixels.as_slice().iter().filter(|p| near(p, c)).count();
                counts.push([
                    count([115, 184, 46]),
                    count([92, 97, 107]),
                    count([128, 102, 102]),
                ]);
                let export = svg(&frame, viewport);
                assert!(!export.contains("NaN"));
                if let Some(dir) = &output {
                    let stem = format!("moss-{name}-{}x{}", w as u32, h as u32);
                    write_png(&dir.join(format!("{stem}-raster.png")), &pixels);
                    std::fs::write(dir.join(format!("{stem}-vector.svg")), &export).unwrap();
                }
                if name == "dim-growth" {
                    assert!(export.contains("#73b82e"), "vector moss fill absent");
                }
            }
            if let Some(dir) = &output {
                let state = serde_json::json!({"simulation_seconds":s.plants.observe().tick,
                    "summary":s.moss_summary(),"preview_only":true});
                std::fs::write(
                    dir.join(format!("moss-{name}-state.json")),
                    serde_json::to_vec_pretty(&state).unwrap(),
                )
                .unwrap();
            }
            counts
        };
        let mut s = GardenLabScenario::init((), 42);
        s.command(Command::PlaceMoss);
        s.command(Command::Light);
        for _ in 0..120 {
            s.tick();
        }
        for [moss, rock, _] in capture("dim-growth", &s) {
            assert!(moss > 20, "moss invisible: {moss}");
            assert!(rock > 20, "rock invisible: {rock}");
        }
        s.command(Command::TurnRock);
        s.command(Command::TurnRock);
        let physiology = engine_core::plants::grass::physiology();
        for _ in 0..physiology.grace_ticks + 100 / u32::from(physiology.damage_per_tick) + 2 {
            s.tick();
        }
        for [moss, _, dead] in capture("inverted-dieback", &s) {
            assert!(moss > 20, "surviving moss invisible");
            assert!(dead > 20, "dead moss invisible: {dead}");
        }
        s.command(Command::RemoveRock);
        for [_, rock, _] in capture("rock-removed", &s) {
            assert_eq!(rock, 0, "removed rock still drawn");
        }
    }
    #[test]
    fn garden_render_contract_selected_target_matches_diagnostics() {
        let mut client = GardenLabClientScenario {
            state: GardenLabScenario::init((), 42),
            held: std::cell::Cell::new([false; 5]),
        };
        for _ in 0..10 {
            client.state.tick();
        }
        let mut centers = Vec::new();
        for expected in [1, 2] {
            let details = client.runtime_diagnostics();
            let v: serde_json::Value =
                serde_json::from_str(details.strip_prefix("garden_lab=").unwrap()).unwrap();
            let selected = &v["selected_tissue"];
            assert_eq!(selected["id"], expected);
            assert_eq!(selected, &v["tissues"][(expected - 1) as usize]);
            let frame = GardenLabScenario::render_frame(&client.state);
            assert!(frame.layers.iter().flat_map(|l|&l.primitives).any(|p|matches!(p,engine_common::RenderPrimitive::Text(t) if t.text.starts_with(&format!("Tissue {expected} |")))));
            let viewport = Viewport::new(800.0, 480.0);
            let image = raster(&frame, viewport);
            let pixels: Vec<_> = image
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(_, p)| p.r > 220 && p.g < 100 && p.b > 220)
                .collect();
            assert!(pixels.len() > 20);
            let x = 400.0 + (selected["cell"][0].as_f64().unwrap() + 0.5 - 8.0) * 480.0 / 22.0;
            let y = 240.0 - (selected["cell"][1].as_f64().unwrap() + 0.5 - 4.0) * 480.0 / 22.0;
            assert!(
                pixels
                    .iter()
                    .all(|(i, _)| ((*i % 800) as f64 - x).abs() < 0.6 * 480.0 / 22.0
                        && (*i / 800) as f64 > y - 2.0 * 480.0 / 22.0
                        && ((*i / 800) as f64) < y + 0.6 * 480.0 / 22.0)
            );
            if let Some(dir) =
                std::env::var_os("SPACEWARS_GARDEN_RENDER_ARTIFACTS").map(std::path::PathBuf::from)
            {
                std::fs::create_dir_all(&dir).unwrap();
                write_png(
                    &dir.join(format!("selected-{expected}-800x480-raster.png")),
                    &image,
                );
                std::fs::write(
                    dir.join(format!("selected-{expected}-800x480-vector.svg")),
                    svg(&frame, viewport),
                )
                .unwrap();
            }
            centers.push(y);
            client.state.command(Command::Select);
        }
        assert!(
            centers[1] < centers[0],
            "selection did not move from root to shoot"
        );
    }
    #[test]
    fn garden_render_contract_tissue_visibility_and_candidate_exports() {
        let output =
            std::env::var_os("SPACEWARS_GARDEN_RENDER_ARTIFACTS").map(std::path::PathBuf::from);
        if let Some(dir) = &output {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut s = GardenLabScenario::init((), 42);
        let capture = |name: &str, s: &GardenLabState| {
            if let Some(dir) = &output {
                let o = s.plants.observe();
                let state = serde_json::json!({"seed":o.seed,"simulation_seconds":o.tick,
                    "configuration":format!("{:?}",o.config),"profile":format!("{:?}",s.grass.traits),
                    "observation":format!("{:?}",o),"plan":format!("{:?}",s.plan),
                    "tissues":o.tissues.iter().map(|t| serde_json::json!({
                        "id":t.id,"role":format!("{:?}",t.role),"health":t.health,
                        "shade_ticks":t.shade_ticks,"shortage_ticks":t.shortage_ticks
                    })).collect::<Vec<_>>(),
                    "preview_only":true});
                std::fs::write(
                    dir.join(format!("{name}-state.json")),
                    serde_json::to_vec_pretty(&state).unwrap(),
                )
                .unwrap();
            }
            let frame = GardenLabScenario::render_frame(s);
            for (w, h) in [(1280.0, 720.0), (800.0, 480.0)] {
                let viewport = Viewport::new(w, h);
                let pixels = raster(&frame, viewport);
                let green = pixels
                    .as_slice()
                    .iter()
                    .filter(|p| p.g > 180 && p.r < 110 && p.b < 130)
                    .count();
                if name == "growth" || name == "regrowth" {
                    assert!(green > 20, "{name} {w}x{h}: {green} green pixels");
                }
                if name == "cut" {
                    assert_eq!(green, 0);
                }
                let count = |f: fn(&slint::Rgb8Pixel) -> bool| {
                    pixels.as_slice().iter().filter(|p| f(p)).count()
                };
                match name {
                    "growth" => {
                        assert!(
                            count(|p| p.r > 160 && p.g > 190 && p.b > 230) > 20,
                            "roots/runners invisible"
                        );
                        assert!(
                            count(|p| p.r < 110 && p.g > 160 && p.b > 230) > 20,
                            "connections invisible"
                        );
                    }
                    "stressed" => assert!(
                        count(|p| p.r > 220 && p.g > 130 && p.g < 190 && p.b < 70) > 20,
                        "stress invisible"
                    ),
                    "dead" => assert!(
                        count(|p| p.r > 115
                            && p.r < 145
                            && p.g > 90
                            && p.g < 115
                            && p.b > 90
                            && p.b < 115)
                            > 20,
                        "death invisible"
                    ),
                    _ => {}
                }
                let vector = crate::render::scene_primitives_from_frames(
                    std::slice::from_ref(&frame),
                    viewport,
                );
                assert!(vector.iter().all(|p| !p.commands.contains("NaN")));
                let export = svg(&frame, viewport);
                if name == "growth" {
                    assert!(
                        export.contains("#40ff4d"),
                        "vector shoot fill absent: {export}"
                    );
                }
                if let Some(dir) = &output {
                    let stem = format!("{name}-{}x{}", w as u32, h as u32);
                    write_png(&dir.join(format!("{stem}-raster.png")), &pixels);
                    std::fs::write(dir.join(format!("{stem}-vector.svg")), export).unwrap();
                }
            }
        };
        for _ in 0..10 {
            s.tick();
        }
        capture("growth", &s);
        s.command(Command::Shade);
        let physiology = engine_core::plants::grass::physiology();
        let shade_started = s.plants.observe().tick;
        let damage_ticks = 100u64.div_ceil(u64::from(physiology.damage_per_tick));
        let deadline = shade_started + u64::from(physiology.grace_ticks) + damage_ticks + 2;
        // Capture actual damage while the same shaded shoot is still alive.
        // The shared minute-scale grace period must not turn this into a
        // healthy or already-dead picture with a misleading label.
        let stressed_shoot = loop {
            let o = s.plants.observe();
            if let Some(t) = o.tissues.iter().find(|t| {
                t.role == engine_core::plants::Role::Shoot
                    && t.shade_ticks > physiology.grace_ticks
                    && t.health > 0
                    && t.health < 100
            }) {
                break t.id;
            }
            assert!(
                o.tick < deadline,
                "no living damaged shoot before {deadline}"
            );
            s.tick();
        };
        let stressed_tick = s.plants.observe().tick;
        capture("stressed", &s);
        loop {
            let o = s.plants.observe();
            let shoot = o
                .tissues
                .iter()
                .find(|t| t.id == stressed_shoot)
                .expect("the stressed shoot must remain observable through death");
            if shoot.health == 0 {
                break;
            }
            assert!(o.tick < deadline, "shaded shoot survived beyond {deadline}");
            s.tick();
        }
        println!(
            "Garden render seed=42 shade-start={shade_started} stressed={stressed_tick} dead={} grace={} damage={}",
            s.plants.observe().tick,
            physiology.grace_ticks,
            physiology.damage_per_tick
        );
        capture("dead", &s);
        s.command(Command::Reset);
        for _ in 0..10 {
            s.tick();
        }
        s.command(Command::Cut);
        capture("cut", &s);
        s.tick();
        capture("regrowth", &s);
        s.command(Command::Reset);
        for tick in 1..=480 {
            s.tick();
            if [2, 60, 120, 186, 240, 241, 300, 360, 403, 480].contains(&tick) {
                capture(&format!("maturation-{tick}"), &s);
            }
            if tick == 240 {
                s.command(Command::Cut);
                capture("maturation-240-cut", &s);
            }
        }
    }
}
