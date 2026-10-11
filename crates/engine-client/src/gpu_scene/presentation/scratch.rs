//! At most one completed CPU plan, retained only below a hard owned-capacity
//! budget. No GL handles, source-frame references or immutable artwork survive.
use super::{Pass, Plan};
use crate::gpu_scene::plan::Shape;

pub(crate) const LIMIT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Scratch {
    plan: Option<Plan>,
    pub bytes: usize,
    pub high_water_bytes: usize,
    pub reuses: u64,
    pub discards: u64,
}

impl Scratch {
    pub fn take(&mut self) -> Option<Plan> {
        let plan = self.plan.take();
        self.reuses += u64::from(plan.is_some());
        self.bytes = 0;
        plan
    }

    pub fn recycle(&mut self, plan: Plan) {
        self.recycle_with_limit(plan, LIMIT_BYTES);
    }

    fn recycle_with_limit(&mut self, mut plan: Plan, limit: usize) {
        // Replace rather than accumulate: skipped draws and shrinking scenes
        // cannot leave a pool of obsolete allocations or retained definitions.
        self.plan = None;
        self.bytes = 0;
        let mut bytes = capacity_bytes(&plan.passes);
        for pass in &mut plan.passes {
            let scene = match pass {
                Pass::Direct { scene, .. } | Pass::Minimap { scene, .. } => scene,
            };
            scene.reuse.clear();
            scene.retained.clear();
            bytes = bytes
                .saturating_add(capacity_bytes(&scene.draws))
                .saturating_add(capacity_bytes(&scene.reuse))
                .saturating_add(capacity_bytes(&scene.retained));
            for draw in &scene.draws {
                match &draw.shape {
                    Shape::Polygon(points) => bytes = bytes.saturating_add(capacity_bytes(points)),
                    // This live adapter does not produce local polygons. Don't
                    // accidentally turn storage recycling into an artwork cache.
                    Shape::LocalPolygon(_) => {
                        self.discards += 1;
                        return;
                    }
                    _ => {}
                }
            }
            if bytes > limit {
                self.discards += 1;
                return;
            }
        }
        if bytes > limit {
            self.discards += 1;
            return;
        }
        self.bytes = bytes;
        self.high_water_bytes = self.high_water_bytes.max(bytes);
        self.plan = Some(plan);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn capacity_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(std::mem::size_of::<T>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gpu_scene::{PlanMode, presentation},
        render::{FrameLayout, Viewport},
    };
    use engine_common::{
        Camera2, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
    };

    fn frame(count: usize) -> RenderFrame {
        let mut f = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
        for i in 0..count {
            f.push_primitive(
                0,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    vec![
                        RenderPoint::new(i as f32, 0.),
                        RenderPoint::new(i as f32 + 1., 0.),
                        RenderPoint::new(i as f32, 2.),
                    ],
                    RenderColor::RED,
                )),
            );
        }
        f
    }

    fn build(f: &RenderFrame, scratch: &mut Scratch) -> Plan {
        presentation::build_reusing(
            std::slice::from_ref(f),
            Viewport::new(640., 480.),
            FrameLayout::EqualHorizontal,
            1.,
            true,
            PlanMode::Scratch,
            scratch.take(),
        )
        .unwrap()
    }

    #[test]
    fn reuses_allocations_but_rewrites_geometry_and_truncates_old_draws() {
        let mut scratch = Scratch::default();
        let mut f = frame(12);
        let plan = build(&f, &mut scratch);
        let scene = plan.passes[0].scene();
        let draws = scene.draws.as_ptr();
        let Shape::Polygon(points) = &scene.draws[0].shape else {
            panic!()
        };
        let pointer = points.as_ptr();
        scratch.recycle(plan);
        f.camera.center.x = 10.;
        f.layers[0].primitives.truncate(3);
        let changed = build(&f, &mut scratch);
        let scene = changed.passes[0].scene();
        assert_eq!(scene.draws.len(), 3);
        assert_eq!(scene.draws.as_ptr(), draws);
        let Shape::Polygon(points) = &scene.draws[0].shape else {
            panic!()
        };
        assert_eq!(points.as_ptr(), pointer);
        let expected = presentation::build_with_storage(
            &[f.clone()],
            Viewport::new(640., 480.),
            FrameLayout::EqualHorizontal,
            1.,
            true,
            PlanMode::Reference,
        )
        .unwrap();
        assert_eq!(format!("{changed:?}"), format!("{expected:?}"));
        scratch.recycle(changed);
        // Changing to no geometry cannot replay a stale prefix or tail.
        f.layers.clear();
        let empty = build(&f, &mut scratch);
        assert_eq!(empty.shapes(), 0);
        scratch.recycle(empty);
        assert!(scratch.bytes <= LIMIT_BYTES);
        assert_eq!(scratch.reuses, 2);
        scratch.clear();
        assert_eq!(scratch.bytes, 0);
        assert!(scratch.take().is_none());
    }

    #[test]
    fn capacity_budget_discards_oversized_storage_and_releases_artwork() {
        use engine_common::{PolygonGeometry, PolygonGeometryHint};
        use std::sync::Arc;
        let mut f = frame(1);
        let RenderPrimitive::Polygon(p) = &f.layers[0].primitives[0] else {
            panic!()
        };
        let definition = Arc::new(PolygonGeometry::new(vec![p.clone()]).unwrap());
        let weak = Arc::downgrade(&definition);
        f.geometry_hints.push(PolygonGeometryHint {
            layer: 0,
            first: 0,
            geometry: definition,
            position: RenderPoint::ZERO,
            angle: 0.,
        });
        let mut scratch = Scratch::default();
        let plan = build(&f, &mut scratch);
        assert_eq!(plan.passes[0].scene().retained.len(), 1);
        drop(f);
        assert!(weak.upgrade().is_some());
        scratch.recycle(plan);
        assert!(weak.upgrade().is_none());
        let bytes = scratch.bytes;
        let plan = scratch.take().unwrap();
        scratch.recycle_with_limit(plan, bytes);
        assert_eq!(scratch.bytes, bytes);
        let plan = scratch.take().unwrap();
        scratch.recycle_with_limit(plan, bytes - 1);
        assert_eq!(scratch.bytes, 0);
        assert_eq!(scratch.discards, 1);
        assert!(scratch.take().is_none());
        let fresh = build(&frame(2), &mut scratch);
        assert_eq!(fresh.shapes(), 2);
    }

    #[test]
    fn changed_types_skipped_shapes_and_rejected_frames_cannot_replay_old_contents() {
        use engine_common::{RenderCircle, RenderText};
        let mut scratch = Scratch::default();
        let original = frame(6);
        let plan = build(&original, &mut scratch);
        scratch.recycle(plan);
        let mut changed = original.clone();
        changed.layers[0].primitives[0] =
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "skip"));
        changed.layers[0].primitives[1] = RenderPrimitive::Circle(RenderCircle::filled(
            RenderPoint::new(3., 4.),
            2.,
            RenderColor::GREEN,
        ));
        let RenderPrimitive::Polygon(polygon) = &mut changed.layers[0].primitives[2] else {
            panic!()
        };
        polygon.fill = None;
        polygon.stroke = None;
        changed.layers[0].primitives.truncate(4);
        for f in [&changed, &original] {
            let actual = build(f, &mut scratch);
            let expected = presentation::build(
                std::slice::from_ref(f),
                Viewport::new(640., 480.),
                FrameLayout::EqualHorizontal,
                1.,
            )
            .unwrap();
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            scratch.recycle(actual);
        }
        // Fail after overwriting an earlier draw, not just camera prevalidation.
        let RenderPrimitive::Polygon(polygon) = &mut changed.layers[0].primitives[3] else {
            panic!()
        };
        polygon.points[0].x = f32::NAN;
        let rejected = presentation::build_reusing(
            &[changed],
            Viewport::new(640., 480.),
            FrameLayout::EqualHorizontal,
            1.,
            true,
            PlanMode::Scratch,
            scratch.take(),
        );
        assert!(rejected.is_err());
        assert!(scratch.take().is_none());
        let recovered = build(&original, &mut scratch);
        assert_eq!(recovered.shapes(), 6);
    }
}
