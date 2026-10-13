use super::*;

fn frame() -> motion::SurfaceFrame {
    motion::SurfaceFrame {
        position: Vec2::new(20.0, 30.0),
        angle: 0.3,
        linear_velocity: Vec2::new(1.0, -2.0),
        angular_velocity: 0.2,
    }
}

fn sample(id: SurfaceForceId, owner: SurfaceAllegiance) -> ForceSample {
    ForceSample {
        id,
        allegiance: owner,
        position: frame().position + Vec2::Y * 30.0,
        support: Some(0),
        retain_contact_gap: true,
    }
}

#[test]
fn contact_grace_follows_the_planet_but_cannot_be_renewed_by_polling() {
    let mut forces = SurfaceForces::default();
    let mut s = sample(
        SurfaceForceId::Pilot(1),
        SurfaceAllegiance::Player(PlayerId::PLAYER_1),
    );
    forces.update(std::slice::from_ref(&s), &[frame()], Duration::ZERO);
    let local = (s.position - frame().position).rotate_radians(-frame().angle);
    let moved = motion::SurfaceFrame {
        position: Vec2::new(-500.0, 20.0),
        angle: 2.0,
        ..frame()
    };
    s.position = moved.position + local.rotate_radians(moved.angle);
    s.support = None;
    for step in 1..=10 {
        forces.update(
            std::slice::from_ref(&s),
            &[moved],
            Duration::from_millis(100),
        );
        assert_eq!(forces.observations().len(), usize::from(step < 10));
    }
    for _ in 0..20 {
        forces.update(
            std::slice::from_ref(&s),
            &[moved],
            Duration::from_millis(100),
        );
    }
    assert!(
        forces.observations().is_empty(),
        "no airborne reacquisition"
    );
    s.support = Some(0);
    forces.update(&[s], &[moved], Duration::ZERO);
    assert_eq!(forces.observations()[0].contact_gap_seconds, 0.0);
}

#[test]
fn departure_reassignment_and_retirement_clear_membership_without_grace() {
    for cause in 0..5 {
        let mut f = SurfaceForces::default();
        let mut s = sample(
            SurfaceForceId::Ground(PhysicsId::new(20)),
            SurfaceAllegiance::Government(99),
        );
        f.update(std::slice::from_ref(&s), &[frame()], Duration::ZERO);
        s.support = None;
        match cause {
            0 => s.position.x += SURFACE_DEPARTURE_DISTANCE + 0.01,
            1 => s.retain_contact_gap = false,
            2 => s.allegiance = SurfaceAllegiance::Player(PlayerId::PLAYER_1),
            3 => s.id = SurfaceForceId::Ground(PhysicsId::new(21)),
            _ => {}
        }
        f.update(
            if cause == 4 {
                &[]
            } else {
                std::slice::from_ref(&s)
            },
            &[frame()],
            Duration::from_millis(1),
        );
        assert!(f.observations().is_empty(), "cause {cause}");
    }
}

#[test]
fn opposition_is_claimant_relative_and_independent_of_registration_order() {
    let mut samples = vec![
        sample(
            SurfaceForceId::Pilot(1),
            SurfaceAllegiance::Player(PlayerId::PLAYER_1),
        ),
        sample(
            SurfaceForceId::Ship(1),
            SurfaceAllegiance::Player(PlayerId::PLAYER_2),
        ),
        sample(
            SurfaceForceId::Ground(PhysicsId::new(3)),
            SurfaceAllegiance::Government(7),
        ),
    ];
    let mut a = SurfaceForces::default();
    a.update(&samples, &[frame()], Duration::ZERO);
    samples.reverse();
    let mut b = SurfaceForces::default();
    b.update(&samples, &[frame()], Duration::ZERO);
    assert_eq!(a.observations(), b.observations());
    assert_eq!(
        a.opposition(0, PlayerId::PLAYER_1),
        SurfaceOpposition {
            ships: 1,
            ground_units: 1,
            pilots: 0
        }
    );
    assert_eq!(
        a.opposition(0, PlayerId::PLAYER_2),
        SurfaceOpposition {
            ships: 0,
            ground_units: 1,
            pilots: 1
        }
    );
    assert!(a.opposition(1, PlayerId::PLAYER_1).is_empty());
}
