//! Optional fixed-width step attribution. The ordinary instantiation has a
//! zero-sized no-op policy: no clock, counter, allocation, or runtime switch.

/// Disjoint regions of a fixed-width step, not a full tessellation partition.
/// Setup, merging, path decoding, caps and closing bookkeeping remain outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeStage {
    JoinGeometry,
    BaseVertices,
    EdgeTriangles,
    JoinExtras,
    FlattenedStep,
}

/// Diagnostic policy; the closure must be called exactly once and its result
/// returned unchanged. In particular, an output error must not skip cleanup.
pub trait StrokeProfiler {
    fn measure<R>(&mut self, stage: StrokeStage, work: impl FnOnce() -> R) -> R;
}

#[derive(Default)]
pub struct NoStrokeProfile;

impl StrokeProfiler for NoStrokeProfile {
    #[inline(always)]
    fn measure<R>(&mut self, _: StrokeStage, work: impl FnOnce() -> R) -> R {
        work()
    }
}

impl<P: StrokeProfiler> StrokeProfiler for &mut P {
    #[inline(always)]
    fn measure<R>(&mut self, stage: StrokeStage, work: impl FnOnce() -> R) -> R {
        (**self).measure(stage, work)
    }
}
