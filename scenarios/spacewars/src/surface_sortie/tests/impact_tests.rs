use super::super::impact::{ImpactKind, SurfaceImpactAction};
use super::*;
const DT: Duration = Duration::from_nanos(16_666_667);

fn strike(state: &mut SurfaceSortieState, kind: ImpactKind) {
    SurfaceSortieScenario::step(
        state,
        &[SurfaceImpactAction {
            held: true,
            kind,
            oblique: false,
        }
        .encode(PlayerId::PLAYER_1)],
        DT,
    );
    // Spawning a rock must not directly change health or the vehicle form.
    assert_eq!(state.damage_observation(0).hits, 0);
    assert_eq!(state.world.ships[0].form, ShipForm::Ship);
    for _ in 0..180 {
        SurfaceSortieScenario::step(
            state,
            &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
        if state.damage_observation(0).hits > 0 {
            break;
        }
    }
    assert_eq!(state.damage_observation(0).last_source, Some("asteroid"));
    assert!(state.damage_observation(0).last_damage_percent > 0.0);
    assert!(state.terrain_diagnostics().issues.is_empty());
}

#[test]
fn real_light_and_heavy_contacts_damage_flying_and_parked_ships() {
    for flying in [false, true] {
        for occupied in [false, true] {
            if flying && !occupied {
                continue;
            }
            for kind in [ImpactKind::Light, ImpactKind::Heavy] {
                let mut s = if flying {
                    SurfaceSortieScenario::init_material_flight(
                        42,
                        1,
                        &[(
                            PlayerId::PLAYER_1,
                            pilot::MaterialFlightStart {
                                bearing: 0.0,
                                altitude: 100.0,
                                radial_speed: 0.0,
                                lateral_speed: 0.0,
                                heading_offset: 0.0,
                            },
                        )],
                    )
                } else {
                    SurfaceSortieScenario::init_material(42, 1)
                };
                idle(&mut s, if flying { 1 } else { 120 });
                if !occupied {
                    assert_eq!(s.try_transfer(0), TransferResult::Exited);
                    idle(&mut s, 1);
                }
                let actor = s.pilots[0].body.as_ref().map(|b| b.body());
                strike(&mut s, kind);
                let r = s.observation(0).recovery.unwrap();
                if kind == ImpactKind::Light {
                    assert!(s.world.ships[0].life > 0.0);
                    assert_eq!(r.ships_lost, 0);
                } else {
                    assert_eq!(r.ships_lost, 1);
                    assert_eq!(r.pod_ejections, u64::from(occupied));
                    assert_eq!(s.world.ships[0].form == ShipForm::EscapePod, occupied);
                    assert_eq!(s.pilots[0].body.as_ref().map(|b| b.body()), actor);
                    assert!(!s.observation(0).controls_armed);
                }
            }
        }
    }
}

#[test]
fn impact_requires_a_release_valid_seat_and_cooldown_and_does_not_advance_on_pause() {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    let held = SurfaceImpactAction {
        held: true,
        kind: ImpactKind::Light,
        oblique: false,
    };
    for _ in 0..20 {
        SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], DT);
    }
    assert!(!s.observation(0).controls_armed);
    assert_eq!(s.damage_observation(0).strikes, 0);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
        DT,
    );
    assert!(s.observation(0).controls_armed);
    SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], Duration::ZERO);
    assert_eq!(s.damage_observation(0).strikes, 0);
    SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_2)], DT);
    assert_eq!(s.damage_observation(0).strikes, 0);
    for _ in 0..30 {
        SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], DT);
    }
    assert_eq!(s.damage_observation(0).strikes, 1);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
        DT,
    );
    SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], DT);
    assert_eq!(s.damage_observation(0).strikes, 1);
    idle(&mut s, 180);
    SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], DT);
    assert_eq!(
        s.damage_observation(0).strikes,
        1,
        "holding through cooldown is not another press"
    );
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
        DT,
    );
    SurfaceSortieScenario::step(&mut s, &[held.encode(PlayerId::PLAYER_1)], DT);
    assert_eq!(s.damage_observation(0).strikes, 2);
    assert!(
        SurfaceImpactAction::decode(&Action::scenario(0x5355_0004, vec![0, 2, 0, 0])).is_none()
    );
}
