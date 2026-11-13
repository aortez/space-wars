//! Collapse of the shared contact path when either finite layer is exhausted.
use super::*;

/// One layer's disposition when the shared contact path collapses.
/// An intact layer also releases its elastic store, without losing extra capacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialFailureDisposition {
    pub source: ContactSource,
    pub failed: bool,
    pub indentation: f64,
    pub plastic_indentation: f64,
    pub released_elastic: f64,
    pub excess_work: f64,
}
impl MaterialFailureDisposition {
    pub fn dissipated(&self) -> f64 {
        self.released_elastic + self.excess_work
    }
}

/// One contact-collapse boundary, with layers and motion in incoming/support order.
/// Only exhausted layers emit `CompressionFailure`; this record also accounts
/// for elastic release by a surviving layer. Motion may advance past this record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedCompressionFailure {
    pub elapsed: f64,
    pub episode: EpisodeId,
    pub indentation: f64,
    pub positions: [f64; 2],
    pub velocities: [f32; 2],
    pub layers: [MaterialFailureDisposition; 2],
}
impl PairedCompressionFailure {
    pub fn dissipated(&self) -> f64 {
        self.layers.iter().map(|l| l.dissipated()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct FailureHistory {
    sources: [ContactSource; 2],
    event: Option<PairedCompressionFailure>,
}

impl PairedHistory {
    pub fn enable_failure(&mut self, sources: [ContactSource; 2]) {
        self.failure = Some(FailureHistory {
            sources,
            event: None,
        });
    }
    pub fn failure_enabled(&self) -> bool {
        self.failure.is_some()
    }
    pub fn failure_event(&self) -> Option<&PairedCompressionFailure> {
        self.failure.as_ref()?.event.as_ref()
    }
    pub fn collapsed(&self) -> bool {
        self.failure_event().is_some()
    }
    pub fn released_elastic(&self) -> f64 {
        self.failure_event()
            .map_or(0.0, |e| e.layers.iter().map(|l| l.released_elastic).sum())
    }
    pub fn layer_excess(&self, i: usize) -> f64 {
        self.failure_event()
            .map_or(0.0, |e| e.layers[i].excess_work)
    }
    pub fn failure_excess(&self) -> f64 {
        self.layer_excess(0) + self.layer_excess(1)
    }
    pub fn committed_failures(&self) -> Vec<CompressionFailure> {
        let Some(e) = self.failure_event() else {
            return Vec::new();
        };
        let mut result: Vec<_> = e
            .layers
            .iter()
            .filter(|l| l.failed)
            .map(|l| CompressionFailure {
                elapsed: e.elapsed,
                source: l.source,
                episode: e.episode,
                indentation: l.indentation,
                velocity: f64::from(e.velocities[0]) - f64::from(e.velocities[1]),
                plastic_indentation: l.plastic_indentation,
                released_elastic: l.released_elastic,
                excess_work: l.excess_work,
            })
            .collect();
        result.sort_by_key(|e| e.source);
        result
    }
    pub(super) fn commit_failure(
        &mut self,
        elapsed: f64,
        episode: EpisodeId,
        indentation: f64,
        motion: &moving::Motion,
        excess: [f64; 2],
    ) {
        let sources = self.failure.as_ref().unwrap().sources;
        let layers = std::array::from_fn(|i| {
            let l = &self.layers[i];
            MaterialFailureDisposition {
                source: sources[i],
                failed: l.remaining == 0.0,
                indentation: l.x,
                plastic_indentation: l.p,
                released_elastic: MaterialCurve::new(&self.initial[i]).energy(l.x, l.p),
                excess_work: excess[i],
            }
        });
        self.failure.as_mut().unwrap().event = Some(PairedCompressionFailure {
            elapsed,
            episode,
            indentation,
            positions: motion.positions,
            velocities: motion.velocities,
            layers,
        });
    }
    pub fn failure_sources_valid(&self, world: &PhysicsWorld) -> bool {
        let Some(f) = &self.failure else { return true };
        let Some(l) = &world.loading else {
            return false;
        };
        f.sources
            .iter()
            .all(|s| s.region.incarnation < l.next_region)
            && f.event
                .as_ref()
                .is_none_or(|e| e.episode.0 < l.next_episode)
    }
    pub fn failure_sources_match(&self, sources: [ContactSource; 2]) -> bool {
        self.failure.as_ref().is_none_or(|f| f.sources == sources)
    }
    pub(super) fn valid_failure(&self, h: &History) -> bool {
        let Some(f) = &self.failure else { return true };
        if h.failure.is_some()
            || f.sources[0].region.id.entity == f.sources[1].region.id.entity
            || f.sources.iter().any(|s| s.region.incarnation == 0)
        {
            return false;
        }
        let Some(e) = &f.event else { return true };
        let Some(moving) = &h.moving else {
            return false;
        };
        let depth = self.curve().depth;
        let motion = moving::Motion {
            positions: e.positions,
            velocities: e.velocities,
        };
        e.elapsed.is_finite()
            && e.elapsed > 0.0
            && e.elapsed <= h.summary.elapsed
            && e.episode.0 > 0
            && (0.0..=depth / 4.0).contains(&e.indentation)
            && (e.indentation - self.layers.iter().map(|l| l.x).sum::<f64>()).abs() <= 1e-10 * depth
            && moving.valid_failure(
                &motion,
                e.indentation,
                motion.relative_velocity(),
                h.summary.elapsed - e.elapsed,
                depth,
            )
            && e.layers.iter().any(|l| l.failed)
            && (0..2).all(|i| {
                let l = &e.layers[i];
                let state = &self.layers[i];
                let c = MaterialCurve::new(&self.initial[i]);
                l.source == f.sources[i]
                    && l.failed == (state.remaining == 0.0)
                    && l.indentation == state.x
                    && l.plastic_indentation == state.p
                    && l.released_elastic == c.energy(state.x, state.p)
                    && l.released_elastic.is_finite()
                    && l.released_elastic >= 0.0
                    && (!l.failed || l.released_elastic > 0.0)
                    && (0.0..=c.y * c.depth / 4.0).contains(&l.excess_work)
                    && (l.failed || l.excess_work == 0.0)
            })
    }
    pub fn serialize_failure(&self, bytes: &mut Vec<u8>) -> Result<(), PhysicsWorldError> {
        let f = self
            .failure
            .as_ref()
            .ok_or_else(|| PhysicsWorldError("missing paired collapse history".into()))?;
        bincode::serialize_into(bytes, f).map_err(|e| PhysicsWorldError(e.to_string()))
    }
    pub fn deserialize_failure(
        &mut self,
        reader: &mut impl std::io::Read,
    ) -> Result<(), PhysicsWorldError> {
        self.failure =
            Some(bincode::deserialize_from(reader).map_err(|e| PhysicsWorldError(e.to_string()))?);
        Ok(())
    }
}

impl PhysicsWorld {
    /// Opt into collapse of two finite layers on free bodies with zero gravity.
    /// Uses paired-compression admission, subdivision and numerical limits.
    /// Both layers resist through the first interval exhausting either capacity;
    /// then both elastic stores and any excess plastic work are dissipated. Each
    /// exhausted layer emits one failure in source order, followed by no further
    /// reaction or damage. Surviving capacity and both permanent sets persist.
    /// Both bodies coast until the existing motion or numerical limits refuse.
    /// Snapshot v23 retains the policy and boundary after invalidation/removal.
    /// Calls are atomic, and enrolled histories cannot switch failure policies.
    pub fn step_material_collapsing_pair(&mut self, dt: f32) -> Result<DynamicCompressionStep, R> {
        step::advance(self, dt, super::super::failure::Policy::Pair)
    }

    /// The retained contact collapse, including the disposition of an intact
    /// layer's elastic energy and both bodies' motion at the internal boundary.
    pub fn material_paired_compression_failure(&self) -> Option<PairedCompressionFailure> {
        self.loading
            .as_ref()?
            .dynamic_compression
            .as_ref()?
            .history
            .paired
            .as_ref()?
            .failure_event()
            .cloned()
    }
}

#[cfg(test)]
#[path = "../tests/paired_failure.rs"]
mod tests;
