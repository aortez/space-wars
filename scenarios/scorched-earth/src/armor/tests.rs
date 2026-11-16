use super::*;
use engine_material::{Part, SourceError};

/// A duel whose tanks have settled on their hills and hold their fire.
fn settled(suspension: bool) -> ScorchedState {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            suspension,
            cpu_driving: false,
            ..Default::default()
        },
        42,
    );
    for tank in &mut state.duel.tanks {
        tank.next_shot = u64::MAX;
    }
    for _ in 0..90 {
        ScorchedScenario::step(&mut state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
    state
}

fn position(state: &ScorchedState, slot: usize) -> Vec2 {
    state
        .physics
        .motion(state.tanks[slot].body)
        .unwrap()
        .position
}

fn last_hit(state: &ScorchedState, slot: usize) -> serde_json::Value {
    serde_json::to_value(&state.bots[slot]).unwrap()["last_hit"].clone()
}

#[test]
fn a_shell_charges_the_tank_it_hits_once_then_bursts_once() {
    for suspension in [false, true] {
        let mut state = settled(suspension);
        let hit = state
            .physics
            .cast_ray(
                position(&state, 0) + Vec2::Y * 6.0,
                Vec2::new(0.0, -1.0),
                RayCastOptions {
                    max_distance: 10.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(hit.collider.entity, state.tanks[0].body.entity);
        let mut source = shell(7, Some(99), 30.0);
        let struck = state.duel.strike(
            &mut state.physics,
            Vec2::new(0.0, -30.0),
            &mut source,
            hit.point,
            &hit,
        );
        let ledger = source.ledger();
        assert!(ledger.kinetic.absorbed > 0.0, "sprung={suspension}");
        assert!(ledger.payload.absorbed > 0.0);
        assert!(ledger.payload.delivered <= f64::from(PAYLOAD));
        assert_eq!(struck.absorbed[1], 0.0);
        assert_eq!(struck.unmodeled, 0.0);
        let charged = ledger.kinetic.absorbed + ledger.payload.absorbed;
        assert!((f64::from(struck.absorbed[0]) - charged).abs() < 0.01);
        // Health is the armor's integrity.
        let tank = state.tanks[0];
        assert_eq!(tank.health, tank.armor.normalized_health());
        let lost = 100.0 * f64::from(struck.absorbed[0]) / armor().remaining_precise();
        assert!((f64::from(100.0 - tank.health) - lost).abs() < 0.001);
        // The shell's parts are spent.
        assert!(matches!(
            source.pass(1, &armor(), Face::new(1.0, ARMOR_SECTION), 10.0),
            Err(SourceError::Spent(Part::Kinetic))
        ));
        assert!(matches!(
            source.release(&[]),
            Err(SourceError::Spent(Part::Payload))
        ));
    }
}

#[test]
fn a_hit_from_a_shell_nobody_fired_draws_no_answer() {
    // Like a bombardment shell, it has no flight for the bots to answer.
    let mut state = settled(false);
    state.duel.shells.push(Shell {
        owner: 1,
        position: position(&state, 0) + Vec2::Y * 6.0,
        velocity: Vec2::new(0.0, -30.0),
        age: 10,
        flight: None,
        source: shell(500, None, 30.0),
    });
    let impacts = state.impacts;
    for _ in 0..60 {
        ScorchedScenario::step(&mut state, &[], Duration::from_secs_f64(1.0 / 60.0));
        if state.impacts > impacts {
            break;
        }
    }
    assert!(state.tanks[0].health < 100.0);
    assert!(last_hit(&state, 0).is_null());
}

#[test]
fn a_burst_between_the_tanks_divides_one_payload() {
    let mut state = settled(false);
    // Level tanks side by side, mirrored about the burst.
    let at = position(&state, 0);
    for (slot, offset) in [(0, 0.0), (1, 6.0)] {
        let body = state.tanks[slot].body;
        state
            .physics
            .set_pose(body, at + Vec2::X * offset, 0.0, true);
    }
    let mut source = charge(1, None);
    let absorbed = state
        .duel
        .release_payload(&state.physics, at + Vec2::X * 3.0, &mut source);
    assert!(absorbed[0] > 0.0);
    assert!((absorbed[0] - absorbed[1]).abs() < 0.01 * absorbed[0]);
    let ledger = source.ledger().payload;
    assert!(ledger.delivered <= f64::from(PAYLOAD));
    assert!((f64::from(absorbed[0] + absorbed[1]) - ledger.absorbed).abs() < 0.01);
    assert!(matches!(
        source.release(&[]),
        Err(SourceError::Spent(Part::Payload))
    ));
}

#[test]
fn a_burst_beyond_reach_spares_the_tanks_and_their_bots() {
    for suspension in [false, true] {
        let mut state = settled(suspension);
        let above = position(&state, 0) + Vec2::Y;
        // No circle reaches a unit above its tank's origin.
        state.explode_from(above + Vec2::Y * (BLAST_REACH + 0.1), Some(1));
        assert_eq!(state.tanks[0].health, 100.0, "sprung={suspension}");
        assert!(last_hit(&state, 0).is_null());
        state.explode_from(above + Vec2::Y * 3.0, Some(1));
        let lost = 100.0 - state.tanks[0].health;
        assert!(lost > 0.0);
        assert_eq!(last_hit(&state, 0)[1], serde_json::json!(lost));
        // A tank's own burst costs it health without a notice.
        let health = state.tanks[0].health;
        state.explode_from(above + Vec2::Y * 3.0, Some(0));
        assert!(state.tanks[0].health < health);
        assert_eq!(last_hit(&state, 0)[1], serde_json::json!(lost));
    }
}

#[test]
fn an_ended_tank_keeps_no_health_while_its_armor_takes_its_share() {
    let mut state = settled(false);
    // As the floor rule leaves it.
    state.duel.tanks[0].health = 0.0;
    let near = position(&state, 0) + Vec2::Y * 2.0;
    state.explode(near);
    assert_eq!(state.tanks[0].health, 0.0);
    assert!(state.tanks[0].armor.integrity() < 1.0);
}

#[test]
fn a_shell_stopped_by_terrain_reports_its_hit_without_charging_it() {
    let mut state = settled(false);
    let between = (position(&state, 0) + position(&state, 1)) * 0.5;
    state.duel.shells.push(Shell {
        owner: 1,
        position: Vec2::new(between.x, 20.0),
        velocity: Vec2::new(0.0, -20.0),
        age: 10,
        flight: None,
        source: shell(500, None, 20.0),
    });
    let impacts = state.impacts;
    for _ in 0..300 {
        ScorchedScenario::step(&mut state, &[], Duration::from_secs_f64(1.0 / 60.0));
        if state.impacts > impacts {
            break;
        }
    }
    assert_eq!(state.duel.impact_events.len(), 1);
    let event = state.duel.impact_events[0];
    assert_eq!(event.source.id, 500);
    assert!(event.unmodeled > 0.0);
    assert_eq!(event.absorbed, [0.0; 2]);
    assert_eq!(event.ledger.kinetic.delivered, 0.0);
    assert_eq!(event.ledger.payload.delivered, 0.0);
    assert_eq!(state.tanks.map(|tank| tank.health), [100.0; 2]);
}

#[test]
fn a_shell_bursts_outside_every_tank() {
    let targets = [Target {
        key: 0,
        center: Vec2::ZERO,
        radius: 1.0,
        thickness: TANK_THICKNESS,
    }];
    let moved = outside(&targets, Vec2::new(0.5, 0.0));
    assert!(moved.x > 1.0 && moved.x < 1.01 && moved.y == 0.0);
    assert!(outside(&targets, Vec2::ZERO).y > 1.0);
    assert_eq!(outside(&targets, Vec2::new(2.0, 0.0)), Vec2::new(2.0, 0.0));
    // A burst on the edge would otherwise meet the tank in every direction.
    let reach = blast_reach(moved, BLAST_REACH, TANK_THICKNESS, &targets).unwrap();
    assert!(reach[0].coupling < 0.51);
}
