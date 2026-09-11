//! Read-only short crawl probes for a supported character that cannot stand.
use super::*;
pub use engine_rapier::spaceling::{SpacelingBalance, SpacelingGetUpResult};

pub const CRAWL_DISTANCE: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CrawlStep {
    /// Ordinary horizontal input, along the actual contact tangent.
    pub direction: f32,
    /// Candidate actor center in the retained planet's local frame.
    pub position: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundPostureObservation {
    pub version: u32,
    pub owner: PlayerId,
    pub planet: usize,
    pub revision: u64,
    pub tick: u64,
    pub balance: SpacelingBalance,
    pub get_up_result: SpacelingGetUpResult,
    pub get_up_attempts: u64,
    pub stable: bool,
    /// Space for the conservative upright pose used by the ground map.
    pub standing_clear: bool,
    pub crawl_clearance: [Option<f32>; 2],
    pub crawl_floor: [Option<bool>; 2],
    /// Left and right, checked afresh against the completed physics step.
    pub crawl: [Option<CrawlStep>; 2],
}

impl SurfaceSortieState {
    pub(super) fn ground_posture_observation(
        &self,
        player: usize,
        p: &pilot::PilotObservationV1,
    ) -> Option<GroundPostureObservation> {
        if !p.queries_ready || p.location != PilotLocation::OnFoot {
            return None;
        }
        let body = self.pilots.get(player)?.body.as_ref()?;
        let physics = &self.world.physics.world;
        let snapshot = body.snapshot(physics)?;
        let spec = Self::spec();
        let stable = snapshot.support.is_some_and(|contact| {
            let offset = contact.position - snapshot.motion.position;
            let velocity = snapshot.motion.linear_velocity
                + Vec2::new(-offset.y, offset.x) * snapshot.motion.angular_velocity;
            (velocity - contact.velocity).length() <= spec.balance.settle_speed
                && (snapshot.motion.angular_velocity - contact.angular_velocity).abs()
                    <= spec.balance.settle_angular_speed
        });
        let standing_clear = snapshot.support.is_none_or(|contact| {
            physics.capsule_clearance_test_excluding(
                spec.half_segment,
                spec.radius + 0.02,
                spec.collision_groups,
                vec![pilot_physics_id(p.owner)],
            )(
                contact.position + snapshot.up * ground_navigation::standing_height(),
                rotation_for_direction(snapshot.up),
            )
        });
        let mut observation = GroundPostureObservation {
            version: 1,
            owner: p.owner,
            planet: p.planet.index,
            revision: p.planet.revision,
            tick: p.tick,
            balance: snapshot.balance,
            get_up_result: snapshot.get_up_result,
            get_up_attempts: snapshot.get_up_attempts,
            stable,
            standing_clear,
            crawl_clearance: [None, None],
            crawl_floor: [None, None],
            crawl: [None, None],
        };
        if !stable
            || !(snapshot.balance == SpacelingBalance::Recovering
                && snapshot.get_up_result == SpacelingGetUpResult::Blocked
                || snapshot.balance == SpacelingBalance::Balanced && !standing_clear)
            || p.supported_planet != Some(p.planet.index)
        {
            return Some(observation);
        }
        let contact = snapshot.support?;
        let tangent = Vec2::new(contact.normal.y, -contact.normal.x);
        // Contact penetration is solver slop. Offset only the query by its
        // allowance; no pose, velocity or support permission is changed.
        // Sweeps permit separating from an existing contact, which a static
        // overlap test would reject even when crawling away from the obstacle.
        for (i, direction) in [-1.0, 1.0].into_iter().enumerate() {
            let travel = tangent * direction;
            let start = snapshot.motion.position + contact.normal * 0.025;
            observation.crawl_clearance[i] = physics.collider_translation_clearance_at(
                body.collider(),
                start,
                snapshot.motion.angle,
                travel,
                CRAWL_DISTANCE + 0.04,
            );
            let Some(clearance) = observation.crawl_clearance[i] else {
                continue;
            };
            // A tilted but supported capsule may only have room for a fraction
            // of a step under the hull. Recheck after each ordinary input as
            // contacts rotate it, reserving more than one walking tick of space.
            let distance = if snapshot.balance == SpacelingBalance::Balanced {
                (clearance - 0.04).min(CRAWL_DISTANCE)
            } else if clearance >= CRAWL_DISTANCE + 0.02 {
                CRAWL_DISTANCE
            } else {
                0.0
            };
            if distance < 0.10 {
                continue;
            }
            // Require surviving floor throughout the short corridor. Detached
            // fragments and unsupported edges never supply crawl eligibility.
            let floor = [distance / 3.0, distance * 2.0 / 3.0, distance]
                .into_iter()
                .all(|distance| {
                    let origin = contact.position + travel * distance + contact.normal * 0.2;
                    self.world
                        .physics
                        .material_ground_ray(p.planet.index, origin, -contact.normal, 0.4)
                        .is_some_and(|hit| {
                            hit.normal.dot(snapshot.up) >= spec.min_support_alignment
                        })
                });
            observation.crawl_floor[i] = Some(floor);
            if floor {
                let position = snapshot.motion.position + travel * distance;
                observation.crawl[i] = Some(CrawlStep {
                    direction,
                    position: (position - p.planet.motion.position)
                        .rotate_radians(-p.planet.motion.angle),
                });
            }
        }
        Some(observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    };

    #[test]
    fn crawl_survey_is_read_only_and_ordinary_controls_escape_a_low_roof() {
        let dt = Duration::from_nanos(16_666_667);
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        state.world.planets[0].wrapper_omega = 0.0;
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let up = -Vec2::Y;
        let hit = state
            .world
            .physics
            .material_ground_ray(0, state.world.planets[0].position + up * 80.0, -up, 100.0)
            .unwrap();
        let roof = PhysicsId::new(45_123);
        assert!(state.world.physics.world.insert_body(
            BodyId::new(roof, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: hit.point + up * 1.05,
                angle: rotation_for_direction(up),
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(roof, ColliderRole::PRIMARY, 0),
                2.5,
                0.15
            )],
        ));
        let mut body = SpacelingAssembly::insert(
            &mut state.world.physics.world,
            pilot_physics_id(PlayerId::PLAYER_1),
            hit.point + up * 0.32,
            std::f32::consts::FRAC_PI_2,
            SurfaceSortieState::spec(),
        )
        .unwrap();
        state
            .world
            .physics
            .world
            .set_velocity(body.body(), Vec2::ZERO, 10.0, true);
        body.apply_control(
            &mut state.world.physics.world,
            SpacelingControl::default(),
            -up * 18.0,
            1.0 / 60.0,
        );
        state
            .world
            .physics
            .world
            .set_velocity(body.body(), Vec2::ZERO, 0.0, true);
        state.pilots[0].body = Some(body);
        state.pilots[0].controls_armed = true;
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                primary_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            dt,
        );
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let old = state.flight_pilot_observation(0, None);
        let observed = state.recovery_task_observation(0, None);
        assert_eq!(old, observed.flight);
        let posture = observed.posture.unwrap();
        assert_eq!(
            posture.get_up_result,
            SpacelingGetUpResult::Blocked,
            "{posture:?}"
        );
        assert_eq!(posture.balance, SpacelingBalance::Recovering);
        let step = posture
            .crawl
            .iter()
            .flatten()
            .next()
            .expect("measured crawl under roof");
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        state.world.physics.material_queries_dirty = true;
        assert!(state.recovery_task_observation(0, None).posture.is_none());
        assert!(state.world.physics.material_queries_dirty);
        state.world.physics.material_queries_dirty = false;
        let mut unsupported = old.pilot;
        unsupported.supported_planet = None;
        assert_eq!(
            state
                .ground_posture_observation(0, &unsupported)
                .unwrap()
                .crawl,
            [None, None]
        );
        for _ in 0..360 {
            SurfaceSortieScenario::step(
                &mut state,
                &[SurfaceSortieAction {
                    horizontal: step.direction,
                    ..Default::default()
                }
                .encode(PlayerId::PLAYER_1)],
                dt,
            );
        }
        let standing = state.spaceling_snapshot(0).unwrap();
        assert_eq!(standing.balance, SpacelingBalance::Balanced, "{standing:?}");
        assert!(standing.grounded());
        assert_eq!(standing.jumps, 0);
    }
}
