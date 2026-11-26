use super::*;
use engine_material::WOOD;
use engine_rapier::world::loading::RegionId;

const TICK: Duration = Duration::from_nanos(16_666_667);

fn config(suspension: bool, course: Option<DrivingCourse>) -> ScorchedConfig {
    ScorchedConfig {
        suspension,
        cpu_driving: false,
        course,
        demo: course.is_some(),
        ..Default::default()
    }
}

/// A duel whose tanks hold their fire and stay put, settled on its ground:
/// the hills, or a course's floor.
fn quiet(suspension: bool, course: Option<DrivingCourse>) -> ScorchedState {
    let mut state = ScorchedState::new(config(suspension, course), 42);
    for tank in &mut state.duel.tanks {
        tank.next_shot = u64::MAX;
    }
    run(&mut state, 120);
    state
}

/// Steps `state` for `ticks` and returns every contact the steps listed.
fn run(state: &mut ScorchedState, ticks: u64) -> Vec<Contact> {
    let mut contacts = Vec::new();
    for _ in 0..ticks {
        ScorchedScenario::step(state, &[], TICK);
        contacts.extend_from_slice(state.contacts());
    }
    contacts
}

/// Steps `state` for `ticks` among the host's `surfaces`, and returns every
/// contact the steps listed.
fn run_among(
    state: &mut ScorchedState,
    surfaces: &mut impl ContactSurfaces,
    ticks: u64,
) -> Vec<Contact> {
    let mut contacts = Vec::new();
    for _ in 0..ticks {
        let drives = state.duel.prepare_controls(&mut state.physics, 0.0);
        state.physics.step_with_surface_drives(DT, &drives);
        state
            .duel
            .finish_step(&mut state.physics, &mut NoRecipients, surfaces);
        contacts.extend_from_slice(state.contacts());
    }
    contacts
}

fn bodies(state: &ScorchedState, slot: usize) -> Vec<BodyId> {
    match &state.running_gear[slot] {
        Some(assembly) => assembly.bodies().to_vec(),
        None => vec![state.tanks[slot].body],
    }
}

/// Moves a tank by `offset`, turns each of its bodies by `turn` and gives
/// every part of it `velocity`.
fn shift(state: &mut ScorchedState, slot: usize, offset: Vec2, turn: f32, velocity: Vec2) {
    for body in bodies(state, slot) {
        let motion = state.physics.motion(body).unwrap();
        assert!(
            state
                .physics
                .set_pose(body, motion.position + offset, motion.angle + turn, true)
        );
        assert!(state.physics.set_velocity(body, velocity, 0.0, true));
    }
}

fn position(state: &ScorchedState, slot: usize) -> Vec2 {
    state
        .physics
        .motion(state.tanks[slot].body)
        .unwrap()
        .position
}

/// The health `work` in the armor stands for.
fn health(work: f32) -> f32 {
    100.0 * work / armor().capacity()
}

/// The work the tanks' armor took in `contacts`, by slot.
fn absorbed(contacts: &[Contact]) -> [f32; 2] {
    let mut work = [0.0; 2];
    for contact in contacts {
        for side in 0..2 {
            if let Some(slot) = contact.tanks[side] {
                work[slot] += contact.absorbed[side];
            }
        }
    }
    work
}

/// Lifts tank 1 above and beside tank 0 on the flat course and throws it at
/// tank 0 at `speed`, then steps until they have met and settled.
fn knock(state: &mut ScorchedState, speed: f32) -> Vec<Contact> {
    let gap = position(state, 0) + Vec2::new(8.0, 2.5) - position(state, 1);
    shift(state, 1, gap, 0.0, Vec2::new(-speed, 2.0));
    run(state, 150)
}

#[test]
fn resting_driving_and_small_drops_cost_neither_gear_anything() {
    for suspension in [false, true] {
        // Settling from spawn, then resting, on the hills.
        let mut state = ScorchedState::new(config(suspension, None), 42);
        for tank in &mut state.duel.tanks {
            tank.next_shot = u64::MAX;
        }
        let settling = run(&mut state, 120);
        assert!(
            (0..2).all(|slot| settling.iter().any(|c| c.tanks.contains(&Some(slot)))),
            "each landing is priced, sprung={suspension}"
        );
        let resting = run(&mut state, 600);
        assert_eq!(absorbed(&settling), [0.0; 2]);
        assert_eq!(absorbed(&resting), [0.0; 2]);
        assert_eq!(state.tanks.map(|tank| tank.health), [100.0; 2]);

        // The roughest courses, driven to their flags.
        for course in [DrivingCourse::Slope, DrivingCourse::Rubble] {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    cpu_driving: true,
                    course_goal_driving: true,
                    ..config(suspension, Some(course))
                },
                42,
            );
            let driving = run(&mut state, 1500);
            assert!(position(&state, 0).x - course::START_X > 15.0);
            assert!(driving.len() > 5, "{course:?} sprung={suspension}");
            assert_eq!(absorbed(&driving), [0.0; 2], "{course:?}");
            assert_eq!(state.tanks[0].health, 100.0);
        }

        // Ten drops of half a unit, each its own impact.
        let mut state = quiet(suspension, Some(DrivingCourse::Flat));
        for drop in 0..10 {
            shift(&mut state, 0, Vec2::Y * 0.5, 0.0, Vec2::ZERO);
            let landing = run(&mut state, 90);
            let fastest = landing.iter().map(|c| c.closing_speed).fold(0.0, f32::max);
            assert!(
                (3.5..5.5).contains(&fastest),
                "drop {drop} lands at {fastest}, sprung={suspension}"
            );
            assert_eq!(absorbed(&landing), [0.0; 2]);
        }
        assert_eq!(state.tanks.map(|tank| tank.health), [100.0; 2]);
        let diagnostics = state.contact_diagnostics();
        assert_eq!(diagnostics.rejected_batches, 0);
        assert_eq!(diagnostics.refused_bindings, 0);
    }
}

#[test]
fn a_flat_landing_costs_nothing_and_a_corner_takes_the_blow() {
    // A hull that lands flat spreads the blow over the rock under its face,
    // which yields first. Its closing speed and energy are listed.
    let mut state = quiet(false, Some(DrivingCourse::Flat));
    shift(&mut state, 0, Vec2::Y * 10.0, 0.0, Vec2::ZERO);
    let landing = run(&mut state, 240);
    let flat = landing[0];
    assert!((18.5..20.0).contains(&flat.closing_speed), "{flat:?}");
    // Half the hull's mass, 38.8, times the approach squared.
    assert!((6_600.0..7_800.0).contains(&flat.energy), "{flat:?}");
    assert_eq!(flat.entities[0], GROUND);
    assert_eq!(flat.tanks, [None, Some(0)]);
    assert_eq!(flat.absorbed, [0.0; 2]);
    assert_eq!(flat.unpriced, None);
    assert_eq!(state.tanks[0].health, 100.0);

    // Tilted, the same hull lands on one corner from a quarter of the
    // height, and the plate there takes work: the corner's patch is a
    // twelfth as wide as the face.
    let mut state = quiet(false, Some(DrivingCourse::Flat));
    shift(&mut state, 0, Vec2::Y * 2.5, 0.3, Vec2::ZERO);
    let landing = run(&mut state, 240);
    let corner = landing[0];
    assert!((8.0..9.5).contains(&corner.closing_speed), "{corner:?}");
    assert!(corner.energy < 0.2 * flat.energy, "{corner:?}");
    assert!(corner.absorbed[1] > 0.0, "{corner:?}");
    let work = absorbed(&landing)[0];
    let lost = 100.0 - state.tanks[0].health;
    assert!((0.2..2.0).contains(&lost), "lost {lost}: {landing:?}");
    assert!((health(work) - lost).abs() < 1.0e-3, "{work} for {lost}");
    assert_eq!(state.tanks[1].health, 100.0);

    // A sprung tank lands on its wheels, each on its own mass of 2.9, and
    // the rock under them takes the work. These land under the 21 u/s at
    // which a wheel alone could cost.
    for (height, speed) in [(5.0, 13.3), (10.0, 18.7)] {
        let mut state = quiet(true, Some(DrivingCourse::Flat));
        shift(&mut state, 0, Vec2::Y * height, 0.0, Vec2::ZERO);
        let landing = run(&mut state, 100);
        let [left, right] = landing[..2] else {
            unreachable!()
        };
        for wheel in [left, right] {
            assert!(
                (wheel.closing_speed - speed).abs() < 0.5,
                "from {height}: {wheel:?}"
            );
            let own = 0.5 * 2.92 * wheel.closing_speed.powi(2);
            assert!((wheel.energy - own).abs() < 0.05 * own, "{wheel:?}");
            assert_eq!(wheel.tanks, [None, Some(0)]);
            assert_eq!(wheel.absorbed, [0.0; 2]);
        }
        assert_eq!(state.tanks[0].health, 100.0, "from {height}");
    }
}

#[test]
fn a_tank_thrown_into_the_other_costs_both_alike() {
    let mut lost = Vec::new();
    for suspension in [false, true] {
        let mut state = quiet(suspension, Some(DrivingCourse::Flat));
        let struck = position(&state, 0);
        let contacts = knock(&mut state, 8.0);
        let between: Vec<&Contact> = contacts
            .iter()
            .filter(|contact| contact.tanks.iter().all(Option::is_some))
            .collect();
        assert!(!between.is_empty(), "sprung={suspension}: {contacts:?}");
        let mut tanks = between[0].tanks;
        tanks.sort();
        assert_eq!(tanks, [Some(0), Some(1)]);
        // Armor meets armor, so both sides take the same work.
        for contact in &between {
            assert_eq!(contact.absorbed[0], contact.absorbed[1], "{contact:?}");
        }
        assert!(between[0].energy > 0.0 && between[0].absorbed[0] > 0.0);
        // Health follows the armor, and what it lost is what was listed.
        let work = absorbed(&contacts);
        let health_lost = state.tanks.map(|tank| 100.0 - tank.health);
        for slot in 0..2 {
            assert!(
                (health(work[slot]) - health_lost[slot]).abs() < 1.0e-3,
                "{work:?} for {health_lost:?}"
            );
            assert_eq!(
                state.tanks[slot].health,
                state.tanks[slot].armor.normalized_health()
            );
        }
        // The blow moves the tank it lands on.
        assert!(
            (position(&state, 0) - struck).length() > 0.25,
            "sprung={suspension}: {:?}",
            position(&state, 0) - struck
        );
        lost.push(health(
            between.iter().map(|contact| contact.absorbed[0]).sum(),
        ));
    }
    // The rigid hulls meet whole. A sprung chassis gives on its wheels.
    assert!((0.5..3.0).contains(&lost[0]), "{lost:?}");
    assert!((0.02..1.0).contains(&lost[1]), "{lost:?}");
}

/// Wood the host keeps: each piece's region and its depth into the screen.
#[derive(Default)]
struct Woods {
    pieces: BTreeMap<RegionToken, (Region, f32)>,
    charges: usize,
}

impl Woods {
    /// A plank `length` by `depth`, and `depth` into the screen, with its
    /// middle at `center`, bound as the host's own.
    fn plank(
        &mut self,
        world: &mut PhysicsWorld,
        id: u64,
        center: Vec2,
        length: f32,
        depth: f32,
    ) -> BodyId {
        let entity = PhysicsId::new(id);
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        let mut spec = ColliderSpec::cuboid(collider, length * 0.5, depth * 0.5);
        spec.density = WOOD.density;
        spec.friction = 0.8;
        assert!(world.insert_body(
            body,
            BodySpec {
                position: center,
                ccd_enabled: true,
                ..Default::default()
            },
            &[spec],
        ));
        let region = world
            .create_loading_region(RegionId { entity, part: 0 }, body)
            .unwrap();
        world
            .bind_loading_collider(collider, ContactSource { region, patch: 0 })
            .unwrap();
        let wood = Region::new(WOOD, length * depth * depth).unwrap();
        self.pieces.insert(region, (wood, depth));
        body
    }
}

impl ContactSurfaces for Woods {
    fn surface(&self, source: ContactSource) -> Option<ContactSurface> {
        let (region, depth) = *self.pieces.get(&source.region)?;
        Some(ContactSurface {
            region,
            thickness: depth,
            depth: Depth::Patch(Some(depth)),
            radius: None,
        })
    }

    fn absorb(&mut self, source: ContactSource, work: f32) {
        let (region, _) = self.pieces.get_mut(&source.region).unwrap();
        region.absorb(work).unwrap();
        self.charges += 1;
    }
}

#[test]
fn wood_breaks_on_a_tank_without_marking_it_and_still_pushes_it() {
    for suspension in [false, true] {
        // A trunk of mass 3.6, from low and from high, and a branch of 0.36.
        for (length, depth, height, breaks) in [
            (6.0, 1.0, 2.0, false),
            (6.0, 1.0, 20.0, true),
            (2.0, 0.3, 5.0, true),
        ] {
            let mut state = quiet(suspension, Some(DrivingCourse::Flat));
            let mut woods = Woods::default();
            let rest = position(&state, 0);
            let above = rest + Vec2::Y * (1.0 + depth * 0.5 + height);
            let wood = woods.plank(&mut state.physics, 90_000, above, length, depth);
            let label = format!("{length} by {depth} from {height}, sprung={suspension}");
            let mut lowest = rest.y;
            let mut struck = None;
            for _ in 0..240 {
                let contacts = run_among(&mut state, &mut woods, 1);
                lowest = lowest.min(position(&state, 0).y);
                for contact in contacts {
                    if !contact.entities.contains(&wood.entity) {
                        continue;
                    }
                    // The host's side shows no tank, and whatever the wood
                    // strikes, the armor takes nothing.
                    let side = usize::from(contact.entities[1] == wood.entity);
                    assert_eq!(contact.tanks[side], None, "{label}");
                    assert_eq!(contact.absorbed, [0.0; 2], "{label}");
                    if struck.is_none() && contact.tanks[1 - side] == Some(0) {
                        assert_eq!(contact.unpriced, None, "{label}");
                        let (region, _) = woods.pieces.values().next().unwrap();
                        struck = Some((region.integrity(), region.failed(), woods.charges));
                    }
                }
            }
            assert_eq!(state.tanks[0].health, 100.0, "{label}");
            // The wood yields on the tank, and breaks from high enough.
            let (integrity, failed, charges) = struck.expect("the wood lands on the tank");
            assert_eq!(charges, 1, "{label}");
            assert!(integrity < 0.9, "{label}: {integrity}");
            assert_eq!(failed, breaks, "{label}: {integrity}");
            // Broken or not, it lands with its weight, and a sprung tank dips
            // on its springs under a trunk.
            if suspension && length > 5.0 {
                assert!(
                    rest.y - lowest > 0.02,
                    "{label}: dipped {}",
                    rest.y - lowest
                );
            }
        }
    }
}

#[test]
fn two_tanks_can_wreck_each_other_in_one_step() {
    let mut state = quiet(false, Some(DrivingCourse::Flat));
    // What is left of each tank's armor is half the knock's cost.
    for tank in &mut state.duel.tanks {
        let left = tank.armor.remaining() - 0.006 * tank.armor.capacity();
        tank.absorb(left);
        assert!((0.5..0.7).contains(&tank.health), "{}", tank.health);
    }
    assert!(state.fighting());
    let gap = position(&state, 0) + Vec2::new(8.0, 2.5) - position(&state, 1);
    shift(&mut state, 1, gap, 0.0, Vec2::new(-8.0, 2.0));
    let mut ended = None;
    for tick in 0..150 {
        ScorchedScenario::step(&mut state, &[], TICK);
        if !state.fighting() {
            ended = Some(tick);
            break;
        }
    }
    assert!(ended.is_some(), "the knock ends the duel");
    // Both armors failed in the same impact, so nobody won.
    assert_eq!(state.tanks.map(|tank| tank.health), [0.0; 2]);
    assert!(state.tanks.iter().all(|tank| tank.armor.failed()));
    assert_eq!(state.winner(), None);
    let contact = state
        .contacts()
        .iter()
        .find(|contact| contact.tanks.iter().all(Option::is_some))
        .expect("the tanks met in the duel's last step");
    assert!(
        contact.absorbed.iter().all(|&work| work > 0.0),
        "{contact:?}"
    );
}

#[test]
fn pauses_clones_and_resets_price_each_impact_once() {
    for suspension in [false, true] {
        let mut plain = quiet(suspension, Some(DrivingCourse::Flat));
        let contacts = knock(&mut plain, 8.0);
        let lost = plain.tanks.map(|tank| 100.0 - tank.health);
        assert!(lost[0] > 0.0);

        // A pause between every two steps prices nothing more.
        let mut paused = quiet(suspension, Some(DrivingCourse::Flat));
        let gap = position(&paused, 0) + Vec2::new(8.0, 2.5) - position(&paused, 1);
        shift(&mut paused, 1, gap, 0.0, Vec2::new(-8.0, 2.0));
        let mut listed = Vec::new();
        for _ in 0..150 {
            ScorchedScenario::step(&mut paused, &[], TICK);
            listed.extend_from_slice(paused.contacts());
            // A pause takes no step, and pricing the same step again finds
            // nothing new in it.
            let before = paused.tanks.map(|tank| tank.health);
            let step = paused.contacts().to_vec();
            ScorchedScenario::step(&mut paused, &[], Duration::ZERO);
            paused
                .duel
                .charge_contacts(&mut paused.physics, &mut NoSurfaces);
            assert_eq!(paused.tanks.map(|tank| tank.health), before);
            assert_eq!(paused.contacts(), step);
        }
        assert_eq!(listed, contacts);
        assert_eq!(paused.observation_hash(), plain.observation_hash());

        // A clone taken mid-flight goes on as the duel does.
        let mut first = quiet(suspension, Some(DrivingCourse::Flat));
        let gap = position(&first, 0) + Vec2::new(8.0, 2.5) - position(&first, 1);
        shift(&mut first, 1, gap, 0.0, Vec2::new(-8.0, 2.0));
        let mut early = run(&mut first, 20);
        let mut second = first.clone();
        let late = run(&mut first, 130);
        assert_eq!(run(&mut second, 130), late);
        early.extend(late);
        assert_eq!(early, contacts);
        assert_eq!(second.tanks.map(|tank| 100.0 - tank.health), lost);
        assert_eq!(second.observation_hash(), plain.observation_hash());

        // A reset starts a world that prices contacts afresh.
        plain.command(Command::Reset);
        assert_eq!(plain.tanks.map(|tank| tank.health), [100.0; 2]);
        assert_eq!(plain.contact_diagnostics(), Default::default());
        assert!(plain.contacts().is_empty());
        for tank in &mut plain.duel.tanks {
            tank.next_shot = u64::MAX;
        }
        run(&mut plain, 120);
        assert_eq!(knock(&mut plain, 8.0), contacts);
        assert_eq!(plain.tanks.map(|tank| 100.0 - tank.health), lost);
    }
}

/// Clock's fights at Scorched's own range, which its field fits: CPUs that
/// plot or learn, drive and dodge, on slumping dirt.
fn dodging(suspension: bool, seed: u64) -> ScorchedConfig {
    ScorchedConfig {
        shape: if seed & 1 == 0 {
            GrainShape::Round
        } else {
            GrainShape::Hexagon
        },
        demo: true,
        cpu_dodging: [seed / 6 % 2 == 1, seed / 12 % 2 == 1],
        cpu_policy: match seed % 3 {
            0 => [CpuPolicy::Plotted; 2],
            1 => [CpuPolicy::Plotted, CpuPolicy::Reactive],
            _ => [CpuPolicy::Reactive, CpuPolicy::Plotted],
        },
        first_to_fire: (seed / 3 % 2) as usize,
        slumping: true,
        cohesion: BankStrength::Weak,
        suspension,
        ..Default::default()
    }
}

#[test]
fn duels_cost_sprung_tanks_nothing_and_leave_the_physics_alone() {
    // A duel in a world that captures no contacts plays out tick for tick as
    // one that does, since no contact costs a sprung tank anything.
    for seed in [6, 13] {
        let config = dodging(true, seed);
        let mut priced = ScorchedState::new(config, seed);
        let mut unpriced = ScorchedState::in_world(ScorchedState::mechanics(), config, seed);
        let mut contacts = Vec::new();
        // Every terrain body the duel has held, fallen pieces included.
        let mut terrain = BTreeSet::new();
        let mut ticks = 0;
        while ticks < 60 * 60 && priced.fighting() {
            let bodies = priced.terrain.iter();
            terrain.extend(bodies.map(|fragment| fragment.assembly.body().entity));
            ScorchedScenario::step(&mut priced, &[], TICK);
            ScorchedScenario::step(&mut unpriced, &[], TICK);
            contacts.extend_from_slice(priced.contacts());
            assert!(unpriced.contacts().is_empty());
            ticks += 1;
        }
        assert_eq!(priced.observation_hash(), unpriced.observation_hash());
        assert!(priced.shots > 2, "seed {seed}");
        assert!(contacts.len() > 10, "seed {seed}: {}", contacts.len());
        assert_eq!(absorbed(&contacts), [0.0; 2], "seed {seed}");
        // Loose dirt and shells have no region, so only the terrain and the
        // other tank are listed against a tank.
        let tanks = priced.tanks.map(|tank| tank.body.entity);
        assert!(contacts.iter().all(|contact| {
            contact
                .entities
                .iter()
                .all(|entity| tanks.contains(entity) || terrain.contains(entity))
        }));
        assert!(priced.loose_cells() > 0, "seed {seed}");
        assert!(
            priced
                .loose
                .iter()
                .all(|grain| priced.physics.loading_binding(grain.collider()).is_none())
        );
        let diagnostics = priced.contact_diagnostics();
        assert_eq!(diagnostics.rejected_batches, 0);
        assert_eq!(diagnostics.refused_bindings, 0);
    }
}

#[test]
fn a_charge_that_takes_all_the_scaled_armor_has_left_fails_the_armor() {
    // Any duel will do: the surfaces only need a tank's region.
    let state = quiet(false, Some(DrivingCourse::Flat));
    let region = state.duel.contacts.regions[&state.tanks[0].body];
    let source = ContactSource { region, patch: 0 };
    for scale in [0.75_f32, 1.0, 2.0, 3.37, 4.55, 7.3] {
        for step in 1..=200 {
            // Armor with a little left, less a far smaller charge, so that
            // what is left is no f32.
            let left = armor().capacity() * (1.0e-4 + 3.7e-4 * step as f32);
            let mut tanks = state.duel.tanks;
            let spent = tanks[0].armor.remaining() - left;
            tanks[0].absorb(spent);
            tanks[0].absorb(left * 1.234_567e-4);
            let before = tanks[0].armor.remaining_precise();
            assert_ne!(f64::from(before as f32), before);
            let mut surfaces = Surfaces {
                parts: &state.duel.contacts.parts,
                tanks: &mut tanks,
                wheel_radius: 0.65,
                scale,
                host: &mut NoSurfaces,
            };
            let seen = surfaces.surface(source).unwrap().region;
            // Half of what the scaled armor has left is half of the armor's.
            surfaces.absorb(source, 0.5 * seen.remaining());
            let half = surfaces.tanks[0].armor.remaining_precise() / before;
            assert!((half - 0.5).abs() < 1.0e-5, "scale {scale}: {half}");
            // The law never consumes more than a surface has left, and
            // consuming all of it fails the armor, with nothing left over.
            let seen = surfaces.surface(source).unwrap().region;
            surfaces.absorb(source, seen.remaining());
            assert!(tanks[0].armor.failed(), "scale {scale}, {left} left");
            assert_eq!(tanks[0].health, 0.0, "scale {scale}, {left} left");
        }
    }
}

#[test]
fn a_landing_in_the_envelope_costs_nothing_wherever_the_terrains_seams_fall() {
    // Dropped at ten places a twentieth of a unit apart, across one of the
    // terrain's cells and the seams between its pieces. The health each place
    // cost a tank dropped from `height`.
    let drops = |suspension: bool, height: f32| {
        let quiet = quiet(suspension, Some(DrivingCourse::Flat));
        (0..10)
            .map(|place| {
                let mut state = quiet.clone();
                let lift = Vec2::new(0.05 * place as f32, height);
                shift(&mut state, 0, lift, 0.0, Vec2::ZERO);
                let landing = run(&mut state, 150);
                let fastest = landing.iter().map(|c| c.closing_speed).fold(0.0, f32::max);
                assert!(fastest > 15.0, "sprung={suspension}: {landing:?}");
                100.0 - state.tanks[0].health
            })
            .collect::<Vec<_>>()
    };
    // A sprung tank from 12 units, the most its wheels land from for nothing,
    // and a rigid one from 15.
    assert_eq!(drops(true, 12.0), [0.0; 10]);
    assert_eq!(drops(false, 15.0), [0.0; 10]);
    // From 15 units, a sprung tank's wheels land faster, and where the seams
    // fall sets the price: 0 to 0.34 here.
    let fifteen = drops(true, 15.0);
    assert!(fifteen.contains(&0.0), "{fifteen:?}");
    assert!(
        fifteen.iter().any(|&lost| lost > 0.1 && lost < 0.5),
        "{fifteen:?}"
    );
}
