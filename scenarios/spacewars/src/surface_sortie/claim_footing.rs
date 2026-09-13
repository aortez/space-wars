//! Nearby retained-material anchors, sampled alongside the measured ground map.
//! These are proposals only: actual support still authorizes every claim tick.
use super::*;
use claim::PlanetClaimStatus;
use ground_navigation::GroundMap;

pub const MAX_CLAIM_FOOTINGS: usize = 16;
pub const CLAIM_SEARCH_RADIUS: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClaimFootingSurvey {
    pub version: u32,
    pub owner: PlayerId,
    pub planet: usize,
    pub revision: u64,
    pub tick: u64,
    /// Proposed standing centers in the retained planet's local frame.
    pub positions: Vec<Vec2>,
}

impl SurfaceSortieState {
    pub(super) fn claim_footing_survey(
        &self,
        p: &pilot::PilotObservationV1,
        ground: Option<&GroundMap>,
    ) -> Option<ClaimFootingSurvey> {
        if self.world.physics.material_queries_dirty
            || !p.queries_ready
            || p.location != PilotLocation::OnFoot
            || !p.balanced
            || p.supported_planet != Some(p.planet.index)
            || !p.planet.claim.as_ref().is_some_and(|c| {
                c.owner != Some(p.owner) && c.status == PlanetClaimStatus::NeedSupport
            })
        {
            return None;
        }
        let map = ground?;
        if map.actor != p.owner
            || map.planet != p.planet.index
            || map.revision != p.planet.revision
            || map.tick != p.tick
        {
            return None;
        }
        let actor = p.actor?;
        let terrain = self.world.terrain.planets.get(&p.planet.index)?;
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let center = local(actor.position);
        let flag = p.planet.claim.as_ref().and_then(|c| c.flag);
        let mut positions: Vec<_> = map
            .nodes
            .iter()
            .filter_map(|node| {
                // Use the same surface-to-material rule as the real flag anchor.
                let cell =
                    terrain
                        .geometry
                        .contact_cell(&terrain.field, node.position, node.normal)?;
                if terrain.field.cell(cell)?.material == engine_terrain::MaterialId::VOID
                    || flag.is_some_and(|f| node.position.distance_to(local(f.position)) > 2.5)
                {
                    return None;
                }
                let position =
                    node.position + node.position.normalized() * Self::spec().half_height();
                let distance = position.distance_to(center);
                (1.25..=CLAIM_SEARCH_RADIUS)
                    .contains(&distance)
                    .then_some(position)
            })
            .collect();
        positions.sort_by(|a, b| a.distance_to(center).total_cmp(&b.distance_to(center)));
        positions.truncate(MAX_CLAIM_FOOTINGS);
        Some(ClaimFootingSurvey {
            version: 1,
            owner: p.owner,
            planet: p.planet.index,
            revision: p.planet.revision,
            tick: p.tick,
            positions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_proposals_use_retained_cells_and_never_mutate_or_authorize_support() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let map = state
            .survey_ground(0, 0, 0..ground_navigation::GROUND_SAMPLES as u16, false)
            .unwrap();
        let node = map.nodes.iter().find(|n| n.id == 128).unwrap();
        let mut p = state.pilot_observation(0, None);
        p.location = PilotLocation::OnFoot;
        p.balanced = true;
        p.supported_planet = Some(0);
        p.planet.claim.as_mut().unwrap().status = PlanetClaimStatus::NeedSupport;
        p.actor = Some(p.ship);
        p.actor.as_mut().unwrap().position = p.planet.motion.position
            + (node.position
                + node.position.normalized() * SurfaceSortieState::spec().half_height())
            .rotate_radians(p.planet.motion.angle);
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let survey = state.claim_footing_survey(&p, Some(&map)).unwrap();
        assert!(!survey.positions.is_empty());
        assert!(survey.positions.len() <= MAX_CLAIM_FOOTINGS);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(state.claim_footing_survey(&p, Some(&map)), Some(survey));
        assert!(state.claim_footing_survey(&p, None).is_none());
        state.world.physics.material_queries_dirty = true;
        assert!(state.claim_footing_survey(&p, Some(&map)).is_none());
        assert!(state.world.physics.material_queries_dirty);
        state.world.physics.material_queries_dirty = false;
        p.queries_ready = false;
        assert!(state.claim_footing_survey(&p, Some(&map)).is_none());
        p.queries_ready = true;
        p.supported_planet = None;
        assert!(
            state.claim_footing_survey(&p, Some(&map)).is_none(),
            "debris is not retained planet support"
        );
        p.supported_planet = Some(0);
        p.planet.claim.as_mut().unwrap().status = PlanetClaimStatus::Contested;
        assert!(state.claim_footing_survey(&p, Some(&map)).is_none());
    }
}
