//! A small observation-only shortlist for the escape experiment. Nominal
//! geometry proposes bearings; the scenario measures actual landing and cover.
use super::*;
use scenario_spacewars::surface_sortie::destination_cover::{
    DestinationCoverRequest, MAX_COVER_CANDIDATES,
};

impl MaterialMissionPilot {
    pub(crate) fn configure_destination_cover_probe(&mut self, enabled: bool) {
        if let Some(d) = &mut self.telemetry.disengagement {
            d.cover_probe = enabled;
            d.cover_request = None;
        } else {
            assert!(!enabled, "enable disengagement before destination cover");
        }
    }

    pub(super) fn destination_shortlist(
        &self,
        o: &MissionObservationV1,
    ) -> DestinationCoverRequest {
        let p = &o.local.combat.recovery.flight.pilot;
        let mut planets: Vec<_> = o
            .planets
            .iter()
            .filter(|planet| {
                planet
                    .claim
                    .as_ref()
                    .is_none_or(|c| c.owner != Some(p.owner))
                    && !self
                        .deferred
                        .iter()
                        .any(|(id, until)| *id == planet.index && p.tick < *until)
            })
            .collect();
        planets.sort_by(|a, b| {
            a.motion
                .position
                .distance_to(p.ship.position)
                .total_cmp(&b.motion.position.distance_to(p.ship.position))
                .then(a.index.cmp(&b.index))
        });
        let mut candidates = [None; MAX_COVER_CANDIDATES];
        for (i, planet) in planets.into_iter().take(2).enumerate() {
            let bearing = |direction: Vec2| {
                let local = direction.rotate_radians(-planet.motion.angle);
                ((-local.x).atan2(local.y).rem_euclid(std::f32::consts::TAU)
                    * f32::from(LANDING_SITE_COUNT)
                    / std::f32::consts::TAU)
                    .round() as u8
                    % LANDING_SITE_COUNT
            };
            // Sample a shadow-side opportunity and the direct arrival side.
            // Coincident bearings get one quarter-turn alternative.
            let shadow = o
                .local
                .combat
                .target
                .map_or(p.ship.position - planet.motion.position, |enemy| {
                    planet.motion.position - enemy.motion.position
                });
            let a = bearing(shadow);
            let b = bearing(p.ship.position - planet.motion.position);
            candidates[i * 2] = Some(LandingSiteId {
                planet: planet.index,
                bearing: a,
            });
            candidates[i * 2 + 1] = Some(LandingSiteId {
                planet: planet.index,
                bearing: if a == b {
                    (b + LANDING_SITE_COUNT / 4) % LANDING_SITE_COUNT
                } else {
                    b
                },
            });
        }
        DestinationCoverRequest {
            generation: p.tick,
            candidates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortlist_is_bounded_uses_material_bearings_and_does_not_change_controls() {
        let (mut original, o) = super::super::tests::fixture(true);
        let mut probe = original.clone();
        probe.configure_destination_cover_probe(true);
        assert_eq!(probe.sensor_request().destination_cover, None);
        assert_eq!(original.intent(&o), probe.intent(&o));
        let request = probe.sensor_request().destination_cover.unwrap();
        let ids: Vec<_> = request.candidates.into_iter().flatten().collect();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|id| id.bearing < LANDING_SITE_COUNT));
        assert!(ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id)));
        let mut changed = o.clone();
        changed.planets[0].claim.as_mut().unwrap().owner =
            Some(o.local.combat.recovery.flight.pilot.owner);
        let request = probe.destination_shortlist(&changed);
        assert_eq!(request.candidates.into_iter().flatten().count(), 2);
        assert!(
            request
                .candidates
                .into_iter()
                .flatten()
                .all(|id| id.planet == 1)
        );
        let request = probe.sensor_request();
        let mut clone = probe.clone();
        assert_eq!(
            clone.sensor_request().destination_cover,
            request.destination_cover
        );
        assert_eq!(probe.intent(&o), clone.intent(&o));
        probe.end_disengagement(o.local.combat.recovery.flight.pilot.tick + 1, "test");
        assert_eq!(probe.sensor_request().destination_cover, None);
        probe.reset(probe.context);
        assert!(probe.telemetry.disengagement.as_ref().unwrap().cover_probe);
        assert_eq!(probe.sensor_request().destination_cover, None);
    }
}
