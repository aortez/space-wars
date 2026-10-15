//! Positive landing/capture fixtures retain both native actors. The otherwise
//! idle opponent uses ordinary thrust only while its landing feet block the
//! tested pilot's flag. It releases thrust as soon as it clears the surface;
//! captures on other planets must not launch the later combat target again.
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
    let opponent = state.observation(actor.opponent().index());
    let departing = pilot.location == PilotLocation::OnFoot
        && pilot.planet_claim.as_ref().is_some_and(|claim| {
            claim.owner != Some(actor) && opponent.ship_support_planet == Some(claim.planet)
        });
    let mut actions = actions.to_vec();
    actions.push(
        SurfaceSortieAction {
            primary_held: departing,
            ..Default::default()
        }
        .encode(actor.opponent()),
    );
    SurfaceSortieScenario::step(state, &actions, dt);
}
