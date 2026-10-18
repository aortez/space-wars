use super::TOIEntry;
use crate::alloc_prelude::*;
#[cfg(feature = "ccd-diagnostics")]
use crate::counters::ccd_probe::{self, Phase, PhaseSpan};
use crate::dynamics::{IntegrationParameters, IslandManager, RigidBodyHandle, RigidBodySet};
use crate::geometry::{
    BroadPhaseBvh, Collider, ColliderHandle, ColliderParent, ColliderSet, CollisionEvent,
    NarrowPhase,
};
use crate::math::Real;
use crate::parry::utils::SortedPair;
use crate::pipeline::{ActiveHooks, EventHandler, PairFilterContext, PhysicsHooks, QueryFilter};
use crate::prelude::{ActiveEvents, CollisionEventFlags};
use alloc::collections::BinaryHeap;
use parry::utils::hashmap::HashMap;

/// Shape geometry is immutable throughout one `PhysicsPipeline::step`, even
/// while collider poses change. Reuse compound thickness across candidate pairs,
/// first-impact searches and motion-clamping passes within that step. This must
/// remain step-local: discard it before callers can edit or replace colliders.
/// It never enters persistent solver state or snapshots.
#[derive(Default)]
pub(crate) struct ThicknessCache(HashMap<ColliderHandle, Real>);

impl ThicknessCache {
    fn get(&mut self, handle: ColliderHandle, collider: &Collider) -> Real {
        if let Some(_compound) = collider.shape.as_compound() {
            #[cfg(feature = "ccd-diagnostics")]
            ccd_probe::compound_lookup();
            *self.0.entry(handle).or_insert_with(|| {
                #[cfg(feature = "ccd-diagnostics")]
                ccd_probe::compound_miss(_compound.shapes().len());
                #[cfg(feature = "ccd-diagnostics")]
                let _span = PhaseSpan::new(Phase::CompoundThickness);
                collider.shape.ccd_thickness()
            })
        } else {
            #[cfg(feature = "ccd-diagnostics")]
            let _span = PhaseSpan::new(Phase::PrimitiveThickness);
            collider.shape.ccd_thickness()
        }
    }
}

/// Returns `true` if the user's `filter_contact_pair` hook rejected this
/// pair. Mirrors the narrow-phase filter call in `NarrowPhase::compute_contacts`
/// so CCD respects the same user-level contact filtering (see issue #754).
///
/// Note: the narrow phase may invoke this hook for sensor pairs too (it
/// uses `solver_flags` to decide downstream). The CCD sweep sites skip
/// sensors before calling this helper, which is intentional — CCD only
/// resolves contact TOIs, and sensor intersections are reported elsewhere.
#[inline]
fn pair_filtered_out_by_hooks(
    hooks: &dyn PhysicsHooks,
    bodies: &RigidBodySet,
    colliders: &ColliderSet,
    co1: &Collider,
    co2: &Collider,
    ch1: ColliderHandle,
    ch2: ColliderHandle,
    bh1: Option<RigidBodyHandle>,
    bh2: Option<RigidBodyHandle>,
) -> bool {
    let active_hooks = co1.flags.active_hooks | co2.flags.active_hooks;
    if !active_hooks.contains(ActiveHooks::FILTER_CONTACT_PAIRS) {
        return false;
    }
    let context = PairFilterContext {
        bodies,
        colliders,
        rigid_body1: bh1,
        rigid_body2: bh2,
        collider1: ch1,
        collider2: ch2,
    };
    hooks.filter_contact_pair(&context).is_none()
}

pub enum PredictedImpacts {
    Impacts(HashMap<RigidBodyHandle, Real>),
    ImpactsAfterEndTime(Real),
    NoImpacts,
}

/// Continuous Collision Detection solver that prevents fast objects from tunneling through geometry.
///
/// CCD (Continuous Collision Detection) solves the "tunneling problem" where fast-moving objects
/// pass through thin walls because they move more than the wall's thickness in one timestep.
///
/// ## How it works
///
/// 1. Detects which bodies are moving fast enough to potentially tunnel
/// 2. Predicts where/when they would impact during the timestep
/// 3. Clamps their motion to stop just before impact
/// 4. Next frame, normal collision detection handles the contact
///
/// ## When to use CCD
///
/// Enable CCD on bodies that:
/// - Move very fast (bullets, projectiles)
/// - Are small and hit thin geometry
/// - Must NEVER pass through walls (gameplay-critical)
///
/// **Cost**: More expensive than regular collision detection. Only use when needed!
///
/// Enable via `RigidBodyBuilder::ccd_enabled(true)` or `body.enable_ccd(true)`.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
pub struct CCDSolver;

impl CCDSolver {
    /// Initializes a new CCD solver
    pub fn new() -> Self {
        Self
    }

    /// Apply motion-clamping to the bodies affected by the given `impacts`.
    ///
    /// The `impacts` should be the result of a previous call to `self.predict_next_impacts`.
    pub fn clamp_motions(&self, dt: Real, bodies: &mut RigidBodySet, impacts: &PredictedImpacts) {
        if let PredictedImpacts::Impacts(tois) = impacts {
            for (handle, toi) in tois {
                let rb = bodies.index_mut_internal(*handle);
                let local_com = &rb.mprops.local_mprops.local_com;

                let min_toi = (rb.ccd.ccd_thickness
                    * 0.15
                    * crate::utils::inv(rb.ccd.max_point_velocity(&rb.ccd_vels)))
                .min(dt);
                // println!(
                //     "Min toi: {}, Toi: {}, thick: {}, max_vel: {}",
                //     min_toi,
                //     toi,
                //     rb.ccd.ccd_thickness,
                //     rb.ccd.max_point_velocity(&rb.integrated_vels)
                // );
                let new_pos = rb
                    .ccd_vels
                    .integrate(toi.max(min_toi), &rb.pos.position, local_com);
                rb.pos.next_position = new_pos;
            }
        }
    }

    /// Updates the set of bodies that needs CCD to be resolved.
    ///
    /// Returns `true` if any rigid-body must have CCD resolved.
    pub fn update_ccd_active_flags(
        &self,
        islands: &IslandManager,
        bodies: &mut RigidBodySet,
        dt: Real,
        include_forces: bool,
    ) -> bool {
        let mut ccd_active = false;

        // println!("Checking CCD activation");
        for handle in islands.active_bodies() {
            let rb = bodies.index_mut_internal(handle);

            if rb.ccd.ccd_enabled {
                let forces = if include_forces {
                    Some(&rb.forces)
                } else {
                    None
                };
                let moving_fast = rb.ccd.is_moving_fast(dt, &rb.ccd_vels, forces);
                rb.ccd.ccd_active = moving_fast;
                ccd_active = ccd_active || moving_fast;
            }
        }

        ccd_active
    }

    /// Find the first time a CCD-enabled body has a non-sensor collider hitting another non-sensor collider.
    pub fn find_first_impact(
        &mut self,
        dt: Real, // NOTE: this doesn’t necessarily match the `params.dt`.
        params: &IntegrationParameters,
        islands: &IslandManager,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        broad_phase: &mut BroadPhaseBvh,
        narrow_phase: &NarrowPhase,
        hooks: &dyn PhysicsHooks,
    ) -> Option<Real> {
        self.find_first_impact_with_cache(
            dt,
            params,
            islands,
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            hooks,
            &mut ThicknessCache::default(),
        )
    }

    #[profiling::function]
    pub(crate) fn find_first_impact_with_cache(
        &mut self,
        dt: Real,
        params: &IntegrationParameters,
        islands: &IslandManager,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        broad_phase: &mut BroadPhaseBvh,
        narrow_phase: &NarrowPhase,
        hooks: &dyn PhysicsHooks,
        thicknesses: &mut ThicknessCache,
    ) -> Option<Real> {
        #[cfg(feature = "ccd-diagnostics")]
        let setup_span = PhaseSpan::new(Phase::FirstSetup);
        // Update the query pipeline with the colliders’ predicted positions.
        for (handle, co) in colliders.iter_enabled() {
            if let Some(co_parent) = co.parent {
                let rb = &bodies[co_parent.handle];
                if rb.is_ccd_active() {
                    let predicted_pos = rb
                        .pos
                        .integrate_forces_and_velocities(dt, &rb.forces, &rb.vels, &rb.mprops);
                    let next_position = predicted_pos * co_parent.pos_wrt_parent;
                    #[cfg(feature = "ccd-diagnostics")]
                    let bounds_span = PhaseSpan::new(Phase::Bounds);
                    let swept_aabb = co.shape.compute_swept_aabb(&co.pos, &next_position);
                    #[cfg(feature = "ccd-diagnostics")]
                    drop(bounds_span);
                    #[cfg(feature = "ccd-diagnostics")]
                    let _update_span = PhaseSpan::new(Phase::BvhUpdate);
                    broad_phase.set_aabb(params, handle, swept_aabb);
                }
            }
        }

        let query_pipeline = broad_phase.as_query_pipeline(
            narrow_phase.query_dispatcher(),
            bodies,
            colliders,
            QueryFilter::default(),
        );

        let mut pairs_seen = HashMap::default();
        let mut min_toi = dt;

        #[cfg(feature = "ccd-diagnostics")]
        drop(setup_span);
        #[cfg(feature = "ccd-diagnostics")]
        let _search_span = PhaseSpan::new(Phase::FirstSearch);
        for handle in islands.active_bodies() {
            let rb1 = &bodies[handle];

            if rb1.ccd.ccd_active {
                let predicted_body_pos1 = rb1.pos.integrate_forces_and_velocities(
                    dt,
                    &rb1.forces,
                    &rb1.ccd_vels,
                    &rb1.mprops,
                );

                for ch1 in &rb1.colliders.0 {
                    let co1 = &colliders[*ch1];
                    let co1_parent = co1
                        .parent
                        .as_ref()
                        .expect("Could not find the ColliderParent component.");

                    if co1.is_sensor() {
                        continue; // Ignore sensors.
                    }

                    let predicted_collider_pos1 = predicted_body_pos1 * co1_parent.pos_wrt_parent;
                    #[cfg(feature = "ccd-diagnostics")]
                    let bounds_span = PhaseSpan::new(Phase::Bounds);
                    let aabb1 = co1
                        .shape
                        .compute_swept_aabb(&co1.pos, &predicted_collider_pos1);
                    #[cfg(feature = "ccd-diagnostics")]
                    drop(bounds_span);

                    let candidates = query_pipeline.intersect_aabb_conservative(aabb1);
                    #[cfg(feature = "ccd-diagnostics")]
                    let candidates = ccd_probe::timed_candidates(candidates);
                    for (ch2, _) in candidates {
                        #[cfg(feature = "ccd-diagnostics")]
                        ccd_probe::candidate(Phase::FirstSearch);
                        if *ch1 == ch2 {
                            // Ignore self-intersection.
                            continue;
                        }

                        if pairs_seen
                            .insert(
                                SortedPair::new(ch1.into_raw_parts().0, ch2.into_raw_parts().0),
                                (),
                            )
                            .is_none()
                        {
                            let co1 = &colliders[*ch1];
                            let co2 = &colliders[ch2];

                            let bh1 = co1.parent.map(|p| p.handle);
                            let bh2 = co2.parent.map(|p| p.handle);

                            // Ignore self-intersection and sensors and apply collision groups filter.
                            if bh1 == bh2                                                       // Ignore self-intersection.
                                    || (co1.is_sensor() || co2.is_sensor())                         // Ignore sensors.
                                    || !co1.flags.collision_groups.test(co2.flags.collision_groups) // Apply collision groups.
                                    || !co1.flags.solver_groups.test(co2.flags.solver_groups)
                            // Apply solver groups.
                            {
                                continue;
                            }

                            if pair_filtered_out_by_hooks(
                                hooks, bodies, colliders, co1, co2, *ch1, ch2, bh1, bh2,
                            ) {
                                continue;
                            }

                            // Most CCD candidates are rejected using shape thickness
                            // alone. Traverse the contact graph only if that check passes.
                            let smallest_dist = || {
                                #[cfg(feature = "ccd-diagnostics")]
                                let _contact_span = PhaseSpan::new(Phase::ContactLookup);
                                narrow_phase
                                    .contact_pair(*ch1, ch2)
                                    .and_then(|p| p.find_deepest_contact())
                                    .map(|c| c.1.dist)
                                    .unwrap_or(0.0)
                            };

                            let rb2 = bh2.and_then(|h| bodies.get(h));

                            if let Some(toi) = TOIEntry::try_from_colliders_with_thickness(
                                narrow_phase.query_dispatcher(),
                                *ch1,
                                ch2,
                                co1,
                                co2,
                                Some(rb1),
                                rb2,
                                None,
                                None,
                                0.0,
                                min_toi,
                                smallest_dist,
                                || thicknesses.get(*ch1, co1) + thicknesses.get(ch2, co2),
                            ) {
                                min_toi = min_toi.min(toi.toi);
                            }
                        }
                    }
                }
            }
        }

        if min_toi < dt {
            Some(min_toi)
        } else {
            None
        }
    }

    /// Outputs the set of bodies as well as their first time-of-impact event.
    pub fn predict_impacts_at_next_positions(
        &mut self,
        params: &IntegrationParameters,
        islands: &IslandManager,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        broad_phase: &mut BroadPhaseBvh,
        narrow_phase: &NarrowPhase,
        hooks: &dyn PhysicsHooks,
        events: &dyn EventHandler,
    ) -> PredictedImpacts {
        self.predict_impacts_at_next_positions_with_cache(
            params,
            islands,
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            hooks,
            events,
            &mut ThicknessCache::default(),
        )
    }

    #[profiling::function]
    pub(crate) fn predict_impacts_at_next_positions_with_cache(
        &mut self,
        params: &IntegrationParameters,
        islands: &IslandManager,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        broad_phase: &mut BroadPhaseBvh,
        narrow_phase: &NarrowPhase,
        hooks: &dyn PhysicsHooks,
        events: &dyn EventHandler,
        thicknesses: &mut ThicknessCache,
    ) -> PredictedImpacts {
        #[cfg(feature = "ccd-diagnostics")]
        let setup_span = PhaseSpan::new(Phase::ClampSetup);
        let dt = params.dt;
        let mut frozen = HashMap::<_, Real>::default();
        let mut all_toi = BinaryHeap::new();
        let mut pairs_seen = HashMap::default();
        let mut min_overstep = dt;

        // Update the query pipeline with the colliders’ `next_position`.
        for (handle, co) in colliders.iter_enabled() {
            if let Some(co_parent) = co.parent {
                let rb = &bodies[co_parent.handle];
                if rb.is_ccd_active() {
                    let rb_next_pos = &bodies[co_parent.handle].pos.next_position;
                    let next_position = rb_next_pos * co_parent.pos_wrt_parent;
                    #[cfg(feature = "ccd-diagnostics")]
                    let bounds_span = PhaseSpan::new(Phase::Bounds);
                    let swept_aabb = co.shape.compute_swept_aabb(&co.pos, &next_position);
                    #[cfg(feature = "ccd-diagnostics")]
                    drop(bounds_span);
                    #[cfg(feature = "ccd-diagnostics")]
                    let _update_span = PhaseSpan::new(Phase::BvhUpdate);
                    broad_phase.set_aabb(params, handle, swept_aabb);
                }
            }
        }

        let query_pipeline = broad_phase.as_query_pipeline(
            narrow_phase.query_dispatcher(),
            bodies,
            colliders,
            QueryFilter::default(),
        );

        /*
         *
         * First, collect all TOIs.
         *
         */
        #[cfg(feature = "ccd-diagnostics")]
        drop(setup_span);
        #[cfg(feature = "ccd-diagnostics")]
        let search_span = PhaseSpan::new(Phase::ClampSearch);
        // TODO: don't iterate through all the colliders.
        for handle in islands.active_bodies() {
            let rb1 = &bodies[handle];

            if rb1.ccd.ccd_active {
                let predicted_body_pos1 = rb1.pos.integrate_forces_and_velocities(
                    dt,
                    &rb1.forces,
                    &rb1.ccd_vels,
                    &rb1.mprops,
                );

                for ch1 in &rb1.colliders.0 {
                    let co1 = &colliders[*ch1];
                    let co_parent1 = co1
                        .parent
                        .as_ref()
                        .expect("Could not find the ColliderParent component.");

                    let predicted_collider_pos1 = predicted_body_pos1 * co_parent1.pos_wrt_parent;
                    #[cfg(feature = "ccd-diagnostics")]
                    let bounds_span = PhaseSpan::new(Phase::Bounds);
                    let aabb1 = co1
                        .shape
                        .compute_swept_aabb(&co1.pos, &predicted_collider_pos1);
                    #[cfg(feature = "ccd-diagnostics")]
                    drop(bounds_span);

                    let candidates = query_pipeline.intersect_aabb_conservative(aabb1);
                    #[cfg(feature = "ccd-diagnostics")]
                    let candidates = ccd_probe::timed_candidates(candidates);
                    for (ch2, _) in candidates {
                        #[cfg(feature = "ccd-diagnostics")]
                        ccd_probe::candidate(Phase::ClampSearch);
                        if *ch1 == ch2 {
                            // Ignore self-intersection.
                            continue;
                        }

                        if pairs_seen
                            .insert(
                                SortedPair::new(ch1.into_raw_parts().0, ch2.into_raw_parts().0),
                                (),
                            )
                            .is_none()
                        {
                            let co1 = &colliders[*ch1];
                            let co2 = &colliders[ch2];

                            let bh1 = co1.parent.map(|p| p.handle);
                            let bh2 = co2.parent.map(|p| p.handle);

                            // Ignore self-intersections and apply groups filter.
                            if bh1 == bh2
                                || !co1.flags.collision_groups.test(co2.flags.collision_groups)
                            {
                                continue;
                            }

                            if pair_filtered_out_by_hooks(
                                hooks, bodies, colliders, co1, co2, *ch1, ch2, bh1, bh2,
                            ) {
                                continue;
                            }

                            // Most CCD candidates are rejected using shape thickness
                            // alone. Traverse the contact graph only if that check passes.
                            let smallest_dist = || {
                                #[cfg(feature = "ccd-diagnostics")]
                                let _contact_span = PhaseSpan::new(Phase::ContactLookup);
                                narrow_phase
                                    .contact_pair(*ch1, ch2)
                                    .and_then(|p| p.find_deepest_contact())
                                    .map(|c| c.1.dist)
                                    .unwrap_or(0.0)
                            };

                            let rb1 = bh1.map(|h| &bodies[h]);
                            let rb2 = bh2.map(|h| &bodies[h]);

                            if let Some(toi) = TOIEntry::try_from_colliders_with_thickness(
                                query_pipeline.dispatcher,
                                *ch1,
                                ch2,
                                co1,
                                co2,
                                rb1,
                                rb2,
                                None,
                                None,
                                0.0,
                                // NOTE: we use dt here only once we know that
                                // there is at least one TOI before dt.
                                min_overstep,
                                smallest_dist,
                                || thicknesses.get(*ch1, co1) + thicknesses.get(ch2, co2),
                            ) {
                                if toi.toi > dt {
                                    min_overstep = min_overstep.min(toi.toi);
                                } else {
                                    min_overstep = dt;
                                    all_toi.push(toi);
                                }
                            }
                        }
                    }
                }
            }
        }

        /*
         *
         * If the smallest TOI is outside of the time interval, return.
         *
         */
        #[cfg(feature = "ccd-diagnostics")]
        drop(search_span);
        if min_overstep == dt && all_toi.is_empty() {
            return PredictedImpacts::NoImpacts;
        } else if min_overstep > dt {
            return PredictedImpacts::ImpactsAfterEndTime(min_overstep);
        }

        // NOTE: all fixed bodies (and kinematic bodies?) should be considered as "frozen", this
        // may avoid some resweeps.
        let mut pseudo_intersections_to_check = vec![];

        #[cfg(feature = "ccd-diagnostics")]
        let _resweep_span = PhaseSpan::new(Phase::Resweep);
        while let Some(toi) = all_toi.pop() {
            assert!(toi.toi <= dt);

            let rb1 = toi.b1.and_then(|b| bodies.get(b));
            let rb2 = toi.b2.and_then(|b| bodies.get(b));

            let mut colliders_to_check = Vec::new();
            let should_freeze1 = rb1.is_some()
                && rb1.unwrap().ccd.ccd_active
                && !frozen.contains_key(&toi.b1.unwrap());
            let should_freeze2 = rb2.is_some()
                && rb2.unwrap().ccd.ccd_active
                && !frozen.contains_key(&toi.b2.unwrap());

            if !should_freeze1 && !should_freeze2 {
                continue;
            }

            if toi.is_pseudo_intersection_test {
                // NOTE: this test is redundant with the previous `if !should_freeze && ...`
                //       but let's keep it to avoid tricky regressions if we end up swapping both
                //       `if` for some reason in the future.
                if should_freeze1 || should_freeze2 {
                    // This is only an intersection so we don't have to freeze and there is no
                    // need to resweep. However, we will need to see if we have to generate
                    // intersection events, so push the TOI for further testing.
                    pseudo_intersections_to_check.push(toi);
                }
                continue;
            }

            if should_freeze1 {
                let _ = frozen.insert(toi.b1.unwrap(), toi.toi);
                colliders_to_check.extend_from_slice(&rb1.unwrap().colliders.0);
            }

            if should_freeze2 {
                let _ = frozen.insert(toi.b2.unwrap(), toi.toi);
                colliders_to_check.extend_from_slice(&rb2.unwrap().colliders.0);
            }

            let start_time = toi.toi;

            // NOTE: the 1 and 2 indices (e.g., `ch1`, `ch2`) below are unrelated to the
            //       ones we used above.
            for ch1 in &colliders_to_check {
                let co1 = &colliders[*ch1];
                let co1_parent = co1.parent.as_ref().unwrap();
                let rb1 = &bodies[co1_parent.handle];

                let co_next_pos1 = rb1.pos.next_position * co1_parent.pos_wrt_parent;
                #[cfg(feature = "ccd-diagnostics")]
                let bounds_span = PhaseSpan::new(Phase::Bounds);
                let aabb = co1.shape.compute_swept_aabb(&co1.pos, &co_next_pos1);
                #[cfg(feature = "ccd-diagnostics")]
                drop(bounds_span);

                let candidates = query_pipeline.intersect_aabb_conservative(aabb);
                #[cfg(feature = "ccd-diagnostics")]
                let candidates = ccd_probe::timed_candidates(candidates);
                for (ch2, _) in candidates {
                    #[cfg(feature = "ccd-diagnostics")]
                    ccd_probe::candidate(Phase::Resweep);
                    let co2 = &colliders[ch2];

                    let bh1 = co1.parent.map(|p| p.handle);
                    let bh2 = co2.parent.map(|p| p.handle);

                    // Ignore self-intersection and apply groups filter.
                    if bh1 == bh2 || !co1.flags.collision_groups.test(co2.flags.collision_groups) {
                        continue;
                    }

                    if pair_filtered_out_by_hooks(
                        hooks, bodies, colliders, co1, co2, *ch1, ch2, bh1, bh2,
                    ) {
                        continue;
                    }

                    let frozen1 = bh1.and_then(|h| frozen.get(&h));
                    let frozen2 = bh2.and_then(|h| frozen.get(&h));

                    let rb1 = bh1.and_then(|h| bodies.get(h));
                    let rb2 = bh2.and_then(|h| bodies.get(h));

                    if (frozen1.is_some() || !rb1.map(|b| b.ccd.ccd_active).unwrap_or(false))
                        && (frozen2.is_some() || !rb2.map(|b| b.ccd.ccd_active).unwrap_or(false))
                    {
                        // We already did a resweep.
                        continue;
                    }

                    // Most CCD candidates are rejected using shape thickness
                    // alone. Traverse the contact graph only if that check passes.
                    let smallest_dist = || {
                        #[cfg(feature = "ccd-diagnostics")]
                        let _contact_span = PhaseSpan::new(Phase::ContactLookup);
                        narrow_phase
                            .contact_pair(*ch1, ch2)
                            .and_then(|p| p.find_deepest_contact())
                            .map(|c| c.1.dist)
                            .unwrap_or(0.0)
                    };

                    if let Some(toi) = TOIEntry::try_from_colliders_with_thickness(
                        query_pipeline.dispatcher,
                        *ch1,
                        ch2,
                        co1,
                        co2,
                        rb1,
                        rb2,
                        frozen1.copied(),
                        frozen2.copied(),
                        start_time,
                        dt,
                        smallest_dist,
                        || thicknesses.get(*ch1, co1) + thicknesses.get(ch2, co2),
                    ) {
                        all_toi.push(toi);
                    }
                }
            }
        }

        for toi in pseudo_intersections_to_check {
            // See if the intersection is still active once the bodies
            // reach their final positions.
            // - If the intersection is still active, don't report it yet. It will be
            //   reported by the narrow-phase at the next timestep/substep.
            // - If the intersection isn't active anymore, and it wasn't intersecting
            //   before, then we need to generate one interaction-start and one interaction-stop
            //   events because it will never be detected by the narrow-phase because of tunneling.
            let co1 = &colliders[toi.c1];
            let co2 = &colliders[toi.c2];

            if !co1.is_sensor() && !co2.is_sensor() {
                // TODO: this happens if we found a TOI between two non-sensor
                //       colliders with mismatching solver_flags. It is not clear
                //       what we should do in this case: we could report a
                //       contact started/contact stopped event for example. But in
                //       that case, what contact pair should be pass to these events?
                // For now we just ignore this special case. Let's wait for an actual
                // use-case to come up before we determine what we want to do here.
                continue;
            }

            let co_next_pos1 = if let Some(b1) = toi.b1 {
                let co_parent1: &ColliderParent = co1.parent.as_ref().unwrap();
                let rb1 = &bodies[b1];
                let local_com1 = &rb1.mprops.local_mprops.local_com;
                let frozen1 = frozen.get(&b1);
                let pos1 = frozen1
                    .map(|t| rb1.ccd_vels.integrate(*t, &rb1.pos.position, local_com1))
                    .unwrap_or(rb1.pos.next_position);
                pos1 * co_parent1.pos_wrt_parent
            } else {
                co1.pos.0
            };

            let co_next_pos2 = if let Some(b2) = toi.b2 {
                let co_parent2: &ColliderParent = co2.parent.as_ref().unwrap();
                let rb2 = &bodies[b2];
                let local_com2 = &rb2.mprops.local_mprops.local_com;
                let frozen2 = frozen.get(&b2);
                let pos2 = frozen2
                    .map(|t| rb2.ccd_vels.integrate(*t, &rb2.pos.position, local_com2))
                    .unwrap_or(rb2.pos.next_position);
                pos2 * co_parent2.pos_wrt_parent
            } else {
                co2.pos.0
            };

            let prev_coll_pos12 = co1.pos.inv_mul(&co2.pos);
            let next_coll_pos12 = co_next_pos1.inv_mul(&co_next_pos2);

            let intersect_before = query_pipeline
                .dispatcher
                .intersection_test(&prev_coll_pos12, co1.shape.as_ref(), co2.shape.as_ref())
                .unwrap_or(false);

            let intersect_after = query_pipeline
                .dispatcher
                .intersection_test(&next_coll_pos12, co1.shape.as_ref(), co2.shape.as_ref())
                .unwrap_or(false);

            if !intersect_before
                && !intersect_after
                && (co1.flags.active_events | co2.flags.active_events)
                    .contains(ActiveEvents::COLLISION_EVENTS)
            {
                // Emit one intersection-started and one intersection-stopped event.
                events.handle_collision_event(
                    bodies,
                    colliders,
                    CollisionEvent::Started(toi.c1, toi.c2, CollisionEventFlags::SENSOR),
                    None,
                );
                events.handle_collision_event(
                    bodies,
                    colliders,
                    CollisionEvent::Stopped(toi.c1, toi.c2, CollisionEventFlags::SENSOR),
                    None,
                );
            }
        }

        PredictedImpacts::Impacts(frozen)
    }
}
