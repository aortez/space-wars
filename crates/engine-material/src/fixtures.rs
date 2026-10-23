//! Reference bodies, acceptance cases and parameter sweeps. Tests assert the
//! acceptance behavior; the `reference_cases` example prints these tables and
//! writes `docs/data/material-model-v2.json` and `docs/data/material-model-v2.md`,
//! which a test compares against the regenerated report so the published
//! evidence cannot drift from the code. The helpers panic on invalid input:
//! this is evidence scaffolding, not host API.

use crate::{
    ARMOR, Beam, Connection, ConnectionLoad, ContactBody, ContactGeometry, FailureMode, HULL,
    Impact, LAW_VERSION, LEDGER_TOLERANCE, Material, Outcome, ROCK, Region, SHELL, SOFT_TISSUE,
    Section, SustainedLoad, WOOD, connection, deformation_depth, effective_mass, impact,
    patch_width_from_radii, sustained, yield_speed,
};
use serde::Serialize;
use std::fmt::Write as _;

/// Fixed physics tick used by the sustained-load cases, seconds.
pub const TICK: f32 = 1.0 / 60.0;
/// Gravity for the resting-load cases, `wu / s^2`.
pub const GRAVITY: f32 = 10.0;
/// Step rates the fixed-step case compares, Hz. 240 Hz is the 60 Hz tick
/// with four substeps.
pub const STEP_RATES: [f32; 4] = [30.0, 60.0, 120.0, 240.0];
/// When the fixed-step case compares state, seconds: a whole number of ticks
/// at every rate in `STEP_RATES`.
pub const COMMON_TIME: f32 = 1.0 / 30.0;

/// A reference body: one material region with its contact geometry. Size,
/// mass and region are separate quantities, but the region never weighs more
/// than the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub name: &'static str,
    /// `None` is immovable.
    pub mass: Option<f32>,
    pub inertia: Option<f32>,
    /// `None` is a flat surface.
    pub radius: Option<f32>,
    pub material: Material,
    pub volume: f32,
    /// Represented depth into the screen, `wu`.
    pub thickness: f32,
    /// How far the material reaches behind a contact along the normal: a
    /// plate's thickness or a round body's diameter. `None` is a half-space.
    pub extent: Option<f32>,
}

impl Body {
    pub fn region(&self) -> Region {
        Region::new(self.material, self.volume).expect("reference body region")
    }

    /// The section behind a contact patch of `width`, at the default depth.
    pub fn section(&self, width: f32) -> Section {
        let depth = deformation_depth(width, self.extent).expect("reference extent");
        Section::new(self.thickness, depth)
    }

    /// Freely translating contact inertia; rotation is handled case by case.
    pub fn contact(&self) -> ContactBody {
        ContactBody {
            mass: self.mass,
            inertia: None,
            lever: 0.0,
        }
    }

    pub fn with_volume(self, volume: f32) -> Self {
        Self { volume, ..self }
    }

    pub fn with_material(self, material: Material) -> Self {
        Self { material, ..self }
    }

    pub fn with_thickness(self, thickness: f32) -> Self {
        Self { thickness, ..self }
    }

    pub fn with_mass(self, mass: f32) -> Self {
        Self {
            mass: Some(mass),
            ..self
        }
    }
}

/// A heavy tank: 160 mu of its 200 mu is the armor plate region.
pub fn tank() -> Body {
    Body {
        name: "tank",
        mass: Some(200.0),
        inertia: Some(2000.0),
        radius: Some(4.0),
        material: ARMOR,
        volume: 20.0,
        thickness: 1.0,
        extent: Some(0.2),
    }
}

pub fn ship() -> Body {
    Body {
        name: "ship",
        mass: Some(31.25),
        inertia: Some(200.0),
        radius: Some(3.0),
        material: HULL,
        volume: 6.0,
        thickness: 1.0,
        extent: Some(0.1),
    }
}

pub fn spaceling() -> Body {
    Body {
        name: "spaceling",
        mass: Some(2.0),
        inertia: Some(0.2),
        radius: Some(0.5),
        material: SOFT_TISSUE,
        volume: 0.18,
        thickness: 0.6,
        extent: Some(1.0),
    }
}

pub fn branch() -> Body {
    Body {
        name: "branch",
        mass: Some(5.0),
        inertia: Some(7.0),
        radius: Some(0.5),
        material: WOOD,
        volume: 4.0,
        thickness: 1.0,
        extent: Some(1.0),
    }
}

/// A solid rock disc: the region is all of it, 70 mu at rock density.
pub fn boulder() -> Body {
    Body {
        name: "boulder",
        mass: Some(70.0),
        inertia: Some(315.0),
        radius: Some(3.0),
        material: ROCK,
        volume: 28.0,
        thickness: 1.0,
        extent: Some(6.0),
    }
}

pub fn stone() -> Body {
    Body {
        name: "stone",
        mass: Some(2.0),
        inertia: Some(0.25),
        radius: Some(0.5),
        material: ROCK,
        volume: 0.8,
        thickness: 1.0,
        extent: Some(1.0),
    }
}

/// A solid steel slug: 0.5 mu of steel in a 0.1 wu radius.
pub fn shell() -> Body {
    Body {
        name: "shell",
        mass: Some(0.5),
        inertia: Some(0.0025),
        radius: Some(0.1),
        material: SHELL,
        volume: 0.0625,
        thickness: 2.0,
        extent: Some(0.2),
    }
}

pub fn planet() -> Body {
    Body {
        name: "planet",
        mass: None,
        inertia: None,
        radius: None,
        material: ROCK,
        volume: 1000.0,
        thickness: 1.0,
        extent: None,
    }
}

/// Contact geometry for `a` meeting `b`.
pub fn geometry(a: &Body, b: &Body) -> ContactGeometry {
    let width = match (a.radius, b.radius) {
        (Some(radius_a), radius_b) => patch_width_from_radii(radius_a, radius_b),
        (None, Some(radius_b)) => patch_width_from_radii(radius_b, None),
        (None, None) => Ok(1.0),
    }
    .expect("reference radii");
    ContactGeometry::new(width, a.section(width), b.section(width))
}

/// Head-on impact of `a` into `b` at `speed`, both translating freely.
pub fn head_on(a: &Body, b: &Body, speed: f32) -> Impact {
    Impact {
        closing_speed: speed,
        effective_mass: effective_mass(a.contact(), b.contact()).expect("reference masses"),
    }
}

/// Closing speed at which `a` and `b` first reach the weaker safe limit.
pub fn safe_limit_speed(a: &Body, b: &Body) -> f32 {
    yield_speed(
        &a.region(),
        &b.region(),
        geometry(a, b),
        head_on(a, b, 0.0).effective_mass,
    )
    .expect("reference yield speed")
}

/// Strike and apply the result to both regions.
pub fn strike(a: &mut Region, b: &mut Region, geometry: ContactGeometry, load: Impact) -> Outcome {
    let outcome = impact(a, b, geometry, load).expect("valid reference impact");
    a.absorb(outcome.consumed[0]).expect("finite work");
    b.absorb(outcome.consumed[1]).expect("finite work");
    outcome
}

/// Apply a sustained load and absorb its work.
pub fn press(region: &mut Region, load: SustainedLoad) -> Outcome {
    let outcome = sustained(region, load).expect("valid reference load");
    region.absorb(outcome.consumed[0]).expect("finite work");
    outcome
}

/// Load a connection and absorb its work.
pub fn bend(joint: &mut Connection, load: ConnectionLoad) -> Outcome {
    let outcome = connection(joint, load).expect("valid reference connection load");
    joint.absorb(outcome.consumed[0]).expect("finite work");
    outcome
}

/// Strike persistent `b` with a fresh `a` each time until failure or `limit` hits.
/// Returns the hit count, both regions and the last outcome.
pub fn hits_to_failure(
    a: &Body,
    b: &Body,
    speed: f32,
    limit: usize,
) -> (usize, Region, Region, Outcome) {
    let (mut region_a, mut region_b) = (a.region(), b.region());
    let geometry = geometry(a, b);
    let load = head_on(a, b, speed);
    let mut hits = 0;
    let mut last = Outcome::default();
    while !region_b.failed() && hits < limit {
        region_a = a.region();
        last = strike(&mut region_a, &mut region_b, geometry, load);
        hits += 1;
    }
    (hits, region_a, region_b, last)
}

/// Short number formatting for labels and tables.
pub fn num(value: f32) -> String {
    if value == 0.0 {
        "0".to_string()
    } else if !(1.0e-2..1.0e5).contains(&value.abs()) {
        format!("{value:.3e}")
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.3}")
    }
}

/// Tick count for labels: "1 tick" or "3 ticks".
fn tick_count(count: usize) -> String {
    if count == 1 {
        "1 tick".to_string()
    } else {
        format!("{count} ticks")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterialRecord {
    pub name: &'static str,
    pub identity: String,
    pub density: f32,
    pub stiffness: f32,
    pub strength: f32,
    pub toughness: f32,
    pub damping: f32,
    pub mode: FailureMode,
    pub resilience: f32,
    pub toughness_over_resilience: f32,
}

fn material_record(material: &Material) -> MaterialRecord {
    MaterialRecord {
        name: material.name,
        identity: material.identity_hex(),
        density: material.density,
        stiffness: material.stiffness,
        strength: material.strength,
        toughness: material.toughness,
        damping: material.damping,
        mode: material.mode,
        resilience: material.resilience(),
        toughness_over_resilience: material.toughness / material.resilience(),
    }
}

/// One table row: the ledger of an event and the regions' integrity after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub label: String,
    pub available: f32,
    pub peak_force: f32,
    pub peak_stress: [f32; 2],
    pub consumed: [f32; 2],
    /// Accumulated work across calls, separate from the displayed event ledger.
    pub cumulative_consumed: Option<[f32; 2]>,
    pub integrity: [f32; 2],
    pub failed: [bool; 2],
    pub recoverable: [f32; 2],
    pub dissipated: [f32; 2],
    pub excess: f32,
    pub failure_motion: f32,
    pub failure_dissipated: f32,
    pub restitution: Option<f32>,
    pub closes: bool,
    /// Regions the event loads: 2 for a contact, 1 for a single region, whose
    /// slot `b` is unused.
    pub regions: usize,
}

impl Row {
    fn from_outcome(label: String, outcome: &Outcome, integrity: [f32; 2], regions: usize) -> Self {
        Self {
            label,
            available: outcome.ledger.available,
            peak_force: outcome.ledger.peak_force,
            peak_stress: outcome.ledger.peak_stress,
            consumed: outcome.consumed,
            cumulative_consumed: None,
            integrity,
            failed: outcome.failed,
            recoverable: outcome.ledger.recoverable,
            dissipated: outcome.ledger.dissipated,
            excess: outcome.ledger.excess,
            failure_motion: outcome.ledger.failure_motion,
            failure_dissipated: outcome.ledger.failure_dissipated,
            restitution: (regions == 2)
                .then(|| outcome.ledger.restitution().ok())
                .flatten(),
            closes: outcome.ledger.closes(LEDGER_TOLERANCE) && outcome.ledger.finite(),
            regions,
        }
    }

    fn pair(label: String, outcome: &Outcome, a: &Region, b: &Region) -> Self {
        Self::from_outcome(label, outcome, [a.integrity(), b.integrity()], 2)
    }

    fn single(label: String, outcome: &Outcome, region: &Region) -> Self {
        Self::from_outcome(label, outcome, [region.integrity(), 1.0], 1)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Case {
    pub name: &'static str,
    pub description: &'static str,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub law_version: &'static str,
    pub post_yield_stiffness_fraction: f32,
    pub tick: f32,
    pub gravity: f32,
    pub materials: Vec<MaterialRecord>,
    pub cases: Vec<Case>,
    pub sweeps: Vec<Case>,
}

pub fn report() -> Report {
    Report {
        law_version: LAW_VERSION,
        post_yield_stiffness_fraction: crate::POST_YIELD_STIFFNESS_FRACTION,
        tick: TICK,
        gravity: GRAVITY,
        materials: crate::ALL_REFERENCE.iter().map(material_record).collect(),
        cases: vec![
            safe_bumps(),
            gentle_landings(),
            failure_allocation(),
            progressive_hits(),
            quantity_and_thickness(),
            wood_vs_armor(),
            tissue_vs_wood(),
            equal_strength_tie(),
            severe_armor_failure(),
            glancing(),
            angular(),
            resting_support(),
            beam_connections(),
            partition(),
            fixed_step(),
        ],
        sweeps: vec![
            closing_speed_sweep(),
            effective_mass_sweep(),
            patch_width_sweep(),
            toughness_sweep(),
            strength_ratio_sweep(),
        ],
    }
}

fn one_strike(a: &Body, b: &Body, load: Impact, label: String) -> Row {
    let (mut region_a, mut region_b) = (a.region(), b.region());
    let outcome = strike(&mut region_a, &mut region_b, geometry(a, b), load);
    Row::pair(label, &outcome, &region_a, &region_b)
}

fn safe_bumps() -> Case {
    let pairs = [
        (branch(), tank()),
        (stone(), ship()),
        (spaceling(), branch()),
        (shell(), tank()),
        (ship(), planet()),
    ];
    let rows = pairs
        .iter()
        .map(|(a, b)| {
            let (mut region_a, mut region_b) = (a.region(), b.region());
            let geometry = geometry(a, b);
            let speed = 0.5 * safe_limit_speed(a, b);
            let load = head_on(a, b, speed);
            let mut consumed = [0.0_f32; 2];
            let mut last = Outcome::default();
            for _ in 0..1000 {
                last = strike(&mut region_a, &mut region_b, geometry, load);
                consumed[0] += last.consumed[0];
                consumed[1] += last.consumed[1];
            }
            let mut row = Row::pair(
                format!(
                    "{} vs {}: 1000 bumps at {} wu/s, half the safe-limit speed",
                    a.name,
                    b.name,
                    num(speed)
                ),
                &last,
                &region_a,
                &region_b,
            );
            row.cumulative_consumed = Some(consumed);
            row
        })
        .collect();
    Case {
        name: "safe_bumps",
        description: "Repeated contacts below the safe limit. Every bump is fully recoverable: the summed irreversible work over a thousand bumps is exactly zero and integrity stays at one.",
        rows,
    }
}

fn progressive_hits() -> Case {
    let (a, b) = (shell(), tank());
    let mut region_b = b.region();
    let geometry = geometry(&a, &b);
    let speed = 300.0;
    let load = head_on(&a, &b, speed);
    let mut rows = Vec::new();
    for hit in 1..=40 {
        let mut region_a = a.region();
        let outcome = strike(&mut region_a, &mut region_b, geometry, load);
        rows.push(Row::pair(
            format!("hit {hit}: shell vs tank at {} wu/s", num(speed)),
            &outcome,
            &region_a,
            &region_b,
        ));
        if region_b.failed() {
            break;
        }
    }
    Case {
        name: "progressive_hits",
        description: "Fresh identical shells hit a persistent plate. Both may yield. Damage per hit and strength stay constant until remaining capacity caps the final charge; each projectile has its own finite capacity.",
        rows,
    }
}

fn quantity_and_thickness() -> Case {
    let a = shell();
    let mut rows = Vec::new();
    for volume in [20.0, 40.0] {
        let b = tank().with_volume(volume);
        let (hits, region_a, region_b, last) = hits_to_failure(&a, &b, 300.0, 200);
        rows.push(Row::pair(
            format!(
                "plate volume {} wu^3: fails after {hits} hits at 300 wu/s",
                num(volume)
            ),
            &last,
            &region_a,
            &region_b,
        ));
    }
    let speed = 1.2 * safe_limit_speed(&a, &tank());
    for thickness in [1.0, 2.0] {
        let b = tank().with_thickness(thickness);
        rows.push(one_strike(
            &a,
            &b,
            head_on(&a, &b, speed),
            format!(
                "plate thickness {} wu: one hit at {} wu/s, 1.2x the thin plate's safe-limit speed",
                num(thickness),
                num(speed)
            ),
        ));
    }
    Case {
        name: "quantity_and_thickness",
        description: "Capacity scales with represented volume, so twice the plate takes about twice the hits. Doubling the represented thickness doubles the load-bearing area and halves the stress, so a hit that dents the thin plate stays recoverable on the thick one.",
        rows,
    }
}

fn wood_vs_armor() -> Case {
    let (a, b) = (branch(), tank());
    let rows = [15.0, 40.0, 80.0, 120.0, 240.0]
        .iter()
        .map(|&speed| {
            one_strike(
                &a,
                &b,
                head_on(&a, &b, speed),
                format!("branch vs tank at {} wu/s", num(speed)),
            )
        })
        .collect();
    Case {
        name: "wood_vs_armor",
        description: "A branch at ordinary speeds breaks without damaging armor. At high speed its rising compression resistance also yields the armor. Excess remains unallocated until the host explicitly selects failure motion or destruction dissipation.",
        rows,
    }
}

fn tissue_vs_wood() -> Case {
    let a = stone();
    let rows = [spaceling(), branch()]
        .iter()
        .map(|b| {
            one_strike(
                &a,
                b,
                head_on(&a, b, 10.0),
                format!("stone vs {} at 10 wu/s", b.name),
            )
        })
        .collect();
    Case {
        name: "tissue_vs_wood",
        description: "The same stone at the same speed against soft tissue and against wood. Both are weaker than rock and yield, but tissue's small strength and capacity lose a far larger integrity fraction.",
        rows,
    }
}

fn equal_strength_tie() -> Case {
    let (a, b) = (ship(), ship());
    let rows = [20.0, 60.0]
        .iter()
        .map(|&speed| {
            one_strike(
                &a,
                &b,
                head_on(&a, &b, speed),
                format!("ship vs ship at {} wu/s", num(speed)),
            )
        })
        .collect();
    Case {
        name: "equal_strength_tie",
        description: "Two hulls of the same material: both reach the safe limit together and share the deformation work equally.",
        rows,
    }
}

fn severe_armor_failure() -> Case {
    let (a, b) = (shell(), tank());
    let rows = [600.0, 1800.0]
        .iter()
        .map(|&speed| {
            one_strike(
                &a,
                &b,
                head_on(&a, &b, speed),
                format!("shell vs tank at {} wu/s", num(speed)),
            )
        })
        .collect();
    Case {
        name: "severe_armor_failure",
        description: "A hardened shell with more energy than the plate can absorb. The plate's capacity is exhausted in one hit, the remainder is reported as excess requiring explicit host allocation, the ledger still closes and nothing is infinite.",
        rows,
    }
}

fn glancing() -> Case {
    let (a, b) = (ship(), planet());
    let speed = 20.0;
    let mass = head_on(&a, &b, 0.0).effective_mass;
    let rows = [0.0_f32, 30.0, 60.0, 80.0]
        .iter()
        .map(|&angle| {
            let closing = speed * libm::cosf(angle.to_radians());
            one_strike(
                &a,
                &b,
                Impact {
                    closing_speed: closing,
                    effective_mass: mass,
                },
                format!(
                    "ship into planet at {} wu/s, incidence {} degrees: normal speed {} wu/s",
                    num(speed),
                    num(angle),
                    num(closing)
                ),
            )
        })
        .collect();
    Case {
        name: "glancing",
        description: "Only the normal component of the approach loads the contact. The same speed at a shallower incidence does strictly less work. Tangential loading is outside this model.",
        rows,
    }
}

fn angular() -> Case {
    let (a, b) = (boulder(), tank());
    let speed = 3.0;
    let rows = [0.0, 1.0, 2.0, 3.0]
        .iter()
        .map(|&lever| {
            let striker = ContactBody::rotating(
                a.mass.expect("boulder mass"),
                a.inertia.expect("boulder inertia"),
                lever,
            );
            let mass = effective_mass(striker, b.contact()).expect("reference masses");
            one_strike(
                &a,
                &b,
                Impact {
                    closing_speed: speed,
                    effective_mass: mass,
                },
                format!(
                    "boulder vs tank at {} wu/s, lever arm {} wu: effective mass {} mu",
                    num(speed),
                    num(lever),
                    num(mass)
                ),
            )
        })
        .collect();
    Case {
        name: "angular",
        description: "An off-center hit spends part of the approach on spinning the boulder. The lever arm lowers the contact-effective mass, the available energy and the damage; a central hit is the upper bound.",
        rows,
    }
}

fn resting_support() -> Case {
    let plank = branch();
    let rest = SustainedLoad {
        force: tank().mass.expect("tank mass") * GRAVITY,
        width: 2.0,
        section: plank.section(2.0),
        duration: TICK,
        measured_plastic_displacement: None,
    };
    let mut rows = Vec::new();
    {
        let mut region = plank.region();
        let mut total = 0.0;
        let mut last = Outcome::default();
        for _ in 0..1000 {
            last = press(&mut region, rest);
            total += last.consumed[0];
        }
        let mut row = Row::single(
            format!(
                "tank resting on a wood plank: force {} over {} wu^2 for 1000 ticks",
                num(rest.force),
                num(rest.width * plank.thickness)
            ),
            &last,
            &region,
        );
        row.cumulative_consumed = Some([total, 0.0]);
        rows.push(row);
    }
    for (force, ratio) in [(6300.0, "1.05x"), (9000.0, "1.5x")] {
        let overload = SustainedLoad { force, ..rest };
        let mut region = plank.region();
        let mut ticks = 0;
        let mut last = Outcome::default();
        while !region.failed() && ticks < 100_000 {
            last = press(&mut region, overload);
            ticks += 1;
        }
        rows.push(Row::single(
            format!(
                "press of force {}, {ratio} the safe stress, fallback progression: fails after {} ({} s)",
                num(force),
                tick_count(ticks),
                num(ticks as f32 * TICK)
            ),
            &last,
            &region,
        ));
    }
    {
        let mut region = plank.region();
        let outcome = press(
            &mut region,
            SustainedLoad {
                force: 9000.0,
                measured_plastic_displacement: Some(0.02),
                ..rest
            },
        );
        rows.push(Row::single(
            "the 1.5x press with a measured 0.02 wu plastic displacement in one tick".to_string(),
            &outcome,
            &region,
        ));
    }
    Case {
        name: "resting_support",
        description: "A safe resting load charges nothing however long it rests. An overload yields and charges plateau work each tick until the plank fails in bounded time, sooner for a larger overload; a host-measured plastic displacement replaces the fallback progression.",
        rows,
    }
}

fn beam_connections() -> Case {
    let beam = Beam::new(1.0, 1.0, 2.0);
    let mut joints = [Connection::new(WOOD, beam).expect("reference beam"); 3];
    let arms = [6.0_f32, 4.0, 2.0];
    let tip_load = 100.0;
    let mut last = [Outcome::default(); 3];
    let mut failed_at = [None; 3];
    for tick in 1..=400 {
        for (index, joint) in joints.iter_mut().enumerate() {
            // A failed connection is the host's to handle; stop loading it.
            if failed_at[index].is_some() {
                continue;
            }
            last[index] = bend(
                joint,
                ConnectionLoad {
                    moment: tip_load * arms[index],
                    axial: 0.0,
                    duration: TICK,
                    measured_plastic_rotation: None,
                    measured_plastic_extension: None,
                },
            );
            if joint.failed() {
                failed_at[index] = Some(tick);
            }
        }
    }
    let rows = joints
        .iter()
        .enumerate()
        .map(|(index, joint)| {
            let status = match failed_at[index] {
                Some(tick) => format!("fails after {}", tick_count(tick)),
                None => "intact after 400 ticks".to_string(),
            };
            Row::single(
                format!(
                    "connection {} at {} wu from the tip, moment {}: {status}",
                    index + 1,
                    num(arms[index]),
                    num(tip_load * arms[index])
                ),
                &last[index],
                joint.region(),
            )
        })
        .collect();
    Case {
        name: "beam_connections",
        description: "Three wood connections along a cantilever under one tip load. Only the root connection exceeds its bending strength; it fails after a bounded number of ticks while the other two keep their full integrity and the assembly is not deleted.",
        rows,
    }
}

fn partition() -> Case {
    let (a, b) = (shell(), tank());
    let whole = geometry(&a, &b);
    let load = head_on(&a, &b, 300.0);
    let parts = [ContactGeometry {
        width: whole.width / 4.0,
        ..whole
    }; 4];
    let merged = ContactGeometry::merge(&parts).expect("equivalent parts");
    let mut rows = Vec::new();
    for (label, geometry) in [
        ("one contact", whole),
        ("four equivalent sub-contacts merged", merged),
    ] {
        let (mut region_a, mut region_b) = (a.region(), b.region());
        let outcome = strike(&mut region_a, &mut region_b, geometry, load);
        rows.push(Row::pair(
            format!("{label}: width {} wu", num(geometry.width)),
            &outcome,
            &region_a,
            &region_b,
        ));
    }
    {
        let (mut region_a, mut region_b) = (a.region(), b.region());
        let mut total = [0.0; 2];
        let mut last = Outcome::default();
        for part in parts {
            last = strike(&mut region_a, &mut region_b, part, load);
            total[0] += last.consumed[0];
            total[1] += last.consumed[1];
        }
        let mut row = Row::pair(
            "four sub-contacts each charged with the whole event, which an adapter must not do"
                .to_string(),
            &last,
            &region_a,
            &region_b,
        );
        row.cumulative_consumed = Some(total);
        rows.push(row);
    }
    Case {
        name: "partition",
        description: "Splitting one contact into equivalent colliders must not multiply energy. Merged sub-contacts reproduce the single contact exactly; charging each part with the whole event would overstate the damage several times over.",
        rows,
    }
}

/// Step a constant overload at `rate` until it fails. `tick` applies one tick
/// and returns its outcome, the integrity after it and whether the material
/// has failed. The row shows the tick that ends at `COMMON_TIME`, with the
/// work consumed by then and the integrity then.
fn stepped(rate: f32, load: &str, mut tick: impl FnMut() -> (Outcome, f32, bool)) -> Row {
    let common = (rate * COMMON_TIME).round() as usize;
    let mut row = None;
    let mut consumed = 0.0;
    let mut ticks = 0;
    let mut failed_after = None;
    while (failed_after.is_none() || ticks < common) && ticks < 100_000 {
        let (outcome, integrity, failed) = tick();
        ticks += 1;
        if ticks <= common {
            consumed += outcome.consumed[0];
            row = Some(Row::from_outcome(
                String::new(),
                &outcome,
                [integrity, 1.0],
                1,
            ));
        }
        if failed && failed_after.is_none() {
            failed_after = Some(ticks);
        }
    }
    let ticks = failed_after.expect("the reference overload fails");
    let mut row = row.expect("the common time is at least one tick");
    row.label = format!(
        "{load} at {} Hz: fails after {} ({} s)",
        num(rate),
        tick_count(ticks),
        num(ticks as f32 / rate)
    );
    row.cumulative_consumed = Some([consumed, 0.0]);
    row
}

fn fixed_step() -> Case {
    let plank = branch();
    let beam = Beam::new(1.0, 1.0, 2.0);
    let mut rows = Vec::new();
    for rate in STEP_RATES {
        let load = SustainedLoad {
            force: 6300.0,
            width: 2.0,
            section: plank.section(2.0),
            duration: 1.0 / rate,
            measured_plastic_displacement: None,
        };
        let mut region = plank.region();
        rows.push(stepped(rate, "1.05x press on the plank", || {
            let outcome = press(&mut region, load);
            (outcome, region.integrity(), region.failed())
        }));
    }
    for rate in STEP_RATES {
        let load = ConnectionLoad {
            moment: 540.0,
            axial: 0.0,
            duration: 1.0 / rate,
            measured_plastic_rotation: None,
            measured_plastic_extension: None,
        };
        let mut joint = Connection::new(WOOD, beam).expect("reference beam");
        rows.push(stepped(rate, "1.08x moment on a connection", || {
            let outcome = bend(&mut joint, load);
            (outcome, joint.integrity(), joint.failed())
        }));
    }
    Case {
        name: "fixed_step",
        description: "The plank press and a connection moment just past their safe limits, stepped at 30, 60, 120 and 240 Hz, where 240 Hz is the 60 Hz tick with four substeps. Constant-strength fallback work integrates linearly. Cumulative work and integrity after 1/30 s match at every rate; failure lands within one tick (plus f32 rounding) of the analytic time. Event columns describe only the tick ending at 1/30 s; cumulative work is explicitly separate.",
        rows,
    }
}

fn sweep(
    name: &'static str,
    description: &'static str,
    values: &[f32],
    row: impl Fn(f32) -> Row,
) -> Case {
    Case {
        name,
        description,
        rows: values.iter().map(|&value| row(value)).collect(),
    }
}

fn closing_speed_sweep() -> Case {
    let (a, b) = (shell(), tank());
    sweep(
        "closing_speed",
        "Hardened shell into a fresh plate at increasing closing speed.",
        &[
            50.0, 100.0, 150.0, 200.0, 300.0, 450.0, 600.0, 900.0, 1200.0,
        ],
        |speed| {
            one_strike(
                &a,
                &b,
                head_on(&a, &b, speed),
                format!("{} wu/s", num(speed)),
            )
        },
    )
}

fn effective_mass_sweep() -> Case {
    let b = tank();
    sweep(
        "effective_mass",
        "Shell mass at a fixed 300 wu/s into a fresh plate; the tank's 200 mu bounds the effective mass.",
        &[0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0],
        |mass| {
            let a = shell().with_mass(mass);
            let load = head_on(&a, &b, 300.0);
            one_strike(
                &a,
                &b,
                load,
                format!(
                    "shell mass {} mu: effective mass {} mu",
                    num(mass),
                    num(load.effective_mass)
                ),
            )
        },
    )
}

fn patch_width_sweep() -> Case {
    let (a, b) = (shell(), tank());
    let load = head_on(&a, &b, 300.0);
    let default = geometry(&a, &b).width;
    sweep(
        "patch_width",
        "Shell into a fresh plate at 300 wu/s with the patch width scaled around its default and the depth that follows from it. A wider patch changes compliance and peak stress, which may exceed either material's yield strength.",
        &[0.25, 0.5, 1.0, 2.0, 4.0],
        |factor| {
            let width = factor * default;
            let (mut region_a, mut region_b) = (a.region(), b.region());
            let outcome = strike(
                &mut region_a,
                &mut region_b,
                ContactGeometry::new(width, a.section(width), b.section(width)),
                load,
            );
            Row::pair(
                format!(
                    "patch width {} wu, {}x the default",
                    num(width),
                    num(factor)
                ),
                &outcome,
                &region_a,
                &region_b,
            )
        },
    )
}

fn toughness_sweep() -> Case {
    let a = shell();
    sweep(
        "toughness",
        "Plate toughness scaled around the reference value: hits to failure at 300 wu/s.",
        &[0.25, 0.5, 1.0, 2.0, 4.0],
        |factor| {
            let b = tank().with_material(Material {
                toughness: ARMOR.toughness * factor,
                ..ARMOR
            });
            let (hits, region_a, region_b, last) = hits_to_failure(&a, &b, 300.0, 400);
            Row::pair(
                format!("toughness x{}: fails after {hits} hits", num(factor)),
                &last,
                &region_a,
                &region_b,
            )
        },
    )
}

fn strength_ratio_sweep() -> Case {
    let b = tank();
    sweep(
        "strength_ratio",
        "Striker strength relative to the plate at 300 wu/s: both can yield, and work allocation changes continuously as their strengths cross. Identical geometry and material properties share equally.",
        &[0.5, 0.75, 0.999, 1.0, 1.001, 1.25, 1.5, 2.0],
        |ratio| {
            let a = shell().with_material(Material {
                strength: ARMOR.strength * ratio,
                ..SHELL
            });
            one_strike(
                &a,
                &b,
                head_on(&a, &b, 300.0),
                format!("striker strength {}x plate strength", num(ratio)),
            )
        },
    )
}

fn failed_label(failed: [bool; 2]) -> &'static str {
    match failed {
        [true, true] => "a, b",
        [true, false] => "a",
        [false, true] => "b",
        [false, false] => "-",
    }
}

/// Render the report as Markdown tables for the design document.
pub fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<!-- Generated by `cargo run -p engine-material --example reference_cases`; do not edit. -->\n"
    );
    let _ = writeln!(
        out,
        "Law `{}`. Post-yield stiffness fraction {}. Tick {} s, gravity {} wu/s^2.\n",
        report.law_version,
        num(report.post_yield_stiffness_fraction),
        num(report.tick),
        num(report.gravity)
    );
    let _ = writeln!(out, "## Reference materials\n");
    let _ = writeln!(
        out,
        "| material | mode | density | stiffness | strength | toughness | toughness / resilience | damping | identity |"
    );
    let _ = writeln!(
        out,
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |"
    );
    for material in &report.materials {
        let _ = writeln!(
            out,
            "| {} | {:?} | {} | {} | {} | {} | {} | {} | `{}` |",
            material.name,
            material.mode,
            num(material.density),
            num(material.stiffness),
            num(material.strength),
            num(material.toughness),
            num(material.toughness_over_resilience),
            num(material.damping),
            material.identity
        );
    }
    for (heading, cases) in [
        ("## Acceptance cases", &report.cases),
        ("## Parameter sweeps", &report.sweeps),
    ] {
        let _ = writeln!(out, "\n{heading}");
        for case in cases {
            let _ = writeln!(out, "\n### {}\n\n{}\n", case.name, case.description);
            let _ = writeln!(
                out,
                "| case | available | peak force | stress a | stress b | consumed a | consumed b | integrity a | integrity b | failed | excess | failure motion | failure dissipation | rebound | cumulative work a / b | closes |"
            );
            let _ = writeln!(
                out,
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | --- | --- |"
            );
            for row in &case.rows {
                // A single-region row has no side b.
                let b = |value: f32| {
                    if row.regions == 2 {
                        num(value)
                    } else {
                        "-".to_string()
                    }
                };
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                    row.label,
                    num(row.available),
                    num(row.peak_force),
                    num(row.peak_stress[0]),
                    b(row.peak_stress[1]),
                    num(row.consumed[0]),
                    b(row.consumed[1]),
                    num(row.integrity[0]),
                    b(row.integrity[1]),
                    failed_label(row.failed),
                    num(row.excess),
                    num(row.failure_motion),
                    num(row.failure_dissipated),
                    row.restitution.map(num).unwrap_or_else(|| "-".into()),
                    row.cumulative_consumed
                        .map(|work| format!("{} / {}", num(work[0]), b(work[1])))
                        .unwrap_or_else(|| "-".into()),
                    if row.closes { "yes" } else { "no" }
                );
            }
        }
    }
    out
}

/// Candidate from the earlier playtest, now fully damping the hull's elastic
/// share. This is reference evidence, not a live scenario configuration.
pub fn landing_ship() -> Body {
    ship().with_material(Material {
        stiffness: 3.5e4,
        toughness: 9.6e4,
        damping: 1.0,
        ..HULL
    })
}

fn gentle_landings() -> Case {
    let (a, b) = (landing_ship(), planet());
    let rows = [1.0, 1.6, 2.2, 13.3, 43.2, 100.0]
        .into_iter()
        .map(|speed| {
            one_strike(
                &a,
                &b,
                head_on(&a, &b, speed),
                format!("candidate hull at {} wu/s", num(speed)),
            )
        })
        .collect();
    Case {
        name: "gentle_landings",
        description: "Provisional playtest hull (stiffness 35000, strength 6000, toughness 96000, damping 1) against reference rock (damping 0.95). Gentle contacts cost exactly zero; harder impacts accumulate damage. Rebound is a target for the solver, not a completed gameplay playtest.",
        rows,
    }
}

fn failure_allocation() -> Case {
    let (a, b) = (branch(), tank());
    let mut rows = Vec::new();
    for fraction in [0.0, 0.5, 1.0] {
        let (mut ra, mut rb) = (a.region(), b.region());
        let mut hit = strike(&mut ra, &mut rb, geometry(&a, &b), head_on(&a, &b, 120.0));
        hit.ledger
            .resolve_excess(hit.ledger.excess * fraction)
            .unwrap();
        rows.push(Row::pair(
            format!("{}% of excess assigned to motion", num(100.0 * fraction)),
            &hit,
            &ra,
            &rb,
        ));
    }
    Case {
        name: "failure_allocation",
        description: "Explicit allocations of one failed branch impact: carry none, half or all of the excess, dissipating the rest. These are alternative host policies; the carry cases only prove accounting, not fragment velocities. The real-solver fixture tests the fully dissipative blocking policy.",
        rows,
    }
}
