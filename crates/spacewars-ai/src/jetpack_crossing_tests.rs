use super::*;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieScenario, pilot::PilotMotion},
};
use serde_json::Value;

fn number(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

fn vector(value: &Value) -> Vec2 {
    Vec2::new(number(&value["x"]), number(&value["y"]))
}

fn motion(value: &Value) -> PilotMotion {
    PilotMotion {
        position: vector(&value["position"]),
        velocity: vector(&value["velocity"]),
        angle: number(&value["angle"]),
        spin: number(&value["spin"]),
    }
}

fn recorded_observation(sample: &Value) -> JetpackCrossingObservation {
    // Restore the consumed fields used by the crossing controller. Other
    // scenario fields come from a valid observation and cannot affect arrival.
    let state = SurfaceSortieScenario::init_material_jetpack(42, 1);
    let mut o = state.jetpack_crossing_observation(0, CrossingDirection::Left);
    let p = &sample["pilot"];
    assert_eq!(p["owner"], "player_2");
    assert_eq!(p["location"], "on_foot");
    o.pilot.tick = sample["tick"].as_u64().unwrap();
    o.pilot.owner = PlayerId::PLAYER_2;
    o.pilot.location = PilotLocation::OnFoot;
    o.pilot.controls_armed = p["controls_armed"].as_bool().unwrap();
    o.pilot.queries_ready = p["queries_ready"].as_bool().unwrap();
    o.pilot.actor = Some(motion(&p["actor"]));
    o.pilot.actor_up = vector(&p["actor_up"]);
    o.pilot.supported_planet = p["supported_planet"].as_u64().map(|n| n as usize);
    o.pilot.balanced = p["balanced"].as_bool().unwrap();
    o.pilot.relative_speed = number(&p["relative_speed"]);
    o.pilot.planet.index = p["planet"]["index"].as_u64().unwrap() as usize;
    o.pilot.planet.revision = p["planet"]["revision"].as_u64().unwrap();
    o.pilot.planet.motion = motion(&p["planet"]["motion"]);
    let plan = &sample["plan"];
    o.plan = Some(CrossingPlan {
        planet: plan["planet"].as_u64().unwrap() as usize,
        revision: plan["revision"].as_u64().unwrap(),
        direction: CrossingDirection::Left,
        start: vector(&plan["start"]),
        destination: vector(&plan["destination"]),
        cruise_radius: number(&plan["cruise_radius"]),
        anchor: CrossingAnchor::GroundGap {
            from: plan["anchor"]["GroundGap"]["from"].as_u64().unwrap() as u16,
            to: plan["anchor"]["GroundGap"]["to"].as_u64().unwrap() as u16,
        },
    });
    assert_eq!(plan["direction"], "Left");
    let j = &sample["jetpack"];
    o.charge = Some(number(&j["charge"]));
    o.reference_velocity = vector(&j["reference_velocity"]);
    o.burning = j["burning"].as_bool().unwrap();
    o.burn_seconds = number(&j["burn_seconds"]);
    o.gravity = vector(&j["gravity"]);
    o.surveyed = false;
    o
}

fn descending(o: &JetpackCrossingObservation) -> JetpackCrossingPilot {
    let mut bot = JetpackCrossingPilot::traversal(
        BrainReset {
            actor: o.pilot.owner,
            episode_seed: 42,
        },
        o.plan.unwrap(),
    );
    bot.telemetry.goal = CrossingGoal::Descend;
    bot
}

#[test]
fn recorded_gap_landings_require_destination_footing_in_every_world_frame() {
    let data: Value =
        serde_json::from_str(include_str!("../tests/fixtures/ground-gap-arrivals.json")).unwrap();
    for sample in data["samples"].as_array().unwrap() {
        for angle in [0.0, 1.3, -2.8] {
            let mut o = recorded_observation(sample);
            let shift = Vec2::new(-213.0, 407.0);
            let drift = Vec2::new(13.0, -27.0);
            let moved = |m: &mut PilotMotion| {
                m.position = m.position.rotate_radians(angle) + shift;
                m.velocity = m.velocity.rotate_radians(angle) + drift;
                m.angle += angle;
            };
            moved(&mut o.pilot.planet.motion);
            moved(o.pilot.actor.as_mut().unwrap());
            o.pilot.actor_up = o.pilot.actor_up.rotate_radians(angle);
            o.reference_velocity = o.reference_velocity.rotate_radians(angle) + drift;
            o.gravity = o.gravity.rotate_radians(angle);
            let mut bot = descending(&o);
            let before = o.clone();
            let action = bot.step(&o);
            let expected = sample["expected_arrival"].as_bool().unwrap();
            assert_eq!(
                bot.telemetry.goal == CrossingGoal::Complete,
                expected,
                "tick {} in frame {angle}",
                o.pilot.tick
            );
            assert_eq!(bot.telemetry.crossings, u32::from(expected));
            assert_eq!(
                bot.telemetry.completed_tick,
                expected.then_some(o.pilot.tick)
            );
            assert_eq!(
                bot.step(&o),
                action,
                "duplicate observations do not advance"
            );
            assert_eq!(o, before, "arrival only consumes observations");
        }
    }
}

#[test]
fn rejecting_a_source_side_landing_does_not_extend_the_crossing_deadline() {
    let data: Value =
        serde_json::from_str(include_str!("../tests/fixtures/ground-gap-arrivals.json")).unwrap();
    let mut o = recorded_observation(&data["samples"][3]);
    let mut bot = descending(&o);
    bot.step(&o);
    assert_eq!(bot.telemetry.goal, CrossingGoal::Descend);
    o.pilot.tick += 90 * 60 + 1;
    assert_eq!(bot.step(&o), SurfaceSortieAction::default());
    assert_eq!(bot.telemetry.goal, CrossingGoal::Blocked);
    assert_eq!(bot.telemetry.crossings, 0);
    assert_eq!(bot.telemetry.completed_tick, None);
}
