use super::*;
use crate::{
    Allegiance, DebrisKind,
    surface_sortie::surface_forces::{SURFACE_DEPARTURE_DISTANCE, SurfaceForceId},
    tanks::{TANK_HEALTH, TankCommand, TankDamage, TankDamageCause, TankId},
};
use engine_common::{RenderLine, RenderPoint};
use engine_rapier::{rover::RoverSpawnPose, world::RayCastOptions};
use engine_terrain::{Brush, CellCoord, EditMode, TerrainEdit};

const GOVERNMENT: Allegiance = Allegiance::INITIAL_GOVERNMENT;
const FIRST: Allegiance = Allegiance::Player(P1);
const SECOND: Allegiance = Allegiance::Player(P2);
const FLYING: SurfaceSortieAction = SurfaceSortieAction {
    primary_held: true,
    ..NEUTRAL
};

/// A material planet with `players` seats: the first pilot aboard its ship at
/// world +Y and the second seat's ship, if any, landed at -Y. The planet's spin
/// is off, so a parked tank stays where it was put.
fn garrison(players: usize) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material(42, players);
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 60);
    state
}

/// Places a tank on the material under `up` and completes the step that the
/// next placement needs.
fn place(state: &mut SurfaceSortieState, up: Vec2, allegiance: Allegiance) -> TankId {
    let planet = state.world.planets[0];
    let tank = state
        .world
        .place_tank(
            planet.position + up * (planet.radius + 6.0),
            -up,
            allegiance,
        )
        .expect("clear material under the tank");
    idle(state, 1);
    tank
}

/// Inserts a tank upside down on the material under `up`, lying on its chassis
/// with its wheels outward.
fn overturned(state: &mut SurfaceSortieState, up: Vec2, allegiance: Allegiance) -> TankId {
    let center = state.world.planets[0].position;
    let surface = state
        .world
        .physics
        .world
        .cast_ray(
            center + up * 80.0,
            -up,
            RayCastOptions {
                max_distance: 40.0,
                collision_groups: physics::tank_material_groups(),
                ..Default::default()
            },
        )
        .expect("material under the tank")
        .point;
    let spec = physics::tank_spec();
    // The tank's own up points into the planet, so its wheels sit beyond its
    // chassis.
    let chassis = surface + up * (spec.chassis_half_height + 0.08);
    let wheel_center = chassis + up * (spec.suspension_rest_length - spec.suspension_anchor_height);
    let tank = state
        .world
        .physics
        .tanks
        .insert(
            &mut state.world.physics.world,
            RoverSpawnPose {
                wheel_center,
                up_angle: (-up).y.atan2((-up).x),
            },
            spec,
            allegiance,
        )
        .expect("room for the tank");
    state.world.physics.material_queries_dirty = true;
    tank
}

/// Sets the whole tank moving straight out from the planet at `speed`.
fn launch(state: &mut SurfaceSortieState, tank: TankId, speed: f32) {
    let index = state.world.tank_ids().position(|id| id == tank).unwrap();
    let chassis = state.world.tank_snapshot(tank).unwrap().assembly.chassis;
    let out = (chassis.position - state.world.planets[0].position).normalized();
    for body in &state.world.physics.tanks.bodies()[index * 3..index * 3 + 3] {
        assert!(
            state
                .world
                .physics
                .world
                .set_velocity(*body, out * speed, 0.0, true)
        );
    }
}

fn ground_units(state: &SurfaceSortieState, player: usize) -> u16 {
    state
        .claim_observation(0, player)
        .unwrap()
        .opposition
        .ground_units
}

/// What the tank's wheels rest on, if anything.
fn wheels_on(state: &SurfaceSortieState, tank: TankId) -> Option<ColliderId> {
    state
        .world
        .tank_snapshot(tank)
        .unwrap()
        .support
        .map(|s| s.collider)
}

fn on_planet(state: &SurfaceSortieState, tank: TankId) -> bool {
    wheels_on(state, tank).and_then(physics::planet_surface_support_index) == Some(0)
}

/// Where the tank's chassis is in the planet's moving frame.
fn local(state: &SurfaceSortieState, tank: TankId) -> Vec2 {
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let chassis = state.world.tank_snapshot(tank).unwrap().assembly.chassis;
    (chassis.position - frame.position).rotate_radians(-frame.angle)
}

/// Steps a world and its clone together and checks that they agree.
fn idle_both(state: &mut SurfaceSortieState, replay: &mut SurfaceSortieState) {
    idle(state, 1);
    idle(replay, 1);
    assert_eq!(
        state.surface_force_observations(),
        replay.surface_force_observations()
    );
    assert_eq!(
        SurfaceSortieScenario::observe(state).payload,
        SurfaceSortieScenario::observe(replay).payload
    );
}

/// Steps a world and its clone together until the tank has left the planet
/// and landed again. Returns, for each tick it was off the ground, how many
/// tanks opposed the first seat and where its chassis was in the planet's
/// frame.
fn flight(
    state: &mut SurfaceSortieState,
    replay: &mut SurfaceSortieState,
    tank: TankId,
) -> Vec<(u16, Vec2)> {
    let mut ticks = Vec::new();
    for _ in 0..600 {
        idle_both(state, replay);
        if !on_planet(state, tank) {
            ticks.push((ground_units(state, 0), local(state, tank)));
        } else if !ticks.is_empty() {
            return ticks;
        }
    }
    panic!("the tank must leave the ground and land: {ticks:?}");
}

fn tanks(state: &SurfaceSortieState) -> Vec<(TankId, Allegiance)> {
    state
        .world
        .tank_ids()
        .map(|id| (id, state.world.tank_snapshot(id).unwrap().allegiance))
        .collect()
}

#[test]
fn tank_allegiance_is_claimant_relative_readable_and_adds_no_seat() {
    let mut state = garrison(2);
    // The players' tanks share a block of the surface across the planet.
    let radius = state.world.planets[0].radius;
    let government = place(&mut state, -Vec2::X, GOVERNMENT);
    let first = place(&mut state, Vec2::X.rotate_radians(-3.2 / radius), FIRST);
    let second = place(&mut state, Vec2::X.rotate_radians(3.2 / radius), SECOND);
    idle(&mut state, 60);
    // Each seat is opposed by every tank but its own side's.
    assert_eq!(ground_units(&state, 0), 2);
    assert_eq!(ground_units(&state, 1), 2);
    let ground: Vec<_> = state
        .surface_force_observations()
        .into_iter()
        .filter(|f| matches!(f.id, SurfaceForceId::Ground(_)))
        .map(|f| (f.allegiance, f.planet))
        .collect();
    assert_eq!(ground, [(GOVERNMENT, 0), (FIRST, 0), (SECOND, 0)]);
    assert_eq!(
        tanks(&state),
        [(government, GOVERNMENT), (first, FIRST), (second, SECOND)]
    );
    // A government is neither seat, and reads as itself.
    assert_eq!(
        serde_json::to_value([GOVERNMENT, FIRST]).unwrap(),
        serde_json::json!([{ "government": 0 }, { "player": "player_1" }])
    );
    // Each tank is drawn in its side's color, and the government's is neither
    // player's.
    let colors: Vec<_> = state
        .world
        .players
        .iter()
        .map(|p| render_color(p.color))
        .collect();
    let olive = render_color(GOVERNMENT_TANK_COLOR);
    assert!(!colors.contains(&olive));
    let frame = SurfaceSortieScenario::player_frame(&state, 1);
    let fills: Vec<_> = frame
        .layers
        .iter()
        .filter(|layer| layer.z == ROVER_LAYER)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Polygon(chassis) => chassis.fill.map(|fill| fill.color),
            _ => None,
        })
        .collect();
    assert_eq!(fills, [olive, colors[0], colors[1]]);
}

/// Each tank's health bar floats above it against its gravity, even with the
/// tank on its back, and the bar's fill shows the health the tank has left.
#[test]
fn a_tank_health_bar_shows_what_is_left_above_it_even_on_its_back() {
    let mut state = garrison(1);
    let upright = place(&mut state, Vec2::X, GOVERNMENT);
    let flipped = overturned(&mut state, -Vec2::Y, GOVERNMENT);
    idle(&mut state, 60);
    let hit = TankDamage {
        tick: state.world.tick,
        cause: TankDamageCause::Laser,
        amount: 25.0,
        source: Some(FIRST),
    };
    state.world.physics.tanks.damage(upright, hit);
    let frame = SurfaceSortieScenario::player_frame(&state, 0);
    let lines: Vec<&RenderLine> = frame
        .layers
        .iter()
        .filter(|layer| layer.z == ROVER_LAYER + 1)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Line(line) => Some(line),
            _ => None,
        })
        .collect();
    let point = |p: RenderPoint| Vec2::new(p.x, p.y);
    let length = |line: &RenderLine| point(line.start).distance_to(point(line.end));
    // A bar is a track and a fill drawn from the same end: the middle of the
    // longer, and how much of it the shorter fills.
    let bars: Vec<(Vec2, f32)> = lines
        .iter()
        .enumerate()
        .flat_map(|(at, a)| lines[at + 1..].iter().map(move |b| (a, b)))
        .filter(|(a, b)| a.start == b.start)
        .map(|(a, b)| {
            let (track, fill) = if length(a) >= length(b) {
                (a, b)
            } else {
                (b, a)
            };
            let middle = (point(track.start) + point(track.end)) * 0.5;
            (middle, length(fill) / length(track))
        })
        .collect();
    assert_eq!(bars.len(), 2, "{bars:?}");
    let center = state.world.planets[0].position;
    for (tank, left) in [(upright, 0.75), (flipped, 1.0)] {
        let chassis = state.world.tank_snapshot(tank).unwrap().assembly.chassis;
        let (middle, filled) = bars
            .iter()
            .copied()
            .min_by(|a, b| {
                let a = a.0.distance_to(chassis.position);
                a.total_cmp(&b.0.distance_to(chassis.position))
            })
            .unwrap();
        assert!((filled - left).abs() < 1.0e-5, "{tank:?}: {filled}");
        let above = middle.distance_to(center) - chassis.position.distance_to(center);
        assert!(above > 2.0, "{tank:?}: {above}");
    }
}

#[test]
fn clearing_the_last_hostile_tank_lets_the_claim_finish_beside_a_flying_ship() {
    let mut state = garrison(2);
    let east = place(&mut state, Vec2::X, GOVERNMENT);
    place(&mut state, -Vec2::X, GOVERNMENT);
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 60);
    assert_eq!(
        state.claim_observation(0, 0).unwrap().opposition,
        SurfaceOpposition {
            ships: 1,
            ground_units: 2,
            ..Default::default()
        }
    );
    // The second seat's ship takes off and stays under thrust. A flying ship
    // never blocks, but both tanks still do.
    step(&mut state, [NEUTRAL, FLYING]);
    assert_eq!(
        state.claim_observation(0, 0).unwrap().opposition,
        SurfaceOpposition {
            ground_units: 2,
            ..Default::default()
        }
    );
    assert!(state.world.remove_tank(east));
    step(&mut state, [NEUTRAL, FLYING]);
    let claim = state.claim_observation(0, 0).unwrap();
    assert_eq!(claim.status, PlanetClaimStatus::OpposingSurfaceForces);
    assert_eq!(claim.opposition.ground_units, 1);
    assert_eq!(claim.progress, 0.0);
    assert!(claim.flag.is_none());
    // Clearing the last tank leaves no stale blocker, and the claim takes its
    // ordinary course from there.
    state.world.clear_tanks();
    let mut ticks = 0;
    while state.world.planets[0].owner_id.is_none() {
        step(&mut state, [NEUTRAL, FLYING]);
        ticks += 1;
        assert!(state.claim_observation(0, 0).unwrap().opposition.is_empty());
        assert!(ticks <= 240, "{:?}", state.claim_observation(0, 0));
    }
    assert_eq!(ticks as u64, FLAG_STAGE_TIME.as_secs() * 60);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    assert!(
        state
            .surface_force_observations()
            .iter()
            .all(|f| !matches!(f.id, SurfaceForceId::Ground(_)))
    );
}

/// A claim that its last defender holds one tick short of done completes on the
/// tick that the defender dies. The defender leaves the gate in that step, with
/// no contact grace, and its wreck never counts while the ground wears it away.
#[test]
fn a_claim_completes_on_the_tick_its_last_defender_dies() {
    let mut state = garrison(1);
    // Where a tank comes to rest on the far side.
    let settled = place(&mut state, -Vec2::Y, GOVERNMENT);
    idle(&mut state, 120);
    let poses: Vec<_> = state
        .world
        .physics
        .tanks
        .bodies()
        .into_iter()
        .map(|body| state.world.physics.world.motion(body).unwrap())
        .collect();
    assert!(state.world.remove_tank(settled));
    // The claim rises unopposed until one tick is left.
    let stage = FLAG_STAGE_TIME.as_secs_f32() * 60.0;
    let held = (stage - 1.0) / stage;
    step(&mut state, [INTERACT, NEUTRAL]);
    let mut ticks = 0;
    while state.claim_observation(0, 0).unwrap().progress < held - 1.0e-4 {
        idle(&mut state, 1);
        ticks += 1;
        assert!(ticks <= 600, "{:?}", state.claim_observation(0, 0));
    }
    // A defender appears just where the first tank rested and holds the claim
    // there.
    let defender = state
        .world
        .physics
        .tanks
        .insert(
            &mut state.world.physics.world,
            RoverSpawnPose {
                wheel_center: (poses[1].position + poses[2].position) * 0.5,
                up_angle: poses[0].angle + core::f32::consts::FRAC_PI_2,
            },
            physics::tank_spec(),
            GOVERNMENT,
        )
        .expect("room for the tank");
    for (body, pose) in state.world.physics.tanks.bodies().into_iter().zip(&poses) {
        let world = &mut state.world.physics.world;
        assert!(world.set_pose(body, pose.position, pose.angle, true));
        assert!(world.set_velocity(body, Vec2::ZERO, 0.0, true));
    }
    state.world.physics.material_queries_dirty = true;
    for _ in 0..60 {
        idle(&mut state, 1);
        let claim = state.claim_observation(0, 0).unwrap();
        assert_eq!(claim.status, PlanetClaimStatus::OpposingSurfaceForces);
        assert_eq!(claim.opposition.ground_units, 1);
        assert!((claim.progress - held).abs() < 1.0e-4, "{claim:?}");
    }
    assert_eq!(state.world.planets[0].owner_id, None);
    // Its armor fails outright between steps, and the claim completes in the
    // step that destroys it.
    let hit = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: TANK_HEALTH,
        source: Some(FIRST),
    };
    state.world.physics.tanks.damage(defender, hit);
    idle(&mut state, 1);
    assert!(state.world.tank_snapshot(defender).is_none());
    assert_eq!(state.world.tank_losses.len(), 1);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    let claim = state.claim_observation(0, 0).unwrap();
    assert_eq!(claim.status, PlanetClaimStatus::Secured);
    assert!(claim.opposition.is_empty());
    // Its wreck falls where it stood, and the ground wears it away as it does
    // any debris. It never counts.
    let wreck = |state: &SurfaceSortieState| {
        let debris = state.world.debris.iter();
        debris
            .filter(|debris| debris.kind == DebrisKind::Fragment)
            .map(|fragment| fragment.position)
            .collect::<Vec<_>>()
    };
    let fragments = wreck(&state);
    assert_eq!(fragments.len(), 6);
    for fragment in fragments {
        assert!(fragment.distance_to(poses[0].position) < 4.0);
    }
    let mut left = 6;
    let mut ticks = 0;
    while left > 0 {
        idle(&mut state, 1);
        ticks += 1;
        assert!(ticks <= 600, "{:?}", wreck(&state));
        assert!(state.claim_observation(0, 0).unwrap().opposition.is_empty());
        // Its wheels' pieces start overlapping its chassis's a little, as a
        // rover's do, and its first steps part them gently.
        if ticks <= 10 {
            let debris = state.world.debris.iter();
            for fragment in debris.filter(|debris| debris.kind == DebrisKind::Fragment) {
                assert!(fragment.velocity.length() < 2.5, "{:?}", fragment.velocity);
            }
        }
        let now = wreck(&state).len();
        assert!(now <= left);
        left = now;
    }
    assert!(
        state
            .surface_force_observations()
            .iter()
            .all(|f| !matches!(f.id, SurfaceForceId::Ground(_)))
    );
    assert_eq!(state.world.planets[0].owner_id, Some(0));
}

#[test]
fn an_overturned_tank_on_the_far_side_still_blocks_capture() {
    let mut state = garrison(1);
    let tank = overturned(&mut state, -Vec2::Y, GOVERNMENT);
    idle(&mut state, 60);
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 300);
    let claim = state.claim_observation(0, 0).unwrap();
    assert_eq!(claim.status, PlanetClaimStatus::OpposingSurfaceForces);
    assert_eq!(
        claim.opposition,
        SurfaceOpposition {
            ground_units: 1,
            ..Default::default()
        }
    );
    assert!(claim.flag.is_none());
    assert_eq!(state.world.planets[0].owner_id, None);
    // It lies on its chassis, wheels outward and off the ground.
    let tank = state.world.tank_snapshot(tank).unwrap();
    let center = state.world.planets[0].position;
    let height = |p: Vec2| p.distance_to(center);
    for wheel in tank.assembly.wheels {
        assert!(height(wheel.position) > height(tank.assembly.chassis.position));
    }
}

#[test]
fn a_tank_hop_keeps_its_block_a_departure_ends_it_and_a_clone_replays_both() {
    let mut state = garrison(1);
    // A second hostile tank keeps the claim blocked throughout, so only the
    // count changes.
    place(&mut state, Vec2::X, GOVERNMENT);
    let tank = place(&mut state, -Vec2::Y, GOVERNMENT);
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 60);
    assert_eq!(ground_units(&state, 0), 2);
    let mut replay = state.clone();
    // A hop leaves the ground and lands again inside the contact grace and the
    // departure distance.
    launch(&mut state, tank, 4.0);
    launch(&mut replay, tank, 4.0);
    let hop = flight(&mut state, &mut replay, tank);
    assert!(
        hop.len() > 5 && hop.iter().all(|&(n, _)| n == 2),
        "a hop keeps the block: {hop:?}"
    );
    for _ in 0..60 {
        idle_both(&mut state, &mut replay);
    }
    // A departure carries it beyond the departure distance well inside the
    // contact grace, and it counts again only once it lands. The distance runs
    // from where it last touched the planet, a tick or two into the climb as
    // its suspension lets its wheels go.
    let lift_off = local(&state, tank);
    launch(&mut state, tank, 20.0);
    launch(&mut replay, tank, 20.0);
    let departure = flight(&mut state, &mut replay, tank);
    let departed = departure
        .iter()
        .position(|&(n, _)| n == 1)
        .expect("a departure ends the block");
    let out = departure[departed].1.distance_to(lift_off);
    assert!(
        departed < 45 && out > SURFACE_DEPARTURE_DISTANCE && out < SURFACE_DEPARTURE_DISTANCE + 1.0,
        "the distance, not the grace, ended it at tick {departed}, {out} out"
    );
    assert!(
        departure[departed..].iter().all(|&(n, _)| n == 1),
        "{departure:?}"
    );
    assert_eq!(ground_units(&state, 0), 2, "a landing restores the block");
    assert_eq!(state.world.planets[0].owner_id, None);
    assert_eq!(
        state.world.physics.world.snapshot_bytes().unwrap(),
        replay.world.physics.world.snapshot_bytes().unwrap()
    );
}

#[test]
fn a_tank_carrying_another_keeps_its_block_through_a_hop() {
    let mut state = garrison(1);
    let below = place(&mut state, -Vec2::Y, GOVERNMENT);
    idle(&mut state, 60);
    // A second tank rides on the first's back, its wheels on the chassis.
    let chassis = state.world.tank_snapshot(below).unwrap().assembly.chassis;
    let up = Vec2::Y.rotate_radians(chassis.angle);
    let spec = physics::tank_spec();
    let above = state
        .world
        .physics
        .tanks
        .insert(
            &mut state.world.physics.world,
            RoverSpawnPose {
                wheel_center: chassis.position
                    + up * (spec.chassis_half_height + spec.wheel_radius + 0.05),
                up_angle: up.y.atan2(up.x),
            },
            spec,
            GOVERNMENT,
        )
        .expect("room for the tank");
    state.world.physics.material_queries_dirty = true;
    let brake = TankCommand {
        brake: true,
        ..TankCommand::default()
    };
    assert!(state.world.command_tank(above, brake));
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 60);
    // The rider rests on something other than the planet, which ends its
    // membership, so only the tank beneath it counts.
    assert!(wheels_on(&state, above).is_some() && !on_planet(&state, above));
    assert_eq!(ground_units(&state, 0), 1);
    // Both leave the planet together, the rider still bearing down on the
    // tank beneath. A touch from above is not support, so the lower tank keeps
    // its grace through the hop.
    let mut replay = state.clone();
    for tank in [below, above] {
        launch(&mut state, tank, 4.0);
        launch(&mut replay, tank, 4.0);
    }
    let hop = flight(&mut state, &mut replay, below);
    assert!(hop.len() > 5 && hop.iter().all(|&(n, _)| n == 1), "{hop:?}");
}

#[test]
fn a_tank_on_detached_terrain_stops_counting_and_the_capture_leaves_it_intact() {
    let mut state = garrison(1);
    let tank = place(&mut state, -Vec2::Y, GOVERNMENT);
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 60);
    assert_eq!(ground_units(&state, 0), 1);
    // It has taken a hit from the first seat's laser.
    let laser = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Laser,
        amount: 25.0,
        source: Some(FIRST),
    };
    state.world.physics.tanks.damage(tank, laser);
    // Cut a column through the planet a few cells beneath the tank, so the cap
    // it rests on breaks away as a fragment.
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let chassis = state
        .world
        .tank_snapshot(tank)
        .unwrap()
        .assembly
        .chassis
        .position;
    let local = (chassis - frame.position).rotate_radians(-frame.angle);
    assert!(local.x < -50.0, "the far side is local -X: {local:?}");
    let field = &state.world.terrain.planets[&0].field;
    let column = field.local_to_cell(local + Vec2::X * 6.0).unwrap().x;
    let top = field.height() as i32 - 1;
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Capsule {
                    start: CellCoord::new(column, 0),
                    end: CellCoord::new(column, top),
                    radius: 0,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    let mut detached = 0;
    let mut ticks = 0;
    while state.world.planets[0].owner_id.is_none() {
        idle(&mut state, 1);
        ticks += 1;
        assert!(ticks <= 300, "{:?}", state.claim_observation(0, 0));
        let fragment = wheels_on(&state, tank).is_some_and(|c| {
            state
                .world
                .terrain
                .fragments
                .contains_key(&c.entity.value())
        });
        if fragment {
            detached += 1;
            assert_eq!(ground_units(&state, 0), 0, "detached tick {ticks}");
        }
    }
    // It stopped counting the step its cap broke away, with no grace, and
    // rode the fragment through the whole claim.
    assert_eq!(ticks as u64, FLAG_STAGE_TIME.as_secs() * 60);
    assert_eq!(detached, ticks);
    // Capture neither removes, converts nor repairs it.
    assert_eq!(tanks(&state), [(tank, GOVERNMENT)]);
    let snapshot = state.world.tank_snapshot(tank).unwrap();
    assert_eq!(
        (snapshot.health, snapshot.last_damage),
        (TANK_HEALTH - 25.0, Some(laser))
    );
}

#[test]
fn losing_the_flag_footing_keeps_every_tank_with_its_allegiance_and_damage() {
    let mut state = garrison(1);
    let friendly = place(&mut state, -Vec2::X, FIRST);
    // The seat's own tank has taken a round that no seat fired.
    let round = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: 40.0,
        source: None,
    };
    state.world.physics.tanks.damage(friendly, round);
    let damage = |state: &SurfaceSortieState, tank: TankId| {
        let tank = state.world.tank_snapshot(tank).unwrap();
        (tank.health, tank.last_damage)
    };
    let round_dealt = (TANK_HEALTH - 40.0, Some(round));
    step(&mut state, [INTERACT, NEUTRAL]);
    idle(&mut state, 240);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "a friendly tank never blocks"
    );
    assert_eq!(
        damage(&state, friendly),
        round_dealt,
        "capture repairs nothing"
    );
    let hostile = place(&mut state, Vec2::X, GOVERNMENT);
    idle(&mut state, 60);
    assert_eq!(
        state.world.planets[0].owner_id,
        Some(0),
        "arrival does not neutralize ownership"
    );
    assert_eq!(ground_units(&state, 0), 1);
    // The defender has taken a hit from the first seat's laser.
    let laser = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Laser,
        amount: 25.0,
        source: Some(FIRST),
    };
    state.world.physics.tanks.damage(hostile, laser);
    let laser_dealt = (TANK_HEALTH - 25.0, Some(laser));
    let footing = state.claims[0].flag.unwrap().anchor.footing.unwrap();
    state
        .world
        .queue_planet_edit(
            0,
            TerrainEdit {
                brush: Brush::Circle {
                    center: footing,
                    radius: 1,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    for tick in 0..240 {
        idle(&mut state, 1);
        assert_eq!(state.world.planets[0].owner_id, None, "tick {tick}");
        assert_eq!(ground_units(&state, 0), 1, "tick {tick}");
    }
    assert_eq!(state.claims[0].neutralizations, 1);
    assert_eq!(tanks(&state), [(friendly, FIRST), (hostile, GOVERNMENT)]);
    assert_eq!(damage(&state, friendly), round_dealt);
    assert_eq!(
        damage(&state, hostile),
        laser_dealt,
        "the planet's loss repairs neither"
    );
}

/// The client restarts a sortie by building it again from its seed, and a new
/// match builds one from a fresh seed, so a world must keep nothing of an
/// earlier world's tanks. One built after another's tanks fought and failed
/// holds no tank, wreck or registration. It steps just as one built before any
/// play does: its first tank takes the same identity, and the two match
/// through the claim that tank blocks.
#[test]
fn a_rebuilt_world_holds_nothing_of_an_earlier_worlds_tanks() {
    let mut pristine = garrison(1);
    // A defender blocks the first seat's claim, with the seat's own tank on the
    // planet too, until the defender's armor fails and leaves its wreck.
    let mut played = garrison(1);
    let friendly = place(&mut played, -Vec2::X, FIRST);
    let defender = place(&mut played, Vec2::X, GOVERNMENT);
    step(&mut played, [INTERACT, NEUTRAL]);
    idle(&mut played, 60);
    assert_eq!(ground_units(&played, 0), 1);
    let round = TankDamage {
        tick: played.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: TANK_HEALTH,
        source: Some(FIRST),
    };
    played.world.physics.tanks.damage(defender, round);
    idle(&mut played, 1);
    assert_eq!(tanks(&played), [(friendly, FIRST)]);
    let wreck = |state: &SurfaceSortieState| {
        let debris = state.world.debris.iter();
        debris
            .filter(|debris| debris.kind == DebrisKind::Fragment)
            .count()
    };
    assert_eq!(wreck(&played), 6);
    let mut restarted = garrison(1);
    assert!(tanks(&restarted).is_empty());
    assert_eq!(wreck(&restarted), 0);
    assert!(
        restarted
            .surface_force_observations()
            .iter()
            .all(|force| !matches!(force.id, SurfaceForceId::Ground(_)))
    );
    let first = place(&mut pristine, Vec2::X, GOVERNMENT);
    assert_eq!(place(&mut restarted, Vec2::X, GOVERNMENT), first);
    for state in [&mut pristine, &mut restarted] {
        step(state, [INTERACT, NEUTRAL]);
        idle(state, 60);
    }
    assert_eq!(ground_units(&restarted, 0), 1);
    assert_eq!(
        restarted.world.physics.world.snapshot_bytes().unwrap(),
        pristine.world.physics.world.snapshot_bytes().unwrap()
    );
    assert_eq!(
        SurfaceSortieScenario::observe(&restarted).payload,
        SurfaceSortieScenario::observe(&pristine).payload
    );
}
