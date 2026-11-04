//! The two ways a block's surface ends other than melting, which an ordinary
//! Meltdown on a closed floor rarely shows: leaving the view through the open
//! drain, and still being solid when the face refills.

use super::presentation::open_floor;
use super::*;
use crate::digit_surface_tests::{Loss, Markers, assert_face_agreement};

#[test]
fn a_block_that_leaves_the_view_through_the_drain_retires_as_exited() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let mut state = ready(aspect, 0);
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    open_floor(event, 1.0);
    let piece = event.cells[0].piece.expect("a digit block").id;
    let block = &mut event.cells[0];
    block.position = Vec2::new(0.0, layout.floor_y + 15.0);
    block.velocity = Vec2::new(0.0, -500.0);
    block.release_tick = 0;
    event.tick = 30;
    for _ in 0..60 {
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
            panic!()
        };
        event.tick += 1;
        event.step_material(layout, &mut state.surfaces);
        // As a step ends, so the publication shows where the block went.
        state.settle_surfaces();
        markers.follow(&state);
    }
    let marker = markers.markers.iter().find(|m| m.surface == piece).unwrap();
    assert_eq!(marker.lost, Some(Loss::Retired(Retirement::Exited)));
    assert!(
        marker.world.y < layout.floor_y,
        "it was last seen below the floor"
    );
    assert!(state.digit_surfaces().iter().all(|s| s.id != piece));
    assert_eq!(state.surface_accounting().retired(Retirement::Exited), 1);
}

#[test]
fn a_block_still_solid_at_the_refill_is_reclaimed_and_its_cell_refills_new() {
    let aspect = 800.0 / 480.0;
    let mut state = ready(aspect, 0);
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    // A block that never falls is still solid, at its cell, when the face
    // refills.
    event.cells[0].release_tick = u64::MAX;
    let piece = event.cells[0].piece.expect("a digit block");
    while state.event_phase() != Some(crate::events::EventPhase::Reforming) {
        tick(&mut state);
        markers.follow(&state);
    }
    let surfaces = assert_face_agreement(&state, "refilling");
    let marker = markers
        .markers
        .iter()
        .find(|m| m.surface == piece.id)
        .unwrap();
    assert_eq!(marker.lost, Some(Loss::Retired(Retirement::Reclaimed)));
    assert_eq!(
        state.surface_accounting().retired(Retirement::Reclaimed),
        1,
        "only the block left solid is reclaimed"
    );
    let refilled = surfaces
        .iter()
        .find(|s| s.slot == piece.slot && s.cell == piece.cell)
        .expect("its cell refills");
    assert_ne!(refilled.id, piece.id);
    assert!(
        !markers.on(refilled.id),
        "the refill does not inherit what the block held"
    );
}
