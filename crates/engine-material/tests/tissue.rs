//! The tissue adapter on a real plant world, loaded with the #326 reference
//! bodies: safe knocks, accumulated hits, simultaneous failures, sustained
//! loads on dead wood, growth, shrinking, removal, saves and refusals.

use engine_core::plants::{
    Config, Environment, Exposure, Intent, Plants, Rejection, Role, Support, Tissue, TissueId,
};
use engine_material::fixtures::{
    Body, GRAVITY, TICK, branch, geometry, head_on, hits_to_failure, press, safe_limit_speed,
    spaceling, stone, strike, tank,
};
use engine_material::{
    MaterialError, Outcome, SOFT_TISSUE, SustainedLoad, TissueError, TissueIntegrity,
    TissueMaterial, TissueRecord, TissueSave, TissueTable, WOOD,
};

#[derive(Clone, Copy)]
struct Field {
    light: u16,
    nutrients: u16,
    water: u16,
}

impl Environment for Field {
    fn exposure(&self, _: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: true,
            surface_contact: true,
            nutrients: self.nutrients,
            water: self.water,
            light: self.light,
        }
    }

    fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
        true
    }

    fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
        true
    }
}

const SUN: Field = Field {
    light: 1000,
    nutrients: 1000,
    water: 1000,
};
const DARK: Field = Field { light: 0, ..SUN };
const BARREN: Field = Field {
    nutrients: 0,
    water: 0,
    ..SUN
};

const ROOT: TissueId = 1;
const SHOOT: TissueId = 2;

/// Woody roots and soft tissue elsewhere, at the volumes of the reference
/// branch and spaceling.
fn table() -> TissueTable {
    let soft = TissueMaterial {
        material: SOFT_TISSUE,
        volume_per_size: spaceling().volume,
    };
    let wood = TissueMaterial {
        material: WOOD,
        volume_per_size: branch().volume,
    };
    TissueTable {
        identity: 7,
        roles: [wood, soft, soft, soft, wood],
    }
}

/// A root and the shoot it grew, with root intake enough for the shoot to
/// develop.
fn pair() -> Plants {
    let config = Config {
        root_intake_per_size: [10, 10, 0],
        ..Config::default()
    };
    let mut plants = Plants::new(config, 3).unwrap();
    let root = plants
        .seed(1, Role::Root, 1, [0, 0], None, [20; 3], &SUN)
        .unwrap();
    step(
        &mut plants,
        SUN,
        &[Intent::Grow {
            parent: root,
            role: Role::Shoot,
            size: 1,
            cell: [1, 0],
            support: None,
        }],
    );
    assert_eq!((root, plants.observe().tissues.len()), (ROOT, 2));
    plants
}

fn step(plants: &mut Plants, field: Field, intents: &[Intent]) {
    plants
        .tick(plants.observe().tick, false, 1, intents, &field)
        .unwrap();
    assert!(plants.observe().ledger.results.iter().all(Result::is_ok));
}

fn tissue(plants: &Plants, id: TissueId) -> Tissue {
    plants
        .observe()
        .tissues
        .into_iter()
        .find(|tissue| tissue.id == id)
        .unwrap()
}

/// A fresh reference stone hits the tissue, which `body` describes.
fn hit(integrity: &mut TissueIntegrity, id: TissueId, body: &Body, speed: f32) -> Outcome {
    let stone = stone();
    integrity
        .strike(
            id,
            &mut stone.region(),
            geometry(&stone, body),
            head_on(&stone, body, speed),
        )
        .unwrap()
}

/// The same hit on a reference region, without the adapter.
fn hit_reference(region: &mut engine_material::Region, body: &Body, speed: f32) {
    let stone = stone();
    strike(
        &mut stone.region(),
        region,
        geometry(&stone, body),
        head_on(&stone, body, speed),
    );
}

fn rest() -> SustainedLoad {
    let plank = branch();
    SustainedLoad {
        force: tank().mass.unwrap() * GRAVITY,
        width: 2.0,
        section: plank.section(2.0),
        duration: TICK,
        measured_plastic_displacement: None,
    }
}

#[test]
fn safe_knocks_cost_tissue_nothing_however_often() {
    let plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    for (id, body) in [(ROOT, branch()), (SHOOT, spaceling())] {
        let speed = 0.99 * safe_limit_speed(&stone(), &body);
        for _ in 0..1000 {
            assert!(hit(&mut integrity, id, &body, speed).safe());
        }
        assert_eq!(integrity.region(id), Some(&body.region()));
    }
    assert!(integrity.take_failures().is_empty());
}

#[test]
fn hits_accumulate_and_fail_the_tissue_on_the_reference_hit() {
    let (hits, _, failed, _) = hits_to_failure(&stone(), &spaceling(), 2.0, 100);
    assert_eq!(hits, 6);
    let plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    let mut reference = spaceling().region();
    for n in 1..=hits {
        hit(&mut integrity, SHOOT, &spaceling(), 2.0);
        hit_reference(&mut reference, &spaceling(), 2.0);
        assert_eq!(integrity.region(SHOOT), Some(&reference));
        let expected = if n == hits { vec![SHOOT] } else { Vec::new() };
        assert_eq!(integrity.take_failures(), expected);
    }
    assert_eq!(integrity.region(SHOOT), Some(&failed));

    // The striker is charged too, as side `a`: a spaceling hitting the woody
    // root loses what the law consumes on its side.
    let (body, root) = (spaceling(), branch());
    let mut striker = body.region();
    let outcome = integrity
        .strike(
            ROOT,
            &mut striker,
            geometry(&body, &root),
            head_on(&body, &root, 10.0),
        )
        .unwrap();
    assert!(outcome.consumed[0] > 0.0);
    let mut expected = body.region();
    expected.absorb(outcome.consumed[0]).unwrap();
    assert_eq!(striker, expected);
}

#[test]
fn simultaneous_failures_come_back_once_in_identity_order() {
    let mut plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    // The #326 tissue-against-wood rows: one 10 wu/s stone fails the soft
    // shoot outright and leaves the woody root about 0.70 of its integrity.
    hit(&mut integrity, SHOOT, &spaceling(), 10.0);
    hit(&mut integrity, ROOT, &branch(), 10.0);
    let (mut soft, mut wood) = (spaceling().region(), branch().region());
    hit_reference(&mut soft, &spaceling(), 10.0);
    hit_reference(&mut wood, &branch(), 10.0);
    assert_eq!(integrity.region(SHOOT), Some(&soft));
    assert_eq!(integrity.region(ROOT), Some(&wood));
    assert!(soft.failed());
    assert!((wood.integrity() - 0.703).abs() < 5.0e-4);

    // The root then fails at the same boundary; both come back in identity
    // order, once.
    let unspent = integrity.charge(ROOT, 1.0e6).unwrap();
    assert_eq!(unspent, (1.0e6 - wood.remaining_precise()) as f32);
    assert_eq!(integrity.take_failures(), vec![ROOT, SHOOT]);

    // Later sources find nothing left to charge and report nothing new.
    let again = hit(&mut integrity, SHOOT, &spaceling(), 10.0);
    assert_eq!((again.consumed[1], again.failed[1]), (0.0, false));
    let crush = integrity
        .press(
            SHOOT,
            SustainedLoad {
                force: 1.0e4,
                section: spaceling().section(1.0),
                width: 1.0,
                ..rest()
            },
        )
        .unwrap();
    assert_eq!((crush.consumed[0], crush.failed[0]), (0.0, false));
    assert_eq!(integrity.charge(ROOT, 5.0), Ok(5.0));
    assert!(integrity.take_failures().is_empty());

    // The host retires each through the plants' own kill path, once: their
    // stores are lost and their cells stay until the host removes them.
    let (root, shoot) = (tissue(&plants, ROOT).stores, tissue(&plants, SHOOT).stores);
    let before = plants.observe().ledger.loss;
    for id in [ROOT, SHOOT, SHOOT] {
        plants.damage(id, 100).unwrap();
    }
    let lost = std::array::from_fn(|i| before[i] + root[i] + shoot[i]);
    assert_eq!(plants.observe().ledger.loss, lost);
    integrity.sync(&plants).unwrap();
    for id in [ROOT, SHOOT] {
        assert_eq!(tissue(&plants, id).health, 0);
        assert!(integrity.region(id).unwrap().failed());
    }
}

#[test]
fn health_and_integrity_never_cross() {
    let mut plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    hit(&mut integrity, SHOOT, &spaceling(), 2.0);
    let damaged = *integrity.region(SHOOT).unwrap();
    assert!(damaged.integrity() < 1.0 && !damaged.failed());
    let while_alive = integrity.save();

    // Shade costs health after three ticks of grace, and sunlight restores
    // it; the integrity stays exactly where the hit left it.
    for _ in 0..5 {
        step(&mut plants, DARK, &[]);
        integrity.sync(&plants).unwrap();
    }
    assert_eq!(tissue(&plants, SHOOT).health, 60);
    for _ in 0..4 {
        step(&mut plants, SUN, &[]);
        integrity.sync(&plants).unwrap();
    }
    assert_eq!(tissue(&plants, SHOOT).health, 100);
    assert_eq!(integrity.region(SHOOT), Some(&damaged));

    // A shoot that dies in the dark keeps its integrity.
    let mut dark = 0;
    while tissue(&plants, SHOOT).health > 0 {
        step(&mut plants, DARK, &[]);
        integrity.sync(&plants).unwrap();
        dark += 1;
    }
    assert_eq!(dark, 8);
    assert_eq!(integrity.region(SHOOT), Some(&damaged));

    // Repairing it, or restoring integrity saved while it lived, leaves it
    // dead: it cannot develop.
    integrity.repair(SHOOT, 1.0).unwrap();
    assert_eq!(integrity.region(SHOOT), Some(&spaceling().region()));
    let restored = TissueIntegrity::restore(table(), &plants, &while_alive).unwrap();
    assert_eq!(restored.region(SHOOT), Some(&damaged));
    plants
        .tick(
            plants.observe().tick,
            false,
            1,
            &[Intent::Develop { tissue: SHOOT }],
            &SUN,
        )
        .unwrap();
    assert_eq!(plants.observe().ledger.results, vec![Err(Rejection::Dead)]);
    assert_eq!(tissue(&plants, SHOOT).health, 0);
}

#[test]
fn dead_wood_bears_its_rest_for_nothing_and_fails_an_overload_on_schedule() {
    let mut plants = Plants::new(Config::default(), 5).unwrap();
    let root = plants
        .seed(1, Role::Root, 1, [0, 0], None, [0; 3], &BARREN)
        .unwrap();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    let mut starving = 0;
    while tissue(&plants, root).health > 0 {
        step(&mut plants, BARREN, &[]);
        integrity.sync(&plants).unwrap();
        starving += 1;
    }
    assert_eq!(starving, 8);
    assert_eq!(integrity.region(root), Some(&branch().region()));

    // The reference tank resting on the dead wood charges nothing.
    for _ in 0..1000 {
        assert_eq!(integrity.press(root, rest()).unwrap().consumed, [0.0; 2]);
    }
    assert_eq!(integrity.region(root), Some(&branch().region()));

    // A 1.05x overload fails it on the tick it fails the reference plank.
    let overload = SustainedLoad {
        force: 6300.0,
        ..rest()
    };
    let mut reference = branch().region();
    let mut expected = 0;
    while !reference.failed() {
        press(&mut reference, overload);
        expected += 1;
    }
    assert_eq!(expected, 12);
    let mut ticks = 0;
    while integrity.take_failures().is_empty() {
        assert!(ticks < expected, "no failure after {ticks} ticks");
        integrity.press(root, overload).unwrap();
        ticks += 1;
    }
    assert_eq!(ticks, expected);
    assert_eq!(integrity.region(root), Some(&reference));
}

#[test]
fn growth_adds_intact_material_and_keeps_the_damage() {
    let mut plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    hit(&mut integrity, SHOOT, &spaceling(), 2.0);
    let small = *integrity.region(SHOOT).unwrap();
    let damage = f64::from(small.capacity()) - small.remaining_precise();
    let develop = [Intent::Develop { tissue: SHOOT }];
    for _ in 0..4 {
        step(&mut plants, SUN, &develop);
        integrity.sync(&plants).unwrap();
    }
    assert_eq!(tissue(&plants, SHOOT).size, 2);
    let grown = *integrity.region(SHOOT).unwrap();
    assert_eq!(grown.volume(), 2.0 * spaceling().volume);
    assert_eq!(
        f64::from(grown.capacity()) - grown.remaining_precise(),
        damage
    );
    assert!(grown.integrity() > small.integrity());

    // A failed shoot the host leaves alive cannot grow until it is repaired.
    integrity.charge(SHOOT, 1.0e6).unwrap();
    assert_eq!(integrity.take_failures(), vec![SHOOT]);
    for _ in 0..4 {
        step(&mut plants, SUN, &develop);
    }
    assert_eq!(tissue(&plants, SHOOT).size, 3);
    let held = integrity.clone();
    assert_eq!(
        integrity.sync(&plants),
        Err(TissueError::FailedGrowth(SHOOT))
    );
    assert_eq!(integrity, held);
    integrity.repair(SHOOT, 0.5).unwrap();
    integrity.sync(&plants).unwrap();
    let repaired = integrity.region(SHOOT).unwrap();
    let added = f64::from(repaired.capacity()) - f64::from(grown.capacity());
    assert_eq!(
        repaired.remaining_precise(),
        0.5 * f64::from(grown.capacity()) + added
    );
}

#[test]
fn shrinking_keeps_integrity_and_removal_drops_the_record() {
    let mut plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    for _ in 0..4 {
        step(&mut plants, SUN, &[Intent::Develop { tissue: SHOOT }]);
    }
    integrity.sync(&plants).unwrap();
    hit(&mut integrity, SHOOT, &spaceling(), 2.0);
    let large = *integrity.region(SHOOT).unwrap();
    plants.resize_storage(SHOOT, 1).unwrap();
    integrity.sync(&plants).unwrap();
    let small = *integrity.region(SHOOT).unwrap();
    assert_eq!(small.volume(), spaceling().volume);
    assert!(small.remaining_precise() <= large.remaining_precise());
    assert!((small.integrity() - large.integrity()).abs() <= f32::EPSILON);

    // A failure not yet taken leaves with its tissue.
    integrity.charge(SHOOT, 1.0e6).unwrap();
    plants.remove(SHOOT).unwrap();
    integrity.sync(&plants).unwrap();
    assert_eq!(integrity.region(SHOOT), None);
    assert!(integrity.take_failures().is_empty());
    assert_eq!(
        integrity.charge(SHOOT, 1.0),
        Err(TissueError::Unknown(SHOOT))
    );

    // New tissue starts intact, and the records follow the plant world.
    step(
        &mut plants,
        SUN,
        &[Intent::Grow {
            parent: ROOT,
            role: Role::Runner,
            size: 1,
            cell: [0, 1],
            support: None,
        }],
    );
    integrity.sync(&plants).unwrap();
    let runner = SHOOT + 1;
    assert_eq!(integrity.region(runner), Some(&spaceling().region()));
    let ids: Vec<_> = integrity.save().records.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![ROOT, runner]);
}

#[test]
fn saves_continue_exactly_and_refuse_what_does_not_belong() {
    let plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    for _ in 0..5 {
        hit(&mut integrity, SHOOT, &spaceling(), 2.0);
    }
    integrity.charge(ROOT, 1.0e6).unwrap();
    let text = serde_json::to_string(&integrity.save()).unwrap();
    let save: TissueSave = serde_json::from_str(&text).unwrap();
    assert_eq!(save, integrity.save());
    let mut restored = TissueIntegrity::restore(table(), &plants, &save).unwrap();
    assert_eq!(restored, integrity);
    for state in [&mut integrity, &mut restored] {
        hit(state, SHOOT, &spaceling(), 2.0);
        assert_eq!(state.take_failures(), vec![ROOT, SHOOT]);
    }
    assert_eq!(restored, integrity);

    // Repairing a failed tissue before its failure is taken withdraws the
    // failure, so that save restores too.
    let mut repaired = TissueIntegrity::restore(table(), &plants, &save).unwrap();
    repaired.repair(ROOT, 0.25).unwrap();
    assert!(repaired.save().failures.is_empty());
    assert_eq!(
        TissueIntegrity::restore(table(), &plants, &repaired.save()),
        Ok(repaired)
    );

    let refused = |save: &TissueSave| TissueIntegrity::restore(table(), &plants, save);
    let other = TissueTable {
        identity: 8,
        ..table()
    };
    assert_eq!(
        TissueIntegrity::restore(other, &plants, &save),
        Err(TissueError::Mismatch)
    );
    let edits: [fn(&mut TissueSave); 9] = [
        |save| {
            save.records.pop();
        },
        |save| {
            save.records.push(TissueRecord {
                id: 9,
                size: 1,
                remaining: 0.0,
            })
        },
        |save| save.records.swap(0, 1),
        |save| save.records[1].size = 2,
        |save| save.records[1].remaining = 11.0,
        |save| save.records[1].remaining = -1.0,
        |save| save.records[1].remaining = f64::NAN,
        |save| save.failures = vec![SHOOT],
        |save| save.failures = vec![ROOT, ROOT],
    ];
    for edit in edits {
        let mut forged = save.clone();
        edit(&mut forged);
        assert_eq!(refused(&forged), Err(TissueError::Mismatch));
    }
    let mut larger = plants.clone();
    step(
        &mut larger,
        SUN,
        &[Intent::Grow {
            parent: ROOT,
            role: Role::Runner,
            size: 1,
            cell: [0, 1],
            support: None,
        }],
    );
    assert_eq!(
        TissueIntegrity::restore(table(), &larger, &save),
        Err(TissueError::Mismatch)
    );
}

#[test]
fn refused_calls_change_nothing() {
    let plants = pair();
    let mut integrity = TissueIntegrity::new(table(), &plants).unwrap();
    let before = integrity.clone();
    let mut striker = stone().region();
    let body = spaceling();
    assert_eq!(
        integrity.strike(
            9,
            &mut striker,
            geometry(&stone(), &body),
            head_on(&stone(), &body, 10.0)
        ),
        Err(TissueError::Unknown(9))
    );
    assert_eq!(striker, stone().region());
    let invalid = SustainedLoad {
        force: f32::NAN,
        ..rest()
    };
    assert_eq!(
        integrity.press(ROOT, invalid),
        Err(TissueError::Material(MaterialError::InvalidLoad))
    );
    assert_eq!(
        integrity.charge(ROOT, -1.0),
        Err(TissueError::Material(MaterialError::InvalidRegion))
    );
    assert_eq!(
        integrity.repair(ROOT, 1.5),
        Err(TissueError::Material(MaterialError::InvalidRegion))
    );
    assert_eq!(integrity, before);

    // Every size up to the plant world's limit must make a valid region.
    for volume_per_size in [0.0, f32::NAN, 1.0e11] {
        let mut bad = table();
        bad.roles[Role::Moss.index()].volume_per_size = volume_per_size;
        assert_eq!(
            TissueIntegrity::new(bad, &plants),
            Err(TissueError::Material(MaterialError::InvalidRegion))
        );
    }
}
