use super::*;
use engine_rapier::world::BodyKind;
use free_flight::FreeFlightObservation;
use physics::MechanicalEntity;

type EncodedObjects = (Vec<[f32; 16]>, Vec<Option<PhysicsBodyId>>);

pub(super) fn encode(
    state: &SurfaceSortieState,
    flight: &FreeFlightObservation,
    world_radius: f32,
) -> Result<EncodedObjects, &'static str> {
    let world = &state.world.physics.world;
    let own = state.world.physics.ship_body(flight.vehicle.0).entity;
    let active = state.world.physics.planet_body(flight.planet);
    let solar = state.solar_hazard();
    let mut bodies: Vec<_> = world.motions().filter(|r| r.id.entity != own).collect();
    bodies.sort_by_key(|r| r.id);
    // Over capacity, the planets, sun and edge stay and the other bodies with
    // the nearest surfaces fill the remaining records, so a crowded scene such
    // as an asteroid stream keeps its closest obstacles instead of refusing.
    // A scene within capacity is encoded exactly as before.
    let global = |entity| {
        matches!(
            physics::classify_entity(entity),
            Some(MechanicalEntity::Body(BodyId::Planet(_) | BodyId::Sun) | MechanicalEntity::World)
        )
    };
    let mut others: Vec<_> = bodies
        .iter()
        .filter(|r| !global(r.id.entity))
        .filter_map(|r| {
            let radius = world.body_solid_radius(r.id)?;
            Some((
                (r.motion.position - flight.ship.position).length() - radius,
                r.id,
            ))
        })
        .collect();
    let globals = bodies.iter().filter(|r| global(r.id.entity)).count();
    if globals + others.len() > OBJECT_CAPACITY {
        others.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        others.truncate(OBJECT_CAPACITY.saturating_sub(globals));
        bodies.retain(|r| global(r.id.entity) || others.iter().any(|(_, id)| *id == r.id));
    }
    let mut objects = Vec::with_capacity(OBJECT_CAPACITY);
    let mut sources = Vec::with_capacity(OBJECT_CAPACITY);
    let mut planets = std::collections::BTreeSet::new();
    let mut sun_seen = false;
    let mut edge_seen = false;
    for record in bodies {
        let radius = world.body_solid_radius(record.id);
        let (kind, bound, hazard_radius, hazard_enabled) =
            match physics::classify_entity(record.id.entity) {
                Some(MechanicalEntity::Body(BodyId::Planet(index))) => {
                    if index >= state.world.planets.len() || !planets.insert(index) {
                        return Err("unsupported planet body grouping");
                    }
                    (1, radius.unwrap_or(0.0), 0.0, false)
                }
                Some(MechanicalEntity::Body(BodyId::Sun)) => {
                    if sun_seen {
                        return Err("duplicate solar body");
                    }
                    sun_seen = true;
                    let sun = solar.ok_or("solar body has no hazard descriptor")?;
                    (
                        2,
                        radius.ok_or("sun has no solid shape")?,
                        sun.heat_radius,
                        sun.heat_radius > sun.radius,
                    )
                }
                Some(MechanicalEntity::World) => {
                    if edge_seen || radius.is_none() {
                        return Err("invalid enclosing edge");
                    }
                    edge_seen = true;
                    (3, world_radius, 0.0, false)
                }
                _ => match radius {
                    Some(radius) => (4, radius, 0.0, false),
                    None => continue,
                },
            };
        if objects.len() == OBJECT_CAPACITY {
            return Err("flight object capacity exceeded");
        }
        // One independently moving rigid body per record. All attached terrain
        // colliders stay with that parent; articulated bodies remain separate.
        let motion = record.motion;
        let position = (motion.position - flight.ship.position).rotate_radians(-flight.ship.angle)
            / world_radius;
        let velocity = if world.body_kind(record.id) == Some(BodyKind::Fixed) {
            Vec2::ZERO
        } else {
            world
                .velocity_at_point(record.id, motion.position)
                .ok_or("missing object origin velocity")?
        };
        let velocity =
            (velocity - flight.ship.velocity).rotate_radians(-flight.ship.angle) / VELOCITY_SCALE;
        let angle = motion.angle - flight.ship.angle;
        let mut row = [0.0; 16];
        row[0] = 1.0;
        row[kind] = 1.0;
        row[5..].copy_from_slice(&[
            position.x,
            position.y,
            velocity.x,
            velocity.y,
            angle.sin(),
            angle.cos(),
            motion.angular_velocity / SPIN_SCALE,
            bound / world_radius,
            hazard_radius / world_radius,
            f32::from(hazard_enabled),
            f32::from(record.id == active),
        ]);
        objects.push(row);
        sources.push(Some(record.id));
    }
    if planets.len() != state.world.planets.len()
        || sun_seen != state.world.sun.is_some()
        || !edge_seen
    {
        return Err("incomplete global flight objects");
    }
    objects.resize(OBJECT_CAPACITY, [0.0; 16]);
    sources.resize(OBJECT_CAPACITY, None);
    Ok((objects, sources))
}
