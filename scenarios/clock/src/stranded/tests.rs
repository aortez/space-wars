use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState,
    arena::Owner,
    events::{
        ActiveEvent, EventPhase,
        tanks::{BEAM_UP_TICKS, TanksEvent, survivor},
    },
};
use engine_common::{
    ClockArenaOwner, ClockEventProfile, ClockEvents, ClockTanksEnding, ClockTimeFormat, Scenario,
};
use std::time::Duration;

const DIGIT_SLIDE_ONLY: ClockEvents = ClockEvents {
    falling: false,
    color_cycle: false,
    meltdown: false,
    duck: false,
    marquee: false,
    digit_slide: true,
    rain: false,
    crow: false,
    explosion: false,
    aquarium: false,
    cell_swap: false,
    grass: false,
    slot_machine: false,
    spider: false,
    coins: false,
    tanks: false,
};

fn ready_with(aspect: f32, seed: u64, time: [u8; 3], sliding: bool) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: if sliding {
                ClockEventProfile::Calm
            } else {
                ClockEventProfile::Off
            },
            events: DIGIT_SLIDE_ONLY,
            time_format: ClockTimeFormat::TwentyFourHour,
            ..Default::default()
        },
        seed,
    );
    read(&mut state, time);
    state
}

/// Shows `state` the time, between ticks.
fn read(state: &mut ClockState, [hour, minute, second]: [u8; 3]) {
    act(
        state,
        ClockAction::set_reading(ClockReading::new(hour, minute, second).unwrap()),
    );
}

/// Gives `state` an action between ticks, as a control does.
fn act(state: &mut ClockState, action: engine_common::Action) {
    ClockScenario::step(state, &[action], Duration::ZERO);
}

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

fn event(state: &mut ClockState) -> &mut TanksEvent {
    match state.active_event.as_mut().unwrap() {
        ActiveEvent::Tanks(event) => event,
        _ => panic!("Tanks"),
    }
}

/// Starts the fight at `state`'s seed, with every loser's spaceling thrown
/// clear and drawing no beam.
fn fight(state: &mut ClockState) {
    state.preview_event(ClockEventKind::Tanks);
    event(state).survivors.rolls = [Some(survivor::Ejection::Thrown); 2];
    event(state).survivors.ending_rolls = [0.99; 2];
}

/// A fight on an `aspect` screen showing `time`, stepped on until its hills
/// have left a spaceling behind and gone.
fn stranded_at(aspect: f32, seed: u64, time: [u8; 3], sliding: bool) -> ClockState {
    let mut state = ready_with(aspect, seed, time, sliding);
    fight(&mut state);
    while state.event_kind() == Some(ClockEventKind::Tanks) {
        tick(&mut state);
    }
    assert!(state.stranded.spaceling.is_some(), "a stranded spaceling");
    state
}

fn stranded(aspect: f32, seed: u64) -> ClockState {
    stranded_at(aspect, seed, [12, 34, 5], false)
}

fn spaceling(state: &ClockState) -> &Stranded {
    state
        .stranded
        .spaceling
        .as_ref()
        .expect("a stranded spaceling")
}

fn spaceling_mut(state: &mut ClockState) -> &mut Stranded {
    state
        .stranded
        .spaceling
        .as_mut()
        .expect("a stranded spaceling")
}

fn blocks_of(state: &ClockState) -> Vec<Block> {
    blocks(Layout::new(state.aspect_ratio()), &state.published)
}

impl Stranded {
    /// Ages the spaceling by `ticks` where it is, with everything it has
    /// timed by its age moved on with it.
    fn skip(&mut self, ticks: u64) {
        self.age += ticks;
        self.since += ticks;
        self.until += ticks;
        self.look += ticks;
        self.headway.0 += ticks;
        self.wary += ticks;
        self.pressed = self.pressed.map(|pressed| pressed + ticks);
        for puff in &mut self.puffs {
            puff.tick += ticks;
        }
    }
}

/// Steps on until the spaceling is in `phase`, at most `limit` ticks.
fn until_phase(state: &mut ClockState, phase: Phase, limit: u64) {
    for _ in 0..limit {
        if spaceling(state).phase == phase {
            return;
        }
        tick(state);
    }
    panic!("{:?} in place of {phase:?}", spaceling(state).phase);
}

/// Sends the spaceling to `place` on its jetpack, and steps on until it has
/// landed there.
fn send(state: &mut ClockState, place: Place) {
    let world = state.arena.held_mut(Owner::Stranded).unwrap();
    state.stranded.spaceling.as_mut().unwrap().fly(world, place);
    for _ in 0..FLIGHT_TICKS[1] + 2 {
        if spaceling(state).phase != Phase::Flying {
            break;
        }
        tick(state);
    }
    assert_eq!(spaceling(state).place, place);
    assert_ne!(spaceling(state).phase, Phase::Flying);
}

/// Sends the spaceling down to the floor under where it is.
fn send_to_floor(state: &mut ClockState) -> Vec2 {
    let layout = Layout::new(state.aspect_ratio());
    let spot = Vec2::new(spaceling(state).feet.x, layout.floor_y);
    send(state, Place::Floor(spot));
    spot
}

#[test]
fn a_face_gives_a_spaceling_its_resting_blocks_to_stand_on() {
    let state = ready_with(5.0 / 3.0, 1, [12, 34, 5], false);
    let layout = Layout::new(state.aspect_ratio());
    let all = blocks_of(&state);
    // Every column with a lit cell has one topmost block, with nothing lit
    // over it, and the blocks under a gap in their column are covered.
    for slot in 0..DIGIT_SLOT_COUNT as u8 {
        for column in 0..6u8 {
            let lit: Vec<u8> = state
                .published
                .iter()
                .filter(|surface| surface.slot == slot && surface.cell.x as u8 == column)
                .map(|surface| surface.cell.y as u8)
                .collect();
            let standing: Vec<&Block> = all
                .iter()
                .filter(|block| block.key[0] == slot && block.key[1] == column)
                .collect();
            let tops: Vec<_> = standing.iter().filter(|block| !block.covered).collect();
            assert_eq!(tops.len(), usize::from(!lit.is_empty()), "{slot} {column}");
            for block in &standing {
                let row = block.key[2];
                assert!(lit.contains(&row) && !lit.contains(&(row + 1)));
                let over = lit.iter().copied().filter(|other| *other > row + 1).min();
                assert_eq!(block.covered, over.is_some());
                assert_eq!(block.headroom, over.map_or(0, |over| over - row - 1));
                let cell = GridCell {
                    x: column as i8,
                    y: row as i8,
                };
                let top = layout.slot_cell_center(usize::from(slot), cell)
                    + Vec2::new(0.0, CELL_HALF * layout.pitch);
                assert_eq!(block.feet, top);
            }
        }
    }
    // Every digit has a block to shelter on: the top of a lower stroke,
    // under the upper one, and the bars under a top bar.
    for slot in 0..DIGIT_SLOT_COUNT as u8 {
        assert!(
            all.iter()
                .any(|block| block.covered && block.key[0] == slot)
        );
    }
    // A slot an event is moving gives none.
    let mut moved = state.published.clone();
    let first = moved.iter_mut().find(|surface| surface.slot == 3).unwrap();
    first.origin.y += 1.0;
    let rest = blocks(layout, &moved);
    assert!(rest.iter().all(|block| block.key[0] != 3));
    assert_eq!(
        rest.len(),
        all.iter().filter(|block| block.key[0] != 3).count()
    );
}

#[test]
fn the_hills_leave_a_spaceling_that_drew_no_beam_behind_on_the_clock() {
    use survivor::Phase as Survivor;
    let mut state = ready_with(5.0 / 3.0, 42, [12, 34, 5], false);
    let layout = Layout::new(state.aspect_ratio());
    fight(&mut state);
    while event(&mut state).result.is_none() {
        tick(&mut state);
    }
    let loser = event(&mut state).survivors.out[0].slot;
    assert_eq!(
        event(&mut state).survivors.out[0].drawn,
        survivor::Ending::Stranded
    );
    assert!(state.stranded_state().is_none());
    // It gets up, looks around and waves, as one the rescue beam comes for
    // does. Then the winner goes up, and no beam comes for it.
    while state.event_phase() != Some(EventPhase::BeamUp) {
        tick(&mut state);
    }
    let left = &event(&mut state).survivors.out[0];
    assert_eq!((left.phase, left.fallback), (Survivor::Stranded, None));
    let body = left.spaceling.as_ref().unwrap().body();
    let stood = state.arena.world().unwrap().motion(body).unwrap();
    for _ in 0..BEAM_UP_TICKS {
        assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
        assert!(state.stranded.spaceling.is_none());
        tick(&mut state);
    }
    // As the hills begin to withdraw, it is the fight's no longer: it is a
    // visitor with its own claim on the arena and its own body, where the
    // fight's spaceling stood.
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    assert_eq!(state.arena.owners().last(), Some(&Owner::Stranded));
    assert!(event(&mut state).survivors.out[0].spaceling.is_none());
    let world = state.arena.world().unwrap();
    assert!(world.motion(body).is_none());
    let visitor = spaceling(&state);
    assert_eq!((visitor.slot, visitor.phase), (loser, Phase::Flying));
    assert!(visitor.motion().position.distance_to(stood.position) < 0.5 * visitor.scale);
    let tanks = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(tanks.ending, ClockTanksEnding::Stranded);
    assert_eq!(tanks.phase, Survivor::Stranded);
    let published = state.stranded_state().unwrap();
    let block = published.spaceling.unwrap();
    assert_eq!(
        (block.fight_id, block.slot, block.phase, block.leaving),
        (state.event_id(), loser as u8, Phase::Flying, None)
    );
    assert_eq!(published.accounting.stranded, 1);
    // Its jetpack takes it up off the hills onto the nearest digit block it
    // can stand on top of, where it plants its flag.
    let from = visitor.feet;
    let nearest = blocks_of(&state)
        .into_iter()
        .filter(|block| !block.covered)
        .min_by(|a, b| {
            a.feet
                .distance_to(from)
                .total_cmp(&b.feet.distance_to(from))
        })
        .unwrap();
    assert_eq!(visitor.place, Place::Block(nearest));
    assert!(nearest.feet.y > from.y);
    until_phase(&mut state, Phase::Planting, FLIGHT_TICKS[1] + 1);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Tanks));
    let visitor = spaceling(&state);
    assert_eq!(visitor.feet, nearest.feet);
    assert!(!visitor.puffs.is_empty(), "its jetpack's smoke");
    assert!(state.stranded.flag.is_none());
    until_phase(&mut state, Phase::Standing, PLANT_TICKS + 1);
    let flag = state.stranded.flag.unwrap();
    assert_eq!((flag.slot, flag.key), (loser, nearest.key));
    assert_eq!(
        state.stranded_state().unwrap().flag.unwrap().block,
        nearest.key
    );
    // The hills go, and it stays, with its perch, on the floor the arena
    // keeps under it.
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert_eq!(state.arena.owners(), [Owner::Stranded]);
    assert!(state.arena.keeps_floor());
    let arena = state.arena_state();
    let claim = arena
        .claims
        .iter()
        .find(|claim| claim.owner == ClockArenaOwner::Stranded)
        .unwrap();
    assert_eq!(
        (claim.population.bodies, claim.population.colliders),
        (1 + PERCHES, 1 + PERCHES)
    );
    let visitor = spaceling(&state);
    assert_eq!(visitor.phase, Phase::Standing);
    assert!((visitor.feet.y - nearest.feet.y).abs() < 0.05 * visitor.scale);
    assert!(visitor.feet.x - layout.bounds_min.x > 0.0);
}

#[test]
fn only_one_of_a_fights_two_spacelings_is_left_behind() {
    // Neither tank survives this fight, and both spacelings roll no beam.
    let mut state = ready_with(4.0 / 3.0, 69, [12, 34, 5], false);
    fight(&mut state);
    while state.event_kind().is_some() {
        tick(&mut state);
        if let Some(ActiveEvent::Tanks(event)) = &state.active_event
            && event.survivors.out.len() == 2
        {
            let drawn = [0, 1].map(|index| event.survivors.out[index].drawn);
            assert_eq!(
                drawn,
                [survivor::Ending::Stranded, survivor::Ending::RescueBeam]
            );
        }
    }
    assert!(state.stranded.spaceling.is_some());
    assert_eq!(state.stranded.accounting.stranded, 1);
    assert_eq!(state.arena.owners(), [Owner::Stranded]);
}

#[test]
fn it_stands_on_digits_walks_the_floor_and_flies_between_them() {
    let mut state = stranded(5.0 / 3.0, 42);
    let layout = Layout::new(state.aspect_ratio());
    let scale = spaceling(&state).scale;
    let [mut walked, mut floor_stays, mut block_stays] = [0u32; 3];
    let mut last = spaceling(&state).phase;
    for _ in 0..6 * 60 * 60 {
        tick(&mut state);
        let visitor = spaceling(&state);
        let tops = blocks_of(&state);
        match (visitor.phase, visitor.place) {
            // On a digit it stands on the topmost block of a column.
            (Phase::Standing | Phase::Planting, Place::Block(block)) => {
                assert!(!block.covered);
                assert!(tops.contains(&block));
                assert!(visitor.feet.distance_to(block.feet) < 0.3 * scale);
                block_stays += u32::from(last == Phase::Flying);
            }
            // On the floor it stands and walks, inside the frame.
            (Phase::Standing | Phase::Walking, Place::Floor(_)) => {
                assert!((visitor.feet.y - layout.floor_y).abs() < 0.3 * scale);
                let [left, right] = visitor.floor_span();
                assert!((left - scale..=right + scale).contains(&visitor.feet.x));
                walked += u32::from(visitor.phase == Phase::Walking && last != Phase::Walking);
                floor_stays += u32::from(last == Phase::Flying);
            }
            (Phase::Flying, _) => assert!(visitor.flight.is_some()),
            other => panic!("{other:?}"),
        }
        assert!(visitor.angle.abs() < 0.5, "upright");
        last = visitor.phase;
    }
    assert!(block_stays >= 10 && floor_stays >= 3 && walked >= 3);
    let published = state.stranded_state().unwrap().spaceling.unwrap();
    assert_eq!(published.knockdowns, 0);
    assert!(published.flights >= block_stays + floor_stays);
    assert_eq!(state.stranded.accounting.stranded, 1);
}

#[test]
fn a_stay_replays_alike_from_its_fights_seed() {
    let run = |seed: u64| {
        let mut state = stranded(5.0 / 3.0, seed);
        let mut seen = Vec::new();
        for _ in 0..90 * 60 {
            tick(&mut state);
            seen.push(state.stranded_state().unwrap());
        }
        seen
    };
    assert_eq!(run(42), run(42));
}

#[test]
fn its_beam_comes_at_the_first_change_of_the_time_once_it_has_stayed_as_long_as_it_may() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    let flag = state.stranded.flag.expect("its flag");
    let age = spaceling(&state).age;
    spaceling_mut(&mut state).skip(STAY_TICKS - age - 30);
    // The time changes before its stay is up: it stays, wherever it flies.
    read(&mut state, [12, 34, 59]);
    for _ in 0..90 {
        tick(&mut state);
        assert!(spaceling(&state).leaving.is_none());
    }
    assert!(spaceling(&state).age >= STAY_TICKS);
    // The first change after it is, with the clock paused or not, brings
    // its beam.
    read(&mut state, [13, 0, 0]);
    let visitor = spaceling(&state);
    assert_eq!(
        (visitor.phase, visitor.leaving.map(|(why, _)| why)),
        (Phase::BeamedUp, Some(Departure::TimeUp))
    );
    assert_eq!(
        state.stranded_state().unwrap().spaceling.unwrap().leaving,
        Some(Departure::TimeUp)
    );
    // The beam comes down on it, then lifts it up and out, fading.
    let from = visitor.feet;
    for ticks in 1..=BEAM_TICKS {
        let visitor = spaceling(&state);
        assert_eq!(visitor.beam().unwrap().0, from);
        tick(&mut state);
        let Some(visitor) = &state.stranded.spaceling else {
            assert_eq!(ticks, BEAM_TICKS);
            break;
        };
        let rising = ticks > BEAM_DESCENT_TICKS;
        assert_eq!(visitor.feet.y > from.y, rising, "{ticks}");
        assert_eq!(visitor.fade() > 0.0, rising);
    }
    assert!(state.stranded.spaceling.is_none());
    assert!(state.arena.idle());
    let published = state.stranded_state().unwrap();
    assert!(published.spaceling.is_none());
    assert_eq!(
        (published.accounting.stranded, published.accounting.time_up),
        (1, 1)
    );
    // Its flag stands on after it, until its digit changes.
    if flag.key[0] == 0 {
        assert_eq!(state.stranded.flag, Some(flag));
        read(&mut state, [23, 0, 0]);
    }
    assert!(state.stranded.flag.is_none());
}

#[test]
fn with_the_time_unknown_its_beam_comes_a_little_after_its_stay_is_up() {
    let mut state = stranded(5.0 / 3.0, 42);
    act(&mut state, ClockAction::set_time_unknown());
    until_phase(&mut state, Phase::Standing, 30 * 60);
    let age = spaceling(&state).age;
    spaceling_mut(&mut state).skip(STAY_TICKS + GRACE_TICKS - age - 3);
    for _ in 0..2 {
        tick(&mut state);
        assert!(spaceling(&state).leaving.is_none());
    }
    tick(&mut state);
    assert_eq!(
        spaceling(&state).leaving.map(|(why, _)| why),
        Some(Departure::TimeUp)
    );
}

#[test]
fn an_event_that_leaves_it_no_digit_to_stand_on_brings_its_beam_first() {
    for kind in [
        ClockEventKind::Meltdown,
        ClockEventKind::Explosion,
        ClockEventKind::Coins,
        ClockEventKind::Marquee,
        ClockEventKind::Aquarium,
        ClockEventKind::Tanks,
    ] {
        let mut state = stranded(5.0 / 3.0, 42);
        until_phase(&mut state, Phase::Standing, 10 * 60);
        // On the floor, where the next fight's hills will rise.
        send_to_floor(&mut state);
        act(&mut state, ClockAction::preview_event(kind));
        assert_eq!(state.event_kind(), Some(kind), "{kind:?}");
        let visitor = spaceling(&state);
        assert_eq!(
            (visitor.phase, visitor.leaving.map(|(why, _)| why)),
            (Phase::BeamedUp, Some(Departure::Event)),
            "{kind:?}"
        );
        for _ in 0..BEAM_TICKS {
            assert!(state.stranded.spaceling.is_some());
            tick(&mut state);
        }
        assert!(state.stranded.spaceling.is_none(), "{kind:?}");
        assert_eq!(state.event_kind(), Some(kind), "{kind:?}");
        assert!(!state.arena.holds(Owner::Stranded));
        assert_eq!(state.stranded.accounting.event, 1);
    }
}

#[test]
fn with_no_room_beside_it_for_fallings_bars_its_beam_comes_first() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    // Three fixture owners, one more than a run can start with, leave the
    // arena no room for the bars beside it.
    let layout = Layout::new(state.aspect_ratio());
    for index in 0..3 {
        state.arena.claim_resident(Owner::Fixture(index), layout);
    }
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Falling),
    );
    // The bars fall in a world of their own, and its beam comes for it.
    assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
    assert!(!state.arena.holds(Owner::Event(ClockEventKind::Falling)));
    let visitor = spaceling(&state);
    assert_eq!(
        (visitor.phase, visitor.leaving.map(|(why, _)| why)),
        (Phase::BeamedUp, Some(Departure::Event))
    );
}

#[test]
fn it_stays_through_the_events_that_leave_the_digits_standing() {
    for kind in [
        ClockEventKind::ColorCycle,
        ClockEventKind::Rain,
        ClockEventKind::Crow,
        ClockEventKind::Duck,
        ClockEventKind::DigitSlide,
        ClockEventKind::CellSwap,
        ClockEventKind::SlotMachine,
        ClockEventKind::Grass,
        ClockEventKind::Spider,
    ] {
        let mut state = stranded(5.0 / 3.0, 42);
        until_phase(&mut state, Phase::Standing, 10 * 60);
        act(&mut state, ClockAction::preview_event(kind));
        let started = state.event_kind() == Some(kind)
            || state.crow_state().is_some()
            || state.spider_state().is_some()
            || state.duck_state().is_some()
            || kind == ClockEventKind::Grass;
        assert!(started, "{kind:?}");
        for _ in 0..40 * 60 {
            tick(&mut state);
            let visitor = spaceling(&state);
            assert!(visitor.leaving.is_none(), "{kind:?}");
            // While the digits turn, swap or slide, it is not on them.
            if matches!(
                (state.event_kind(), visitor.phase),
                (
                    Some(ClockEventKind::SlotMachine | ClockEventKind::CellSwap),
                    Phase::Standing | Phase::Walking
                )
            ) && let Place::Block(block) = visitor.place
            {
                assert!(blocks_of(&state).contains(&block), "{kind:?}");
            }
        }
        assert_eq!(state.stranded.accounting.stranded, 1);
        assert!(state.arena.holds(Owner::Stranded));
    }
}

/// A spaceling the fight at `seed` left, down on the floor at `x` as Falling
/// begins.
fn falling_on_the_floor(seed: u64, x: f32) -> ClockState {
    let mut state = stranded(5.0 / 3.0, seed);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    let floor = Layout::new(state.aspect_ratio()).floor_y;
    send(&mut state, Place::Floor(Vec2::new(x, floor)));
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Falling),
    );
    state
}

/// Steps on while Falling lasts, at most a minute, answering whether the
/// spaceling was ever knocked down meanwhile; it never dies.
fn through_falling(state: &mut ClockState) -> bool {
    let mut knocked = false;
    for _ in 0..60 * 60 {
        if state.event_kind() != Some(ClockEventKind::Falling) {
            return knocked;
        }
        tick(state);
        let visitor = spaceling(state);
        assert!(visitor.leaving.is_none());
        assert_ne!(visitor.phase, Phase::Dead);
        knocked |= visitor.phase == Phase::GettingUp;
    }
    panic!("Falling still on");
}

#[test]
fn fallings_bars_come_down_in_its_world_as_it_takes_off_from_its_digit() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    assert!(matches!(spaceling(&state).place, Place::Block(_)));
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Falling),
    );
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Falling)));
    // It takes off as its block drops away.
    assert_eq!(spaceling(&state).phase, Phase::Flying);
    tick(&mut state);
    // Every bar coming down is a body in the world it flies in.
    let world = state.arena.held(Owner::Stranded).unwrap();
    let rigid = state
        .published
        .iter()
        .filter(|surface| surface.source == SurfaceSource::Rigid)
        .count();
    assert!(rigid > 0);
    assert_eq!(bars(world, &state.published).len(), rigid);
    assert!(!through_falling(&mut state));
    // Once they are gone it lands again, on a digit or the floor, and stays.
    until_phase(&mut state, Phase::Standing, 10 * 60);
    let visitor = spaceling(&state);
    assert!(!matches!(visitor.place, Place::Air(_)));
    assert_eq!(visitor.knockdowns, 0);
    assert!(visitor.hardest < spec(visitor.scale).balance.knockdown_velocity_change);
    assert!(visitor.leaving.is_none());
}

#[test]
fn on_the_floor_it_runs_out_from_under_the_bars_it_can_get_clear_of() {
    // Seed 3's bars come down on it, a few steps from a stretch they miss.
    let mut state = falling_on_the_floor(3, -218.4);
    let start = spaceling(&state).feet;
    // It takes a moment to see them.
    for _ in 1..DODGE_REACTION {
        tick(&mut state);
        assert_eq!(spaceling(&state).phase, Phase::Standing);
    }
    until_phase(&mut state, Phase::Dodging, 2);
    let visitor = spaceling(&state);
    assert!((visitor.place.feet().x - start.x).abs() > REACH * visitor.scale);
    assert!(!through_falling(&mut state));
    let visitor = spaceling(&state);
    assert_eq!(visitor.knockdowns, 0);
    assert!(visitor.hardest < spec(visitor.scale).balance.knockdown_velocity_change);
}

#[test]
fn a_bar_that_knocks_it_down_on_the_floor_leaves_it_to_get_up() {
    // A bar seed 2 brings down beside it catches it a glancing blow.
    let mut state = falling_on_the_floor(2, 218.4);
    assert!(through_falling(&mut state));
    let visitor = spaceling(&state);
    let knockdown = spec(visitor.scale).balance.knockdown_velocity_change;
    assert!((knockdown..LETHAL * knockdown).contains(&visitor.hardest));
    assert_eq!(visitor.knockdowns, 1);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    assert_eq!(state.stranded.accounting.died, 0);
}

#[test]
fn a_bar_that_comes_down_on_it_kills_it_and_it_turns_to_dirt_where_grass_takes_root() {
    // Seed 2 brings a bar down on it squarely.
    let mut state = falling_on_the_floor(2, 109.2);
    state.config.events.grass = true;
    let added = state
        .soil
        .as_ref()
        .map_or(0, |soil| soil.accounting().added);
    until_phase(&mut state, Phase::Dead, 10 * 60);
    let visitor = spaceling(&state);
    let knockdown = spec(visitor.scale).balance.knockdown_velocity_change;
    assert!(visitor.hardest >= LETHAL * knockdown);
    // It lies where it fell, comes to rest there well before it could have
    // lain dead as long as it may, turns to dirt, and its stay ends.
    let mut lay = None;
    for n in 0..DEAD_TICKS + CRUMBLE_TICKS + 2 {
        let Some(visitor) = state.stranded.spaceling.as_ref() else {
            break;
        };
        assert_eq!(visitor.phase, Phase::Dead);
        if visitor.crumbling.is_some() && lay.is_none() {
            assert!(n < DEAD_TICKS / 2, "it lay still after {n} ticks");
            let world = state.arena.held(Owner::Stranded).unwrap();
            let motion = world.motion(body()).unwrap();
            assert!(motion.linear_velocity.length() < REST_SPEED * visitor.scale);
            lay = Some(motion.position.x);
        }
        tick(&mut state);
    }
    assert!(state.stranded.spaceling.is_none());
    assert_eq!(state.stranded.accounting.died, 1);
    assert!(!state.arena.holds(Owner::Stranded));
    // Its remains are dirt in the columns under them, and a seed has taken
    // root in the middle one.
    let soil = state.soil.as_ref().unwrap();
    let column = soil.column_at(lay.expect("it turned to dirt"));
    let rows = crate::grass::SOIL_DEPTH_CELLS as i32;
    for x in column - REMAINS_REACH..=column + REMAINS_REACH {
        for y in 0..rows {
            let cell = soil
                .terrain()
                .cell(engine_terrain::CellCoord::new(x, y))
                .unwrap();
            assert_eq!(cell.material, crate::grass::CLOCK_DIRT, "{x} {y}");
        }
    }
    assert_eq!(
        soil.accounting().added - added,
        (2 * REMAINS_REACH as u64 + 1) * crate::grass::SOIL_DEPTH_CELLS as u64
    );
    let roots: Vec<_> = state
        .plants
        .plants
        .observe()
        .tissues
        .iter()
        .filter(|tissue| tissue.role == engine_core::plants::Role::Root)
        .map(|tissue| tissue.cell)
        .collect();
    assert_eq!(roots, [[column, rows - 1]]);
}

#[test]
fn killed_it_goes_limp_and_its_remains_come_to_rest_where_the_blow_leaves_them() {
    // Seed 13's bar kills it with one blow, which would have sent it gliding
    // along the floor with no grip but its controller's.
    let mut state = falling_on_the_floor(13, -218.4);
    until_phase(&mut state, Phase::Dead, 10 * 60);
    let world = state.arena.held(Owner::Stranded).unwrap();
    assert_eq!(
        spaceling(&state).spaceling.snapshot(world).unwrap().balance,
        SpacelingBalance::KnockedDown
    );
    // Its remains lie still on the floor within a second, so they begin to
    // turn to dirt a second later, while the bars still come down.
    for _ in 0..2 * SECOND {
        if spaceling(&state).crumbling.is_some() {
            assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
            return;
        }
        tick(&mut state);
    }
    panic!("its remains never lay still");
}

#[test]
fn its_remains_turn_to_no_dirt_where_the_floor_holds_no_soil() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    state.config.events.grass = true;
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Falling),
    );
    let added = |state: &ClockState| {
        state
            .soil
            .as_ref()
            .map_or(0, |soil| soil.accounting().added)
    };
    let before = added(&state);
    // Falling's drain is open under the middle of the floor.
    state.bury_stranded(0.0);
    assert_eq!(added(&state), before);
    assert_eq!(state.plants.grass_tissues(), 0);
}

#[test]
fn remains_under_a_side_of_the_frame_turn_to_dirt_where_the_floor_shows() {
    let aspect = 4.0 / 3.0;
    let mut state = stranded(aspect, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    state.config.events.grass = true;
    let layout = Layout::new(aspect);
    // A blow has thrown them against the left wall, under the frame.
    state.bury_stranded(layout.bounds_min.x + 1.0);
    let soil = state.soil.as_ref().unwrap();
    assert_eq!(
        soil.accounting().added,
        (2 * REMAINS_REACH as u64 + 1) * crate::grass::SOIL_DEPTH_CELLS as u64
    );
    let roots: Vec<_> = state
        .plants
        .plants
        .observe()
        .tissues
        .iter()
        .filter(|tissue| tissue.role == engine_core::plants::Role::Root)
        .map(|tissue| tissue.cell[0])
        .collect();
    let [column] = roots[..] else {
        panic!("{roots:?}")
    };
    // All three columns of its dirt show, and the seed in the middle one.
    let (first, last) = soil
        .columns_within(
            layout.bounds_min.x + layout.frame_width,
            layout.bounds_max.x - layout.frame_width,
        )
        .unwrap();
    assert!(first <= column - REMAINS_REACH && column + REMAINS_REACH <= last);
    for x in column - REMAINS_REACH..=column + REMAINS_REACH {
        let cell = soil
            .terrain()
            .cell(engine_terrain::CellCoord::new(x, 0))
            .unwrap();
        assert_eq!(cell.material, crate::grass::CLOCK_DIRT, "{x}");
    }
}

#[test]
fn a_stay_through_falling_replays_alike_from_its_seed() {
    let run = || {
        let mut state = falling_on_the_floor(2, 109.2);
        let mut seen = Vec::new();
        for _ in 0..12 * 60 {
            tick(&mut state);
            let soil = state.soil.as_ref().map(|soil| soil.accounting().added);
            seen.push((state.stranded_state(), soil));
        }
        seen
    };
    assert_eq!(run(), run());
}

#[test]
fn a_digit_that_changes_under_it_sends_it_to_another_place_and_takes_its_flag() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    // Onto the last digit, the first to change, with its flag there.
    state.stranded.flag = None;
    let own = *blocks_of(&state)
        .iter()
        .find(|block| block.key[0] == 3 && !block.covered)
        .unwrap();
    send(&mut state, Place::Block(own));
    until_phase(&mut state, Phase::Standing, PLANT_TICKS + 2);
    assert_eq!(state.stranded.flag.unwrap().key, own.key);
    let (age, flights) = (spaceling(&state).age, spaceling(&state).flights);
    // Another digit changing leaves it and its flag be.
    read(&mut state, [22, 34, 5]);
    assert_eq!(spaceling(&state).phase, Phase::Standing);
    assert!(state.stranded.flag.is_some());
    // Its own takes the block, between ticks: it is in the air at once, for
    // a block that is there, and the flag is gone.
    read(&mut state, [22, 35, 0]);
    let visitor = spaceling(&state);
    assert_eq!((visitor.age, visitor.flights), (age, flights + 1));
    assert_eq!(visitor.phase, Phase::Flying);
    let Place::Block(next) = visitor.place else {
        panic!("{:?}", visitor.place);
    };
    assert_ne!(next.surface, own.surface);
    assert!(blocks_of(&state).contains(&next));
    assert!(state.stranded.flag.is_none());
    until_phase(&mut state, Phase::Planting, FLIGHT_TICKS[1] + 1);
    assert_eq!(spaceling(&state).feet, next.feet);
}

#[test]
fn a_digit_slide_moves_it_off_the_digit_that_slides_and_no_other() {
    for on in [3, 0] {
        let mut state = stranded_at(5.0 / 3.0, 42, [12, 34, 5], true);
        until_phase(&mut state, Phase::Standing, 10 * 60);
        // On a digit, with its flag there.
        state.stranded.flag = None;
        let own = *blocks_of(&state)
            .iter()
            .find(|block| block.key[0] == on && !block.covered)
            .unwrap();
        send(&mut state, Place::Block(own));
        until_phase(&mut state, Phase::Standing, PLANT_TICKS + 2);
        spaceling_mut(&mut state).until = u64::MAX;
        // A slide waits for nothing: past the fight's cooldown, the minute
        // turns as time moves, and the last digit slides.
        while state.lifecycle() != crate::EventLifecycle::Idle {
            tick(&mut state);
        }
        read(&mut state, [12, 34, 59]);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(12, 35, 0).unwrap(),
            )],
            Duration::from_secs_f64(1.0 / 60.0),
        );
        assert_eq!(state.event_kind(), Some(ClockEventKind::DigitSlide));
        let mut flew = false;
        while state.event_kind() == Some(ClockEventKind::DigitSlide) {
            let visitor = spaceling(&state);
            assert!(visitor.leaving.is_none());
            assert!(blocks_of(&state).iter().all(|block| block.key[0] != 3));
            flew |= visitor.phase == Phase::Flying;
            if let Place::Block(block) = visitor.place {
                assert_ne!(block.key[0], 3);
            }
            tick(&mut state);
        }
        assert_eq!(flew, on == 3);
        assert_eq!(state.stranded.flag.is_some(), on == 0);
        // The digit at rest again, its blocks are there to stand on.
        tick(&mut state);
        assert!(blocks_of(&state).iter().any(|block| block.key[0] == 3));
    }
}

#[test]
fn it_shelters_from_the_rain_under_a_lit_block_of_a_digit() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    send_to_floor(&mut state);
    act(&mut state, ClockAction::preview_event(ClockEventKind::Rain));
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    // It leaves the floor at once for a block with another lit block of
    // its column over it: the nearest of those closest under their cover,
    // the top of a lower stroke under the upper one.
    let visitor = spaceling(&state);
    let Place::Block(cover) = visitor.place else {
        panic!("{:?}", visitor.place);
    };
    assert_eq!(visitor.phase, Phase::Flying);
    let from = visitor.feet;
    let nearest = blocks_of(&state)
        .into_iter()
        .filter(|block| block.covered && block.headroom == 1)
        .map(|block| block.feet.distance_to(from))
        .fold(f32::INFINITY, f32::min);
    assert!(cover.covered);
    assert_eq!(cover.headroom, 1);
    assert_eq!(cover.feet.distance_to(from), nearest);
    until_phase(&mut state, Phase::Sheltering, FLIGHT_TICKS[1] + 1);
    let flags = spaceling(&state).flags;
    // It stays there as long as it rains, and plants nothing.
    let mut rained = 0;
    while state.event_kind() == Some(ClockEventKind::Rain) {
        let visitor = spaceling(&state);
        assert_eq!(
            (visitor.phase, visitor.place),
            (Phase::Sheltering, Place::Block(cover))
        );
        assert!(visitor.feet.distance_to(cover.feet) < 0.3 * visitor.scale);
        tick(&mut state);
        rained += 1;
    }
    assert!(rained > 20 * 60);
    // The rain over, it stands a little longer, then moves on.
    tick(&mut state);
    let visitor = spaceling(&state);
    assert_eq!((visitor.phase, visitor.flags), (Phase::Standing, flags));
    until_phase(&mut state, Phase::Flying, *AFTER_RAIN.end() + 2);
}

#[test]
fn with_no_block_to_shelter_under_it_waits_out_the_rain_on_top_of_a_block() {
    // The dashes of an unknown time have no lit block under another.
    let mut state = stranded(5.0 / 3.0, 42);
    act(&mut state, ClockAction::set_time_unknown());
    assert!(blocks_of(&state).iter().all(|block| !block.covered));
    until_phase(&mut state, Phase::Standing, 10 * 60);
    send_to_floor(&mut state);
    act(&mut state, ClockAction::preview_event(ClockEventKind::Rain));
    let Place::Block(top) = spaceling(&state).place else {
        panic!("{:?}", spaceling(&state).place);
    };
    until_phase(
        &mut state,
        Phase::Standing,
        FLIGHT_TICKS[1] + PLANT_TICKS + 2,
    );
    while state.event_kind() == Some(ClockEventKind::Rain) {
        let visitor = spaceling(&state);
        assert_eq!(
            (visitor.phase, visitor.place),
            (Phase::Standing, Place::Block(top))
        );
        tick(&mut state);
    }
}

/// The birds the spaceling sees when a visit's only crow is `at`, going to
/// `going`.
fn one_crow(at: Option<Vec2>, going: Option<Vec2>) -> [Option<Vec2>; BIRDS] {
    let mut birds = [None; BIRDS];
    birds[0] = at;
    birds[1] = going;
    birds
}

/// What is around the spaceling in `state`, with `birds` there.
fn around(state: &ClockState, birds: [Option<Vec2>; BIRDS]) -> Surroundings<'_> {
    Surroundings {
        surfaces: &state.published,
        birds,
        ..state.stranded_view()
    }
}

#[test]
fn it_keeps_clear_of_a_crow_and_of_where_a_crow_is_going() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    let layout = Layout::new(state.aspect_ratio());
    let reach = BIRD_CLEAR * layout.pitch;
    for coming in [false, true] {
        // A crow lands on its block, or sets out for it from afar.
        let own = spaceling(&state).place.feet();
        let far = Vec2::new(layout.bounds_max.x, layout.canopy_y);
        let birds = if coming {
            one_crow(Some(far), Some(own))
        } else {
            one_crow(Some(own), None)
        };
        spaceling_mut(&mut state).wary = 0;
        let published = state.published.clone();
        let view = Surroundings {
            surfaces: &published,
            birds,
            ..state.stranded_view()
        };
        let world = state.arena.held_mut(Owner::Stranded).unwrap();
        let visitor = state.stranded.spaceling.as_mut().unwrap();
        visitor.synchronize(world, view);
        assert_eq!(visitor.phase, Phase::Flying, "{coming}");
        let to = visitor.place.feet();
        assert!(to.distance_to(own) > reach, "{coming}");
        until_phase(
            &mut state,
            Phase::Standing,
            FLIGHT_TICKS[1] + PLANT_TICKS + 2,
        );
    }
    // Walking the floor, it goes by where it is as well as where it is
    // going.
    send_to_floor(&mut state);
    let visitor = spaceling_mut(&mut state);
    visitor.walks = 1;
    visitor.until = visitor.age;
    until_phase(&mut state, Phase::Walking, 3);
    let (feet, to) = (spaceling(&state).feet, spaceling(&state).place.feet());
    let beside = feet - Vec2::X * ((to.x - feet.x).signum() * 0.9 * reach);
    assert!(beside.distance_to(to) > reach);
    spaceling_mut(&mut state).wary = 0;
    let published = state.published.clone();
    let view = Surroundings {
        surfaces: &published,
        birds: one_crow(Some(beside), None),
        ..state.stranded_view()
    };
    let world = state.arena.held_mut(Owner::Stranded).unwrap();
    let visitor = state.stranded.spaceling.as_mut().unwrap();
    visitor.synchronize(world, view);
    assert_eq!(visitor.phase, Phase::Flying);
    assert!(visitor.place.feet().distance_to(beside) > reach);
    until_phase(
        &mut state,
        Phase::Standing,
        FLIGHT_TICKS[1] + PLANT_TICKS + 2,
    );
    // With nowhere clear of it, it stays where it is.
    let own = spaceling(&state).place;
    let everywhere = one_crow(Some(own.feet()), None);
    let crowd: Vec<Block> = blocks_of(&state);
    assert!(crowd.len() > 1);
    let published: Vec<_> = state
        .published
        .iter()
        .copied()
        .filter(|surface| Some(surface.id) == own.block().map(|block| block.surface))
        .collect();
    let view = Surroundings {
        surfaces: &published,
        birds: everywhere,
        floor: false,
        ..state.stranded_view()
    };
    spaceling_mut(&mut state).wary = 0;
    let world = state.arena.held_mut(Owner::Stranded).unwrap();
    let visitor = state.stranded.spaceling.as_mut().unwrap();
    visitor.synchronize(world, view);
    assert_eq!((visitor.phase, visitor.place), (Phase::Standing, own));
    let _ = around(&state, one_crow(None, None));
}

#[test]
fn a_real_crow_visit_never_finds_it_within_reach() {
    for seed in [42, 43] {
        let mut state = stranded(5.0 / 3.0, seed);
        until_phase(&mut state, Phase::Standing, 10 * 60);
        act(&mut state, ClockAction::preview_event(ClockEventKind::Crow));
        assert!(state.crow_state().is_some());
        let layout = Layout::new(state.aspect_ratio());
        let mut nearest = f32::INFINITY;
        while let Some(crow) = state.crow() {
            let visitor = spaceling(&state);
            if visitor.phase != Phase::Flying {
                nearest = nearest.min(crow.position.distance_to(visitor.feet));
            }
            tick(&mut state);
        }
        // The crow's body is 0.4 pitches across: it never touched it.
        assert!(nearest > layout.pitch, "{seed}: {nearest}");
        assert!(spaceling(&state).leaving.is_none());
        assert_eq!(spaceling(&state).knockdowns, 0);
    }
}

#[test]
fn a_ducks_course_sends_it_up_onto_the_digits_until_the_floor_is_back() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    send_to_floor(&mut state);
    act(&mut state, ClockAction::preview_event(ClockEventKind::Duck));
    assert!(state.duck_state().is_some());
    // The course takes the floor's place, and it takes off for a digit.
    let visitor = spaceling(&state);
    assert_eq!(visitor.phase, Phase::Flying);
    assert!(visitor.place.block().is_some());
    while state.duck_state().is_some() {
        tick(&mut state);
        let visitor = spaceling(&state);
        assert!(visitor.leaving.is_none());
        assert!(visitor.place.block().is_some());
    }
    // The floor back, it goes down to it again in time.
    let mut down = false;
    for _ in 0..4 * 60 * 60 {
        tick(&mut state);
        down |= spaceling(&state).place.block().is_none();
    }
    assert!(down);
}

#[test]
fn knocked_off_its_block_it_gets_up_and_flies_back() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    spaceling_mut(&mut state).until = u64::MAX;
    let own = spaceling(&state).place;
    let scale = spaceling(&state).scale;
    // Something sends it tumbling off its block, down to the floor.
    let world = state.arena.held_mut(Owner::Stranded).unwrap();
    assert!(world.set_velocity(body(), Vec2::new(14.0, 6.0) * scale, 18.0, true));
    until_phase(&mut state, Phase::GettingUp, 4 * 60);
    assert_eq!(spaceling(&state).knockdowns, 1);
    let flights = spaceling(&state).flights;
    for _ in 0..(GET_UP_TICKS + FLIGHT_TICKS[1] + 4 * 60) {
        tick(&mut state);
    }
    let visitor = spaceling(&state);
    assert_eq!((visitor.phase, visitor.place), (Phase::Standing, own));
    assert!(visitor.feet.distance_to(own.feet()) < 0.3 * scale);
    assert_eq!(visitor.flights, flights + 1);
    assert!(visitor.angle.abs() < 0.2);
    assert_eq!(state.stranded.accounting.lost, 0);
}

#[test]
fn a_resize_ends_its_stay_and_takes_its_flag() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    assert!(state.stranded.flag.is_some());
    // Pausing ages nothing.
    let before = state.stranded_state();
    for _ in 0..10 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.stranded_state(), before);
    state.set_aspect_ratio(4.0 / 3.0);
    assert!(state.stranded.spaceling.is_none() && state.stranded.flag.is_none());
    assert!(state.arena.idle());
    let published = state.stranded_state().unwrap();
    assert_eq!(
        (published.accounting.stranded, published.accounting.cleared),
        (1, 1)
    );
    for _ in 0..60 {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn the_next_fight_starts_with_it_on_the_floor_and_its_beam_takes_it_as_the_hills_rise() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    send_to_floor(&mut state);
    // The arena has room for both, and the hills' place need not be clear
    // of it.
    assert!(
        state
            .event_blocked_by_arena(ClockEventKind::Tanks)
            .is_none()
    );
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Tanks),
    );
    assert_eq!(state.event_phase(), Some(EventPhase::Raising));
    assert_eq!(
        state.arena.owners(),
        [Owner::Stranded, Owner::Event(ClockEventKind::Tanks)]
    );
    while state.stranded.spaceling.is_some() {
        assert_eq!(state.event_phase(), Some(EventPhase::Raising));
        tick(&mut state);
    }
    assert_eq!(state.arena.owners(), [Owner::Event(ClockEventKind::Tanks)]);
    // The fight goes on to leave its own spaceling behind, with the rolls
    // for it.
    event(&mut state).survivors.rolls = [Some(survivor::Ejection::Thrown); 2];
    event(&mut state).survivors.ending_rolls = [0.99; 2];
    while state.event_kind() == Some(ClockEventKind::Tanks) {
        tick(&mut state);
    }
    let accounting = state.stranded.accounting;
    assert_eq!((accounting.stranded, accounting.event), (2, 1));
    assert!(state.stranded.spaceling.is_some());
}

#[test]
fn a_fight_cut_short_leaves_a_spaceling_behind_only_once_its_hills_withdraw() {
    use survivor::Phase as Survivor;
    // Waving for a beam, or standing as the winner goes up, it is still the
    // fight's, and goes with it.
    for (phase, resized) in [
        (Survivor::Waving, false),
        (Survivor::Stranded, false),
        (Survivor::Stranded, true),
    ] {
        let mut state = ready_with(5.0 / 3.0, 42, [12, 34, 5], false);
        fight(&mut state);
        while event(&mut state)
            .survivors
            .out
            .first()
            .is_none_or(|survivor| survivor.phase != phase)
        {
            tick(&mut state);
        }
        tick(&mut state);
        if resized {
            state.set_aspect_ratio(4.0 / 3.0);
        } else {
            state.finish_event();
        }
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert!(state.event_kind().is_none());
        assert!(state.stranded_state().is_none(), "{phase:?}");
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(state.arena.idle());
    }
    // Once the hills have begun to withdraw it is the clock's, and stays
    // when the fight is cut short.
    let mut state = ready_with(5.0 / 3.0, 42, [12, 34, 5], false);
    fight(&mut state);
    while state.stranded.spaceling.is_none() {
        tick(&mut state);
    }
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    state.finish_event();
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    assert!(state.event_kind().is_none());
    assert_eq!(state.arena.owners(), [Owner::Stranded]);
    until_phase(
        &mut state,
        Phase::Standing,
        FLIGHT_TICKS[1] + PLANT_TICKS + 2,
    );
    assert!(spaceling(&state).place.block().is_some());
    assert_eq!(state.stranded.accounting.stranded, 1);
}

#[test]
fn a_cell_swap_leaves_other_cells_at_the_same_places_to_stand_on() {
    let mut state = ready_with(5.0 / 3.0, 1, [12, 34, 5], false);
    let places = |state: &ClockState| {
        let mut places: Vec<_> = blocks_of(state)
            .into_iter()
            .map(|block| (block.key, block.covered, block.headroom))
            .collect();
        places.sort();
        places
    };
    let ids = |state: &ClockState| {
        let mut ids: Vec<_> = blocks_of(state)
            .into_iter()
            .map(|block| (block.key, block.surface))
            .collect();
        ids.sort();
        ids
    };
    let (before, cells) = (places(&state), ids(&state));
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::CellSwap),
    );
    // While cells are on their way, the slots they leave and cross give
    // none.
    let mut fewest = before.len();
    while state.event_kind() == Some(ClockEventKind::CellSwap) {
        tick(&mut state);
        fewest = fewest.min(blocks_of(&state).len());
    }
    assert!(fewest < before.len());
    tick(&mut state);
    // The same places hold a lit cell, though many hold another one.
    assert_eq!(places(&state), before);
    assert_ne!(ids(&state), cells);
}

#[test]
fn with_nowhere_to_stand_for_a_moment_it_hovers_until_a_place_is_back() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    // A duck's course lies over the floor, and then the digits' reels turn.
    act(&mut state, ClockAction::preview_event(ClockEventKind::Duck));
    assert!(state.duck_state().is_some());
    for _ in 0..3 * 60 {
        tick(&mut state);
    }
    until_phase(&mut state, Phase::Standing, 20 * 60);
    let stood = spaceling(&state).feet;
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::SlotMachine),
    );
    assert_eq!(state.event_kind(), Some(ClockEventKind::SlotMachine));
    assert!(state.duck_state().is_some());
    // No digit rests and the floor is a course: its jetpack lifts it a
    // little above where it stood and holds it there.
    let visitor = spaceling(&state);
    assert!(blocks_of(&state).is_empty());
    assert_eq!(visitor.phase, Phase::Flying);
    let Place::Air(aloft) = visitor.place else {
        panic!("{:?}", visitor.place);
    };
    assert_eq!(aloft, stood + Vec2::Y * (HOVER_RISE * visitor.scale));
    until_phase(&mut state, Phase::Hovering, FLIGHT_TICKS[1] + 1);
    let mut hovered = 0;
    while state.event_kind() == Some(ClockEventKind::SlotMachine) {
        let visitor = spaceling(&state);
        if visitor.phase == Phase::Hovering {
            hovered += 1;
            assert!(visitor.feet.distance_to(aloft) <= 1.01 * HOVER_BOB * visitor.scale);
            assert!(visitor.jet().is_some());
        }
        assert!(visitor.leaving.is_none());
        tick(&mut state);
    }
    assert!(hovered > 3 * 60, "{hovered}");
    // The reels at rest, it flies to a digit again.
    tick(&mut state);
    let visitor = spaceling(&state);
    assert_eq!(visitor.phase, Phase::Flying);
    assert!(visitor.place.block().is_some());
    until_phase(
        &mut state,
        Phase::Standing,
        FLIGHT_TICKS[1] + PLANT_TICKS + 2,
    );
    assert_eq!(state.stranded.accounting.no_place, 0);
}

#[test]
fn its_beam_comes_once_it_has_had_nowhere_to_stand_for_as_long_as_it_hovers() {
    let mut state = stranded(5.0 / 3.0, 42);
    until_phase(&mut state, Phase::Standing, 10 * 60);
    // No digit and no floor, for good.
    let nowhere = Surroundings {
        surfaces: &[],
        floor: false,
        ..state.stranded_view()
    };
    let mut flag = None;
    let mut aloft = 0;
    loop {
        let world = state.arena.held_mut(Owner::Stranded).unwrap();
        let visitor = state.stranded.spaceling.as_mut().unwrap();
        visitor.drive(world);
        if let Some(gone) = visitor.step(world, nowhere, &mut flag) {
            assert_eq!(gone, Gone::Left(Departure::NoPlace));
            break;
        }
        if visitor.leaving.is_none() {
            aloft += 1;
            assert!(matches!(visitor.phase, Phase::Flying | Phase::Hovering));
        }
        assert!(aloft < HOVER_TICKS);
    }
    // Its beam comes that long after it last had a place, a tick ago.
    assert_eq!(aloft + 1, HOVER_TICKS);
}
