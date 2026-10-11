//! Reuse existing geometry-lab counters in the bounded live KMS frame trace.
//! No extra clocks/census outside an explicit start-geometry capture.
use super::{encode_profile::EncodeProfile, retained::Stats};
use i_slint_backend_linuxkms::profiling::geometry::{
    self, Count as C, DetailCount as D, DetailTiming as T, Sample, Timing,
};
use std::time::Duration;

pub(super) fn sample(
    draws: usize,
    vertices: usize,
    indices: usize,
    stats: &Stats,
    encode: Duration,
    profile: Option<&EncodeProfile>,
) -> Sample {
    let mut sample = Sample::default();
    for (counter, value) in [
        (C::Passes, 1),
        (C::SourceDraws, draws as u64),
        (C::Vertices, vertices as u64),
        (C::Indices, indices as u64),
        (C::CacheHits, stats.hits),
        (C::CacheMisses, stats.misses),
        (C::CacheAdmissions, stats.admissions),
        (C::CacheEvictions, stats.evictions),
        (C::ScaleMisses, stats.scale_misses),
        (C::GuardedPolygons, stats.zoom_fallback_polygons),
        (C::CulledIndices, stats.culled_indices),
        (C::StrokeRejections, stats.stroke_bound_rejections),
    ] {
        sample.count(counter, value);
    }
    sample.time(Timing::Encode, encode);
    sample.time(Timing::CacheBuild, Duration::from_nanos(stats.build_ns));
    if let Some(p) = profile {
        sample.detail(
            &[
                (D::ExactVertices, p.exact_vertices),
                (D::ScalableVertices, p.scalable_vertices),
                (D::ScalableRanges, p.scalable_ranges),
                (D::FallbackFills, p.fallback_fills),
                (D::FallbackStrokes, p.fallback_strokes),
                (D::OrdinaryDraws, p.ordinary_draws),
                (D::StrokePoints, p.guarded_stroke.points),
                (D::StrokeVertices, p.guarded_stroke.vertices),
                (D::StrokeIndices, p.guarded_stroke.indices),
            ],
            &[
                (T::Resolve, p.resolve),
                (T::ExactExpand, p.exact_expand),
                (T::ScalableExpand, p.scalable_expand),
                (T::FallbackFill, p.fallback_fill),
                (T::FallbackStroke, p.fallback_stroke),
                (T::Ordinary, p.ordinary),
                (T::Validate, p.validate),
                (T::StrokePrepare, p.guarded_stroke.prepare),
                (T::StrokeTessellate, p.guarded_stroke.stroke),
                (T::StrokeFinish, p.guarded_stroke.finish),
                (T::Visibility, p.stroke_visibility),
            ],
        );
    }
    sample
}

pub(super) use geometry::{capture_mode, record};
