//! Bounded three-role history with separately retained recoil corrections.
use super::*;

pub(in crate::world::loading::response) mod phase;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct State {
    // Kept outside Status so invalidation cannot discard or renew either total.
    corrections: Corrections,
    status: StackStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Corrections {
    // Signed sum of mB*vB and nonnegative sum of 0.5*mB*vB^2.
    impulse: f64,
    work: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum StackStatus {
    Active(Box<StackActive>),
    Invalidated(Invalidation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StackActive {
    // Geometric A/B/C order, independent of canonical source order.
    plates: [Plate; 3],
    capture: CaptureConfig,
    integration: IntegrationParameters,
    support: SupportOwner,
    incoming: Option<OwnedEpisode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SupportOwner {
    sources: [ContactSource; 2],
    id: EpisodeId,
}

impl State {
    pub(super) fn correction_summary(&self, history: &History) -> GravityCorrectionSummary {
        GravityCorrectionSummary {
            impulse: self.corrections.impulse,
            dissipated: self.corrections.work,
            impulse_bound: 1.0e-5 * history.scales[1],
            energy_bound: 1.0e-5 * history.scales[0],
        }
    }

    pub(super) fn invalidation(&self) -> Option<Invalidation> {
        match &self.status {
            StackStatus::Active(_) => None,
            StackStatus::Invalidated(reason) => Some(*reason),
        }
    }

    pub(super) fn invalidate(&mut self, reason: Invalidation) {
        if matches!(self.status, StackStatus::Active(_)) {
            self.status = StackStatus::Invalidated(reason);
        }
    }

    pub(super) fn valid_for(&self, loading: &LoadingState, history: &History) -> bool {
        let c = &self.corrections;
        c.impulse.is_finite()
            && c.work.is_finite()
            && c.work >= 0.0
            && c.impulse.abs() <= 1.0e-5 * history.scales[1]
            && c.work <= 1.0e-5 * history.scales[0]
            && (c.impulse == 0.0 || c.work > 0.0)
            && match &self.status {
                StackStatus::Invalidated(_) => true,
                StackStatus::Active(active) => active.valid_for(loading, history),
            }
    }
}

impl StackActive {
    fn valid_for(&self, loading: &LoadingState, history: &History) -> bool {
        let [a, b, c] = &self.plates;
        let mut support = [b.source, c.source];
        support.sort();
        valid_contract(loading, &self.plates, &self.capture, &self.integration)
            && valid_masses(history.revision, &[a.endpoint.mass, b.endpoint.mass])
            && f64::from(a.endpoint.mass) == history.reference[2]
            && a.endpoint.position[0] <= 1.0e-6
            && b.endpoint.position == [1.0, 0.0]
            && b.endpoint.velocity == [0.0, 0.0]
            && c.endpoint.position == [2.0, 0.0]
            && c.endpoint.velocity == [0.0, 0.0]
            && self.support.sources == support
            && loading
                .episodes
                .get(&support)
                .is_some_and(|e| e.id == self.support.id && e.absent_seconds == 0.0)
            && loading.episodes.len() == 1 + usize::from(self.incoming.is_some())
            && self.incoming.as_ref().is_none_or(|e| {
                e.id != self.support.id
                    && e.valid_for(loading, [a.source, b.source], &a.endpoint)
                    && loading.episodes[&e.sources].absent_seconds == 0.0
                    && (e.settled_work == 0.0
                        || history.settling == GravitySettling::BoundedNumerical)
            })
    }
}

#[cfg(test)]
mod tests;
