use super::*;

#[test]
fn jetpack_charge_is_finite_consumed_by_lift_and_recharged_only_after_landing() {
    let (mut world, mut actor) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut world, &mut actor);
    assert!(actor.equip_jetpack(0.15));
    assert!(!actor.equip_jetpack(f32::NAN));
    let start = actor.snapshot(&world).unwrap().motion.position.y;
    let mut peak = start;
    let held = SpacelingControl {
        jump_held: true,
        ..Default::default()
    };
    for _ in 0..180 {
        tick(&mut world, &mut actor, held, GRAVITY);
        peak = peak.max(actor.snapshot(&world).unwrap().motion.position.y);
        assert!((0.0..=1.0).contains(&actor.jetpack().unwrap().charge));
    }
    let pack = actor.jetpack().unwrap();
    assert!(peak > start + 3.0);
    assert_eq!(pack.charge, 0.0);
    assert!((pack.burn_seconds - 0.15 * crate::spaceling::jetpack::BURN_SECONDS).abs() < 0.001);
    for _ in 0..600 {
        tick(&mut world, &mut actor, held, GRAVITY);
    }
    assert!(actor.snapshot(&world).unwrap().grounded());
    assert_eq!(actor.snapshot(&world).unwrap().jumps, 1);
    assert_eq!(
        actor.jetpack().unwrap().charge,
        0.0,
        "holding does not refill and relaunch"
    );
    for _ in 0..300 {
        tick(&mut world, &mut actor, SpacelingControl::default(), GRAVITY);
    }
    assert_eq!(actor.jetpack().unwrap().charge, 1.0);
}

#[test]
fn a_tapped_jump_keeps_its_height_and_jetpack_clone_replays() {
    let (mut normal_world, mut normal) = floor_fixture(0.0, Vec2::ZERO);
    settle(&mut normal_world, &mut normal);
    let mut world = normal_world.clone();
    let mut actor = normal.clone();
    assert!(actor.equip_jetpack(1.0));
    for i in 0..150 {
        let control = SpacelingControl {
            jump_held: i == 0,
            ..Default::default()
        };
        tick(&mut normal_world, &mut normal, control, GRAVITY);
        tick(&mut world, &mut actor, control, GRAVITY);
        assert_eq!(actor.snapshot(&world), normal.snapshot(&normal_world));
    }
    let mut copy_world = world.clone();
    let mut copy = actor.clone();
    for i in 0..240 {
        let control = SpacelingControl {
            jump_held: i < 90,
            walk: if i < 120 { -0.4 } else { 0.4 },
        };
        tick(&mut world, &mut actor, control, GRAVITY);
        tick(&mut copy_world, &mut copy, control, GRAVITY);
        assert_eq!(actor.snapshot(&world), copy.snapshot(&copy_world));
        assert_eq!(actor.jetpack(), copy.jetpack());
    }
}
