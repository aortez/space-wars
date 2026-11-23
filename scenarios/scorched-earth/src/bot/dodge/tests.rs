use super::*;

/// Two plotting tanks settled on `floor`, 40 units apart, that hold where they
/// stand. The right one dodges when `dodging`. The left one fires first, half
/// a second from now, and the right one five seconds after it.
fn duel(dodging: bool, floor: impl Fn(f32) -> f32) -> ScorchedState {
    let mut state = driving::tests::fixture(GrainShape::Hexagon, BankStrength::Off, floor);
    state.config.demo = true;
    state.config.cpu_driving = true;
    state.config.cpu_policy = [CpuPolicy::Plotted; 2];
    state.config.cpu_dodging = [false, dodging];
    state.config.cpu_range = FiringRange {
        near: 18.0,
        far: 60.0,
    };
    state.tanks[0].next_shot = state.tick + 30;
    state.tanks[1].next_shot = state.tick + 330;
    state
}

fn x(state: &ScorchedState, player: usize) -> f32 {
    state
        .physics
        .motion(state.tanks[player].body)
        .unwrap()
        .position
        .x
}

/// A burst above the right tank, owned by the left one: a hit that leaves the
/// ground as it was. Returns its tick.
fn hit_right_tank(state: &mut ScorchedState) -> u64 {
    let position = state.physics.motion(state.tanks[1].body).unwrap().position;
    state.explode_from(position + Vec2::Y * 4.5, Some(0));
    assert_eq!(state.bots[1].hit_serial, 1);
    state.tick
}

/// What the left tank's first shell launched at `tick` or later did to the
/// right tank, once it has landed.
fn next_shell_damage(state: &mut ScorchedState, tick: u64) -> f32 {
    loop {
        let landed = state
            .shot_observations()
            .iter()
            .find(|shot| shot.launch.owner == 0 && shot.launch.tick >= tick)
            .map(|shot| shot.damage[1]);
        if let Some(damage) = landed {
            return damage;
        }
        assert!(state.tick < tick + 900, "the left tank fired no shell");
        state.advance(0.0);
    }
}

#[test]
fn a_hit_tank_drives_away_from_where_the_next_shell_bursts() {
    // Without dodging, the plotting tank's next shell strikes the right tank.
    let mut state = duel(false, |_| -5.0);
    let tick = hit_right_tank(&mut state);
    let undodged = next_shell_damage(&mut state, tick);
    assert!(undodged > 20.0, "{undodged}");
    // A tank that does not dodge keeps no dodges.
    assert!(state.bots[1].dodging.is_none());

    let mut state = duel(true, |_| -5.0);
    let tick = hit_right_tank(&mut state);
    let shots = state.tanks[1].shots;
    while !state.bots[1].dodging() {
        assert!(state.tick < tick + 900, "the right tank never dodged");
        state.advance(0.0);
    }
    let dodging = state.bots[1].dodging.as_ref().unwrap();
    let dodge = dodging.active.as_ref().unwrap();
    let launch = state.incoming(1, tick).unwrap().flight.unwrap().launch;
    assert_eq!(dodge.shell, launch.id);
    assert_eq!(dodge.started, launch.tick + REACTION_TICKS);
    // The shell is aimed at the tank's middle and carries on past it, so the
    // tank drives under it, toward the shooter.
    let (origin, burst, direction) = (dodge.origin, dodge.burst, dodge.direction);
    assert!(burst.x > origin.x, "{burst:?} {origin:?}");
    assert_eq!(direction, -1.0);
    while state.bots[1].dodging() {
        assert_eq!(state.driving[1].diagnostics.mode, "dodging");
        state.advance(0.0);
    }
    assert_eq!(state.tanks[1].shots, shots, "the tank held its fire");
    let last = state.bots[1]
        .dodging
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap();
    assert_eq!((last.shell, last.reason), (launch.id, "shell landed"));
    assert!(
        origin.x - x(&state, 1) > 2.0,
        "{last:?}",
        last = last.position
    );
    let dodged = next_shell_damage(&mut state, tick);
    assert!(dodged < undodged / 3.0, "{dodged} of {undodged}");
}

#[test]
fn a_shell_that_will_burst_out_of_reach_is_left_to_fly() {
    let mut state = duel(true, |_| -5.0);
    state.config.demo = false;
    state.selected = 0;
    let tick = hit_right_tank(&mut state);
    let start = x(&state, 1);
    // A lob that comes down by its own tank.
    state.tanks[0].facing = 1.0;
    state.tanks[0].elevation = 80.0_f32.to_radians();
    state.tanks[0].speed = 12.0;
    state.tanks[0].next_shot = state.tick;
    state.fire(0);
    assert_eq!(state.shots, 1);
    for _ in 0..REACTION_TICKS + 1 {
        state.advance(0.0);
    }
    let dodging = state.bots[1].dodging.as_ref().unwrap();
    assert_eq!((dodging.dodges, dodging.passed), (0, 1));
    assert_eq!(dodging.last.as_ref().unwrap().reason, "will miss");
    assert!(dodging.after.is_none(), "the hit's dodge is spent");
    assert!((x(&state, 1) - start).abs() < 0.1);
    assert!(state.tick > tick);
}

#[test]
fn a_dodge_stops_short_of_a_drop_and_waits_there_for_the_shell() {
    // A trench across the way the right tank dodges, beyond the few units the
    // drive check sees from where the tank starts.
    let right = x(&duel(true, |_| -5.0), 1);
    let edge = right - 5.0;
    let mut state = duel(true, |x| {
        if (edge - 2.5..edge).contains(&x) {
            -8.0
        } else {
            -5.0
        }
    });
    let tick = hit_right_tank(&mut state);
    while !state.bots[1].dodging() {
        assert!(state.tick < tick + 900, "the right tank never dodged");
        state.advance(0.0);
    }
    let active = |state: &ScorchedState| {
        state.bots[1]
            .dodging
            .as_ref()
            .unwrap()
            .active
            .clone()
            .unwrap()
    };
    let shell = active(&state).shell;
    let mut farthest = right;
    while state.bots[1].dodging() {
        assert_eq!(state.driving[1].diagnostics.mode, "dodging");
        // It never turns back under the shell.
        farthest = farthest.min(x(&state, 1));
        assert!(x(&state, 1) < farthest + 0.05);
        state.advance(0.0);
    }
    let last = state.bots[1]
        .dodging
        .as_ref()
        .unwrap()
        .last
        .clone()
        .unwrap();
    assert_eq!((last.shell, last.reason), (shell, "drop ahead"));
    // It stopped with its leading wheel short of the trench, and waited there
    // until the shell landed.
    assert!(right - x(&state, 1) > SHORTEST, "{}", right - x(&state, 1));
    assert!(x(&state, 1) - HALF_LENGTH > edge, "{}", x(&state, 1) - edge);
    let landed = state
        .shot_observations()
        .iter()
        .find(|shot| shot.launch.id == shell)
        .expect("the shell has landed")
        .tick;
    assert!(last.ended >= landed, "{} before {landed}", last.ended);
}

#[test]
fn a_tank_with_no_ground_to_drive_on_stays_and_keeps_firing() {
    // The ground falls away just in front of the right tank, where it would
    // drive to get out from under the shell.
    let right = x(&duel(true, |_| -5.0), 1);
    let mut state = duel(true, |x| {
        if (right - 4.5..right - 2.2).contains(&x) {
            -12.0
        } else {
            -5.0
        }
    });
    let tick = hit_right_tank(&mut state);
    while state.bots[1]
        .dodging
        .as_ref()
        .is_none_or(|dodging| dodging.refused == 0)
    {
        assert!(!state.bots[1].dodging(), "the tank drove off");
        assert!(state.tick < tick + 900, "the left tank fired no shell");
        state.advance(0.0);
    }
    let last = state.bots[1]
        .dodging
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap();
    assert_eq!(last.reason, "no room");
    // Its own next shot is not held up.
    let shots = state.tanks[1].shots;
    while state.tanks[1].shots == shots {
        assert!(state.tick < tick + 900, "the right tank stopped firing");
        state.advance(0.0);
    }
}
