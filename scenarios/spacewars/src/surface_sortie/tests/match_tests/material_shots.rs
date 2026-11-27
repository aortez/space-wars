//! Supplied rounds as material weapon shots against a match's ship, pod and
//! pilot on foot (#330).
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    Ship,
    Pod,
    OnFoot,
}

/// Seat 1 at rest in `target`'s form, 200 units over the first planet with
/// gravity off, and seat 0's ship 40 units off.
pub(super) fn fixture(target: Target) -> SurfaceSortieState {
    let state = arrange(round(), target);
    assert_eq!(health(&state, target), 100.0);
    state
}

/// `state` laid out as [`fixture`] lays out a match.
pub(super) fn arrange(mut state: SurfaceSortieState, target: Target) -> SurfaceSortieState {
    weightless(&mut state);
    let center = state.world.planets[0].position + Vec2::Y * 200.0;
    let shooter = &mut state.world.ships[0];
    shooter.position = center - Vec2::X * 40.0 - SHIP_PIVOT;
    shooter.velocity = Vec2::ZERO;
    shooter.omega = 0.0;
    let behind = shooter.position + Vec2::Y * 60.0;
    let ship = &mut state.world.ships[1];
    ship.velocity = Vec2::ZERO;
    ship.omega = 0.0;
    match target {
        Target::Ship => ship.position = center - SHIP_PIVOT,
        Target::Pod => {
            ship.change_to_escape_pod();
            ship.life = POD_HULL_HEALTH;
            ship.position = center - POD_PIVOT;
        }
        Target::OnFoot => {
            // The empty ship waits behind the attack, out of the pilot's way.
            ship.position = behind;
            external(&mut state, 1, center);
        }
    }
    idle(&mut state, 1);
    state
}

/// The body a round meets in `target`'s form.
pub(super) fn entity(target: Target) -> MechanicalEntity {
    match target {
        Target::Ship | Target::Pod => MechanicalEntity::Ship(1),
        Target::OnFoot => MechanicalEntity::Spaceling(1),
    }
}

pub(super) fn health(state: &SurfaceSortieState, target: Target) -> f32 {
    let ship = &state.world.ships[1];
    let form = match target {
        Target::Ship => ShipForm::Ship,
        Target::Pod => ShipForm::EscapePod,
        Target::OnFoot => return vitals(state, 1).health,
    };
    if ship.form == form && !ship.dead {
        ship.life
    } else {
        0.0
    }
}

/// A round that a laser destroys in the step it strikes lands no hit and counts
/// none. The old rules spared a pod such a hit, though not a ship.
#[test]
fn a_round_a_laser_destroys_as_it_strikes_lands_no_hit() {
    let step = |state: &mut SurfaceSortieState, laser: bool| {
        let action = combat::SurfaceWeaponAction {
            laser,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        SurfaceSortieScenario::step(state, &[action], Duration::from_nanos(16_666_667));
    };
    let alive = |state: &SurfaceSortieState| {
        state
            .world
            .debris
            .iter()
            .any(|d| d.kind == DebrisKind::Shell && !d.dead)
    };
    for target in [Target::Ship, Target::Pod] {
        let mut state = fixture(target);
        let shooter = &mut state.world.ships[0];
        shooter.direction = Vec2::X;
        shooter.rotation_radians = rotation_for_direction(Vec2::X);
        step(&mut state, true);
        let on = state.world.laser_hits.iter();
        let on = on
            .filter(|hit| hit.target == LaserTarget::Ship(1))
            .map(|hit| hit.point)
            .next()
            .expect("the laser must meet the target");
        // A round on the beam, half a unit short of where the beam meets the
        // target, worn to just above breaking.
        let mut shell = DebrisState::new_shell(
            0,
            state.world.tick,
            on - Vec2::X * (CANNON_SHELL_RADIUS + 0.5),
            Vec2::X * 300.0,
            0.0,
        );
        shell.inertial_mass = Some(weapons::ROUND_MASS);
        shell.life = shell.life_max * (DEBRIS_DEATH_LIFE_FACTOR + 0.01);
        state.world.debris.push(shell);
        // Out of the laser, the round strikes in step `strike`.
        let mut unlit = state.clone();
        let strike = (1..=3).find(|_| {
            step(&mut unlit, false);
            let hits = unlit.world.shot_hits.values();
            hits.filter(|hit| hit.target == entity(target))
                .any(|hit| hit.health > 0.0)
        });
        let strike = strike.unwrap_or_else(|| panic!("{target:?}: the round must strike"));
        // Lit for that step alone, the laser destroys the round as it strikes,
        // and the target loses nothing to it.
        let before = health(&state, target);
        let hits = state.combat_telemetry(0).cannon_hits;
        for _ in 1..strike {
            step(&mut state, false);
        }
        assert!(alive(&state), "{target:?}");
        step(&mut state, true);
        let met = state
            .world
            .laser_hits
            .iter()
            .any(|hit| matches!(hit.target, LaserTarget::Debris(_)));
        assert!(met, "{target:?}: the laser must meet the round");
        assert!(!alive(&state), "{target:?}");
        assert!(state.world.shot_hits.is_empty(), "{target:?}");
        assert_eq!(health(&state, target), before, "{target:?}");
        assert_eq!(state.combat_telemetry(0).cannon_hits, hits, "{target:?}");
    }
}

/// A pod's or pilot's protection from rounds ends in the tick that the old
/// rules' does: covered while the next tick is before the deadline, and open
/// from then on.
#[test]
fn ejection_protection_from_rounds_ends_when_the_old_rules_do() {
    for target in [Target::Pod, Target::OnFoot] {
        let mut state = fixture(target);
        let tick = state.world.tick;
        for (until, covered) in [(tick + 1, false), (tick + 2, true)] {
            state.pilots[1]
                .vitals
                .as_mut()
                .unwrap()
                .protected_until_tick = until;
            let exposed = match_rules::exposed(&state.world, &state.pilots);
            let body = exposed
                .iter()
                .find(|body| body.entity == entity(target))
                .unwrap();
            assert_eq!(body.protected, covered, "{target:?} until {until}");
        }
    }
}

/// A payload that bursts beside a pod or a pilot on foot charges it, as a
/// shell's hit does, unless ejection protection covers it.
#[test]
fn a_blast_charges_a_pod_and_a_pilot_on_foot_unless_protection_covers_them() {
    for target in [Target::Pod, Target::OnFoot] {
        for covered in [false, true] {
            let case = format!("{target:?}, covered {covered}");
            let mut state = fixture(target);
            if covered {
                let tick = state.world.tick;
                let vitals = state.pilots[1].vitals.as_mut().unwrap();
                vitals.protect_ejection(tick);
            }
            // A round falls on a rock below and beside the target, which
            // leaves the target in the open, and bursts there.
            let motion = state.impact_motion(1).unwrap();
            let center = motion.center_of_mass;
            state.world.debris.push(DebrisState::new(
                DebrisKind::Asteroid,
                center + Vec2::new(-6.0, -3.0),
                motion.velocity,
                1.5,
                0.01,
                Color::WHITE,
            ));
            let mut shell = DebrisState::new_shell(
                0,
                state.world.tick,
                center + Vec2::new(-6.0, 12.0),
                motion.velocity - Vec2::Y * 300.0,
                0.0,
            );
            shell.inertial_mass = Some(weapons::ROUND_MASS);
            shell.blast = Some(crate::shots::TEST_BLAST);
            state.world.debris.push(shell);
            let before = health(&state, target);
            for _ in 0..30 {
                idle(&mut state, 1);
                if !state.world.blast_hits.is_empty() {
                    break;
                }
            }
            assert!(!state.world.blast_hits.is_empty(), "{case}: it must burst");
            let hits = state.world.blast_hits.iter();
            let blasted: f32 = hits
                .filter(|hit| hit.target == entity(target))
                .map(|hit| hit.health)
                .sum();
            assert_eq!(blasted > 0.0, !covered, "{case}: {blasted}");
            let lost = before - health(&state, target);
            assert!((lost - blasted).abs() < 1.0e-3, "{case}: {lost} {blasted}");
        }
    }
}

/// A round from 8 units off the target's center of mass, closing at 300.
fn launch(state: &mut SurfaceSortieState) {
    launch_at(state, 300.0);
}

/// A round from 8 units off the target's center of mass, closing at `speed`.
fn launch_at(state: &mut SurfaceSortieState, speed: f32) {
    let motion = state.impact_motion(1).unwrap();
    let mut shell = DebrisState::new_shell(
        0,
        state.world.tick,
        motion.center_of_mass - Vec2::X * 8.0,
        motion.velocity + Vec2::X * speed,
        0.0,
    );
    shell.inertial_mass = Some(weapons::ROUND_MASS);
    state.world.debris.push(shell);
}

/// Pilot tissue, tough enough to last 2.87 seconds in the laser, also survives
/// a slow round: a round kills a pilot on foot on the first hit only when it
/// closes at about 236 or faster. The old rules' round killed one at every
/// speed from 50 to 300.
#[test]
fn a_round_kills_a_pilot_on_foot_at_once_only_when_it_closes_fast() {
    for (speed, killed) in [(240.0, true), (230.0, false)] {
        let mut state = fixture(Target::OnFoot);
        launch_at(&mut state, speed);
        let mut charged = None;
        for _ in 0..30 {
            idle(&mut state, 1);
            let hits = state.world.shot_hits.values();
            let mut hits = hits.filter(|hit| hit.target == entity(Target::OnFoot));
            charged = charged.or(hits.next().map(|hit| hit.health));
        }
        let charged = charged.unwrap_or_else(|| panic!("{speed}: the round must strike"));
        let left = health(&state, Target::OnFoot);
        assert_eq!(
            left == 0.0,
            killed,
            "{speed}: charged {charged}, left {left}"
        );
    }
}

#[test]
fn rounds_fail_a_ship_on_the_fourth_hit_a_pod_on_the_second_and_a_pilot_on_the_first() {
    for (target, losses) in [(Target::Ship, 4), (Target::Pod, 2), (Target::OnFoot, 1)] {
        let mut state = fixture(target);
        let mut hits = Vec::new();
        for elapsed in 0_u64..60 * 30 {
            if elapsed.is_multiple_of(120) {
                launch(&mut state);
            }
            let before = health(&state, target);
            idle(&mut state, 1);
            let lost = before - health(&state, target);
            // The target loses what the round's shot charged it, and no old
            // damage besides.
            let charged = state.world.shot_hits.values();
            let charged = charged
                .filter(|hit| hit.target == entity(target))
                .map(|hit| hit.health)
                .sum::<f32>();
            assert!((lost - charged.min(before)).abs() < 1.0e-3, "{target:?}");
            if lost > 0.0 {
                hits.push(lost);
            }
            if health(&state, target) == 0.0 {
                break;
            }
        }
        assert_eq!(hits.len(), losses, "{target:?}: {hits:?}");
        assert_eq!(
            state.combat_telemetry(0).cannon_hits,
            losses as u64,
            "{target:?}"
        );
        if target != Target::OnFoot {
            assert_eq!(
                vitals(&state, 1).health,
                PILOT_HEALTH,
                "the hull shields its pilot"
            );
        }
    }
}

#[test]
fn ejection_protection_covers_a_pod_and_a_pilot_from_rounds() {
    for target in [Target::Pod, Target::OnFoot] {
        let mut state = fixture(target);
        let tick = state.world.tick;
        state.pilots[1]
            .vitals
            .as_mut()
            .unwrap()
            .protect_ejection(tick);
        launch(&mut state);
        let mut charged = Vec::new();
        for _ in 0..30 {
            idle(&mut state, 1);
            charged.extend(state.world.shot_hits.values().copied());
        }
        // The round stopped on the target and charged it nothing.
        let [hit] = charged[..] else {
            panic!("{target:?}: {charged:?}");
        };
        assert_eq!(hit.target, entity(target), "{target:?}");
        assert_eq!(hit.health, 0.0, "{target:?}");
        assert_eq!(health(&state, target), 100.0, "{target:?}");
        assert_eq!(state.combat_telemetry(0).cannon_hits, 1, "{target:?}");
        assert!(
            state
                .world
                .debris
                .iter()
                .all(|d| d.kind != DebrisKind::Shell),
            "{target:?}: the round stops on what it hits"
        );
    }
}
