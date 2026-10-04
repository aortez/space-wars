use super::*;

fn config(fixture: Fixture, experiment: Experiment, model: Model) -> LabConfig {
    LabConfig {
        fixture,
        experiment,
        model,
        seed: 42,
    }
}

fn advance(lab: &mut SoilLab, tick: u64) -> Metrics {
    while lab.tick < tick {
        lab.step().unwrap();
        lab.audit().unwrap();
    }
    lab.audit().unwrap()
}

#[test]
fn poured_material_stays_conserved_and_friction_changes_the_pile() {
    let mut soil = SoilLab::new(config(Fixture::Flat, Experiment::Pour, Model::Mpm)).unwrap();
    let initial = soil.audit().unwrap();
    assert_eq!(initial.particles, 12);
    assert!(initial.reservoir_mass.iter().sum::<f64>() > 4.0);
    let soil_metrics = advance(&mut soil, 360);
    assert_eq!(soil_metrics.particles, 288);
    assert_eq!(soil_metrics.reservoir_mass, [0.0; 2]);
    assert!(soil_metrics.rms_speed < 0.05);
    assert!(soil_metrics.height > 1.0);
    let mut control =
        SoilLab::new(config(Fixture::Flat, Experiment::Pour, Model::Frictionless)).unwrap();
    let control_metrics = advance(&mut control, 360);
    assert_eq!(soil_metrics.mass, control_metrics.mass);
    assert!(control_metrics.height < soil_metrics.height * 0.5);
    assert!(control_metrics.width > soil_metrics.width * 1.5);
}

#[test]
fn removing_support_slumps_the_bank_without_removing_soil() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        let mut lab = SoilLab::new(config(fixture, Experiment::Bank, Model::Mpm)).unwrap();
        let before = advance(&mut lab, SUPPORT_TICK);
        assert!(!lab.ground.support_removed);
        assert_eq!(before.below_platform_mass, 0.0);
        let after = advance(&mut lab, 360);
        assert!(lab.ground.support_removed);
        assert_eq!(before.mass, after.mass);
        assert_eq!(before.particles, after.particles);
        assert!(after.below_platform_mass > 3.0, "{fixture:?}: {after:?}");
        assert!(after.rms_speed < 0.12, "{fixture:?}: {after:?}");
    }
}

#[test]
fn second_blast_hits_changed_material_and_clone_replays_every_step() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        let mut lab = SoilLab::new(config(fixture, Experiment::Blasts, Model::Mpm)).unwrap();
        let before = advance(&mut lab, BLAST_TICKS[0]);
        lab.step().unwrap();
        let first_hits = lab.last_blast_hits;
        assert!(first_hits > 100);
        let first_crater = advance(&mut lab, BLAST_TICKS[1]);
        assert!(first_crater.center_height < before.center_height - 0.1);
        let mut cloned = lab.clone();
        lab.step().unwrap();
        cloned.step().unwrap();
        assert!(lab.last_blast_hits > 0);
        assert_ne!(lab.last_blast_hits, first_hits);
        for _ in 0..60 {
            lab.step().unwrap();
            cloned.step().unwrap();
            assert_eq!(lab.state_hash(), cloned.state_hash());
            assert_eq!(before.mass, lab.audit().unwrap().mass);
        }
    }
}

#[test]
fn curved_seed_map_preserves_area_and_inherits_surface_motion() {
    let fixture = Fixture::MovingPlanet;
    let local = Vec2::new(2.5, 3.0);
    let time = 1.7;
    let p = fixture.to_world(local, time);
    assert!((fixture.to_local(p, time) - local).length() < 1.0e-5);
    let epsilon = 0.001;
    let dx = (fixture.to_world(local + Vec2::X * epsilon, time) - p) / epsilon;
    let dy = (fixture.to_world(local + Vec2::Y * epsilon, time) - p) / epsilon;
    assert!((dx.x * dy.y - dx.y * dy.x - 1.0).abs() < 0.002);
    let lab = SoilLab::new(config(fixture, Experiment::Bank, Model::Mpm)).unwrap();
    assert!(
        lab.points()
            .iter()
            .all(|p| (p.velocity - fixture.surface_velocity(p.position, 0.0)).length() < 1.0e-6)
    );
}

#[test]
fn comparison_backends_start_with_identical_material_and_momentum() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        for experiment in [Experiment::Pour, Experiment::Bank, Experiment::Blasts] {
            let mpm = SoilLab::new(config(fixture, experiment, Model::Mpm)).unwrap();
            let grains = SoilLab::new(config(fixture, experiment, Model::Grains)).unwrap();
            let left = mpm.points();
            let right = grains.points();
            assert_eq!(left.len(), right.len());
            for (left, right) in left.iter().zip(right) {
                assert_eq!(left.position, right.position);
                assert!((left.velocity - right.velocity).length() < 1.0e-6);
                assert_eq!(left.mass, right.mass);
                assert_eq!(left.material, right.material);
            }
        }
    }
}
