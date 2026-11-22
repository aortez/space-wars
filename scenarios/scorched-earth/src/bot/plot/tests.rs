use super::*;
use crate::bot_lab::{AimCase, stationary_case};

fn step(state: &mut ScorchedState) {
    state.advance(0.0);
}

/// Steps until the first shell from `player` lands, and returns its record.
fn first_shot(state: &mut ScorchedState, player: usize) -> ShotObservation {
    for _ in 0..1200 {
        step(state);
        if let Some(shot) = state
            .shot_observations()
            .iter()
            .find(|shot| shot.launch.owner == player)
        {
            return *shot;
        }
    }
    panic!("no shell landed, mode {}", state.bots[player].mode);
}

/// A wall halfway between the stationary tanks, too tall for any arc to
/// clear, with one opening from `low` to `high`.
fn wall_with_opening(state: &mut ScorchedState, low: f32, high: f32) {
    let id = PhysicsId::new(1_000_000);
    let part = |index, bottom: f32, top: f32| {
        let mut part = ColliderSpec::cuboid(
            ColliderId::new(id, ColliderRole::PRIMARY, index),
            0.5,
            (top - bottom) * 0.5,
        );
        part.local_position = Vec2::new(0.0, (bottom + top) * 0.5);
        part
    };
    assert!(state.physics.insert_body(
        BodyId::new(id, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[part(0, -5.0, low), part(1, high, 120.0)],
    ));
    // Rays meet a body once a step has indexed it.
    let next_shot = state.tanks[0].next_shot;
    state.tanks[0].next_shot = u64::MAX;
    step(state);
    state.tanks[0].next_shot = next_shot.max(state.tick);
}

#[test]
fn a_plotted_shot_goes_through_the_only_opening_in_a_wall() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = stationary_case(shape, AimCase::Flat, CpuPolicy::Plotted);
        wall_with_opening(&mut state, 1.0, 3.5);
        let shot = first_shot(&mut state, 0);
        assert_eq!(shot.termination, "opponent", "{shape:?}: {shot:?}");
        assert_eq!(shot.assessment, "useful hit");
        let plot = state.bots[0].plot.as_ref().unwrap();
        let band: Vec<_> = plot
            .arcs
            .iter()
            .filter(|arc| arc.reaches())
            .map(|arc| arc.elevation.to_degrees().round() as u16)
            .collect();
        // Only the arcs through the opening reach, and the shot took the
        // middle one.
        assert!(band.len() >= 3, "{band:?}");
        assert!(
            band.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{band:?}"
        );
        assert!(
            band.iter().all(|degrees| (20..35).contains(degrees)),
            "{band:?}"
        );
        assert_eq!(
            shot.launch.elevation.to_degrees().round() as u16,
            band[(band.len() - 1) / 2]
        );

        // Learning's first shot meets the wall.
        let mut learning = stationary_case(shape, AimCase::Flat, CpuPolicy::Calibrated);
        wall_with_opening(&mut learning, 1.0, 3.5);
        let shot = first_shot(&mut learning, 0);
        assert_eq!(shot.termination, "terrain", "{shot:?}");
        assert!((shot.end.x + 0.5).abs() < 0.01, "{shot:?}");
    }
}

#[test]
fn with_no_opening_a_plotted_tank_fires_the_burst_nearest_the_target() {
    let mut state = stationary_case(GrainShape::Hexagon, AimCase::Flat, CpuPolicy::Plotted);
    wall_with_opening(&mut state, 60.0, 60.0);
    let shot = first_shot(&mut state, 0);
    let plot = state.bots[0].plot.as_ref().unwrap();
    assert_eq!(plot.reaching, 0);
    assert_eq!(shot.termination, "terrain");
    let nearest = plot
        .arcs
        .iter()
        .filter_map(|arc| arc.miss)
        .fold(f32::INFINITY, f32::min);
    assert_eq!(plot.chosen.unwrap().miss, Some(nearest));
    assert!((shot.end.x + 0.5).abs() < 0.01, "{shot:?}");
}

#[test]
fn a_plotted_shell_lands_where_its_trace_did() {
    for case in [
        AimCase::Flat,
        AimCase::Uphill,
        AimCase::Downhill,
        AimCase::Ridge,
    ] {
        let mut state = stationary_case(GrainShape::Hexagon, case, CpuPolicy::Plotted);
        let shot = first_shot(&mut state, 0);
        // The fired plot stays until the next shot's begins.
        let plot = state.bots[0].plot.as_ref().unwrap();
        assert!(plot.fired);
        let arc = plot.chosen.unwrap();
        assert_eq!(shot.launch.elevation, arc.elevation, "{case:?}");
        assert_eq!(shot.launch.power, arc.power, "{case:?}");
        assert_eq!(shot.termination, arc.termination, "{case:?}");
        assert!(
            shot.end.distance_to(arc.end) < 1e-4,
            "{case:?}: {shot:?} {arc:?}"
        );
        assert!(arc.reaches(), "{case:?}: {arc:?}");
        assert_eq!(shot.assessment, "useful hit", "{case:?}");
        assert_eq!(state.bots[0].mode, "waiting to plot", "{case:?}");
    }
}

#[test]
fn plotting_spreads_its_rays_over_ticks_and_firing_waits_for_it() {
    let mut state = stationary_case(GrainShape::Round, AimCase::Flat, CpuPolicy::Plotted);
    let mut cast = 0;
    while state.tanks[0].shots == 0 {
        let ready = state.bots[0].plotted_arc().is_some();
        assert_eq!(state.bot_allows_fire(0), ready);
        step(&mut state);
        let plot = state.bots[0].plot.as_ref().unwrap();
        assert!(plot.casts - cast <= CASTS_PER_TICK, "{}", plot.casts - cast);
        cast = plot.casts;
        assert!(state.tick < 600, "never fired");
    }
    let plot = state.bots[0].plot.as_ref().unwrap();
    assert!(plot.fired);
    assert_eq!(
        plot.arcs.len(),
        usize::from(HIGHEST_DEGREES - LOWEST_DEGREES + 1)
    );
    assert!(plot.casts > 4 * CASTS_PER_TICK, "{}", plot.casts);
    assert!(plot.finished.unwrap() < state.tick);
    assert_eq!(state.bots[0].mode, "watching shot");
}

#[test]
fn a_plotted_tank_holds_its_ground_only_while_it_has_a_plotted_hit() {
    let mut state = driving::tests::fixture(GrainShape::Round, BankStrength::Off, |_| -5.0);
    state.config.demo = true;
    state.config.cpu_driving = true;
    state.config.cpu_policy = [CpuPolicy::Plotted; 2];
    let mut plot = Plot::new(&state, &state.physics, 0);
    let mut arc = TracedArc {
        elevation: 45.0_f32.to_radians(),
        power: 30.0,
        end: plot.target,
        termination: "opponent",
        miss: Some(0.0),
    };
    plot.finished = Some(state.tick);
    plot.chosen = Some(arc);
    state.bots[0].plot = Some(plot.clone());
    state.duel.bots[0].hit(state.duel.tick, 10.0);
    assert_eq!(state.bot_drive(0), None);
    assert_eq!(state.bots[0].relocations, 0);
    assert!(state.bot_allows_fire(0));

    arc.termination = "terrain";
    arc.miss = Some(6.0);
    plot.chosen = Some(arc);
    state.bots[0].plot = Some(plot);
    state.duel.bots[0].hit(state.duel.tick, 10.0);
    state.bot_drive(0);
    assert_eq!(state.bots[0].relocations, 1);
    assert!(state.bots[0].plot.is_none(), "moving drops the plot");
    assert!(!state.bot_allows_fire(0));
}

#[test]
fn plotted_duels_clone_replay_and_conserve_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                shape,
                demo: true,
                suspension: true,
                cpu_footing: [true; 2],
                cpu_policy: [CpuPolicy::Plotted; 2],
                ..Default::default()
            },
            42,
        );
        for _ in 0..330 {
            step(&mut state);
        }
        let mut replay = state.clone();
        for i in 0..1200 {
            step(&mut state);
            step(&mut replay);
            if i % 60 == 0 {
                assert_eq!(state.observation_hash(), replay.observation_hash());
                state.audit().unwrap();
            }
        }
        assert!(state.shots >= 4, "{shape:?}: {}", state.shots);
        assert!(
            state
                .shot_observations()
                .iter()
                .any(|shot| shot.assessment == "useful hit"),
            "{shape:?}"
        );
    }
}

#[test]
fn firing_power_meets_the_target_from_a_still_or_moving_tank() {
    let gravity = GRAVITY;
    for degrees in [15.0_f32, 30.0, 45.0, 60.0, 75.0] {
        let elevation = degrees.to_radians();
        let direction = Vec2::new(elevation.cos(), elevation.sin());
        let power = firing_power(Vec2::ZERO, direction, Vec2::ZERO, Vec2::new(30.0, 0.0)).unwrap();
        let level = (30.0 * gravity / (2.0 * elevation).sin()).sqrt();
        assert!((power - level).abs() < 1e-3, "{degrees}: {power} {level}");
        for (velocity, target) in [
            (Vec2::new(1.5, 0.0), Vec2::new(25.0, -4.0)),
            (Vec2::new(-0.5, 0.3), Vec2::new(18.0, 3.0)),
        ] {
            let power = firing_power(Vec2::ZERO, direction, velocity, target).unwrap();
            let launch = direction * power + velocity;
            let t = target.x / launch.x;
            let height = launch.y * t - 0.5 * gravity * t * t;
            assert!((height - target.y).abs() < 1e-3, "{degrees}: {height}");
        }
    }
    // A barrel pointing below the target, or away from it, has no power.
    let flat = Vec2::new(10.0_f32.to_radians().cos(), 10.0_f32.to_radians().sin());
    assert_eq!(
        firing_power(Vec2::ZERO, flat, Vec2::ZERO, Vec2::new(10.0, 20.0)),
        None
    );
    let away = Vec2::new(-flat.x, flat.y);
    assert_eq!(
        firing_power(Vec2::ZERO, away, Vec2::ZERO, Vec2::new(10.0, 0.0)),
        None
    );
}

#[test]
fn the_choice_is_the_middle_of_the_widest_band_that_reaches() {
    let arc = |degrees: u16, miss: Option<f32>| TracedArc {
        elevation: f32::from(degrees).to_radians(),
        power: 20.0,
        end: Vec2::ZERO,
        termination: if miss.is_some() { "terrain" } else { "self" },
        miss,
    };
    let arcs: Vec<_> = (10..=30)
        .map(|degrees| match degrees {
            12..=14 => arc(degrees, Some(0.0)),
            20..=24 => arc(degrees, Some(1.0)),
            27 => arc(degrees, None),
            _ => arc(degrees, Some(5.0)),
        })
        .collect();
    let chosen = choose(&arcs).unwrap();
    assert_eq!(chosen.elevation, 22.0_f32.to_radians());

    // With nothing in reach, the nearest burst wins, and a shell that hits its
    // own tank is never chosen.
    let arcs: Vec<_> = (10..=20)
        .map(|degrees| match degrees {
            15 => arc(degrees, Some(3.0)),
            16 => arc(degrees, None),
            _ => arc(degrees, Some(8.0)),
        })
        .collect();
    assert_eq!(choose(&arcs).unwrap().elevation, 15.0_f32.to_radians());
    assert!(choose(&[arc(40, None)]).is_none());
}
