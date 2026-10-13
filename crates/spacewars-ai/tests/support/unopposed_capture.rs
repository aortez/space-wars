//! Positive landing/capture fixtures retain both native actors. The otherwise
//! idle opponent uses ordinary thrust while the tested pilot works an unowned
//! flag, then releases it. A parked hostile ship would now defend that planet.
//! Production controllers and the tested pilot's actions remain unchanged.
use engine_common::{Action, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
    },
};
use std::time::Duration;

pub fn step(state: &mut SurfaceSortieState, actor: PlayerId, actions: &[Action], dt: Duration) {
    assert_eq!(state.player_count(), 2);
    let pilot = state.observation(actor.index());
    let claiming = pilot.location == PilotLocation::OnFoot
        && pilot
            .planet_claim
            .as_ref()
            .is_some_and(|claim| claim.owner != Some(actor));
    let mut actions = actions.to_vec();
    actions.push(
        SurfaceSortieAction {
            primary_held: claiming,
            ..Default::default()
        }
        .encode(actor.opponent()),
    );
    SurfaceSortieScenario::step(state, &actions, dt);
}
