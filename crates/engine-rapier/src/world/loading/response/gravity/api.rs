//! Public reports for the admitted two-plate gravity operation.
use super::*;

/// Maximum positive flight/held intervals in one call, including retained tails.
pub const MAX_GRAVITY_INTERVALS: usize = 8;
/// Maximum zero-duration material impact solves in one call.
pub const MAX_GRAVITY_IMPACTS: usize = 8;
/// Two projections per held interval, plus at most one numerical settlement.
pub const MAX_GRAVITY_PROJECTIONS: usize = 17;
/// Only fresh stationary contact needs an initial support capture.
pub const MAX_GRAVITY_SUPPORT_CAPTURES: usize = 1;

/// A discarded transaction. Refusal preserves the entire world and prior outputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GravityRejection {
    /// The requested duration is negative or nonfinite.
    InvalidTimestep,
    /// Positive intervals or their exact elapsed/remainder sums cannot be represented.
    TimePrecision,
    /// The positive-interval cap was reached before consuming the requested time.
    AdvanceLimit,
    /// An impact, projection or initial-support solve would exceed its cap.
    SolveLimit,
    /// An End, Support or Settlement record would exceed the capture record limit.
    RecordLimit,
    /// Stationary support is unsafe or outside the sustained material-law bounds.
    UnsafeSupport,
    /// Strict policy cannot settle an approaching contact still owned by an episode.
    SettlingDisabled,
    /// Retained evidence does not prove an eligible, contained excursion.
    AmbiguousDeparture,
    /// A signed cumulative flight defect would exceed its initial fixed allowance.
    NumericalAllowance {
        axis: GravityErrorAxis,
        attempted: f64,
        bound: f64,
    },
    /// Loading capture or material response is not enabled.
    ResponseDisabled,
    /// The complete world does not fit the admitted two-plate scene.
    UnsupportedScene,
    /// Saved ownership/state no longer matches, or an external edit/mode invalidated it.
    StaleContinuation,
    /// A positive call selected a different policy from the saved continuation.
    PolicyChanged,
    /// A region was already failed or failed during a provisional impact.
    FailedRegion,
    /// Rounded motion, measured reaction or work failed its numerical checks.
    Numerical,
    /// A touching approach did not produce the required accounted material response.
    UnaccountedApproach,
    /// A material impact solve would replace an existing episode owner.
    ExistingEpisode,
    /// The disposable snapshot transaction could not be created.
    Snapshot,
    /// Capture or material response refused a provisional batch.
    Response(ResponseRejection),
}

/// Components of the persistent numerical ledger, in array order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GravityErrorAxis {
    Work,
    Impulse,
    Position,
}

/// Copy of the retained numerical ledger; changing this report cannot reset it.
#[derive(Debug, Clone, PartialEq)]
pub struct GravityNumericalSummary {
    /// Initial energy, momentum and distance scales, fixed across all episodes.
    pub scales: [f64; 3],
    /// Signed cumulative work, impulse and position defects; each is bounded by
    /// `1e-5 * scales[i]`. These are numerical measurements, never material charges.
    pub errors: [f64; 3],
}

/// One certified ballistic interval using rounded kicks and force-free backend drift.
#[derive(Debug, Clone, PartialEq)]
pub struct GravityFlight {
    pub duration: f32,
    /// Weight times the actual signed displacement.
    pub gravity_work: f64,
    pub kinetic_change: f64,
    /// Work minus kinetic change, weight*time minus momentum change, and actual
    /// displacement minus ballistic displacement, respectively.
    pub errors: [f64; 3],
    /// Ballistic endpoint in the admitted frame (touching dynamic center at x=0).
    pub predicted_x: f64,
    /// Drift bound plus measured midpoint-velocity rounding, in length units.
    pub uncertainty: f64,
    /// Upper bound on the separation over this interval, including any apex.
    pub gap_upper: f64,
}

/// Checked zero-duration material response at a physical arrival.
#[derive(Debug, Clone, PartialEq)]
pub struct GravityImpact {
    /// Actual signed x velocity before/after the solve, without a gravity kick.
    pub incoming: f32,
    pub outgoing: f32,
    pub event: EpisodeEvent,
    /// Index in this call's [`PhysicsWorld::material_response_batches`].
    pub response_batch: usize,
    /// Owned copy of that batch's successfully committed response.
    pub response: AppliedResponse,
}

/// Explicit numerical sink for one contained return in its original episode.
/// This is neither material damage nor calibrated damping.
#[derive(Debug, Clone, PartialEq)]
pub struct GravitySettlement {
    pub episode: EpisodeId,
    /// Retained post-impact energy allowance, never renewed by settlement.
    pub available: f64,
    /// Actual kinetic energy removed, including the reported rounding correction.
    pub dissipated: f64,
    /// Measured backend reaction magnitude; excludes the final velocity correction.
    pub impulse: f64,
    /// Signed reaction correction `mass * residual_velocity` required for exact rest.
    pub rounding_impulse: f64,
    /// Energy removed by that final correction (already included in dissipated).
    pub rounding_loss: f64,
}

/// Safe rigid support with zero physical work and no material-capacity charge.
#[derive(Debug, Clone, PartialEq)]
pub struct GravitySupport {
    pub duration: f32,
    pub episode: EpisodeId,
    /// Actual weight, checked against both materials' safe force.
    pub force: f64,
    /// Sum of the two measured backend projection reactions.
    pub impulse: f64,
    /// Signed reaction corrections required for exact rest after the projections.
    pub rounding_impulse: f64,
    /// Correction energy, already included in projection_energy.
    pub rounding_loss: f64,
    /// Zero: accepted hold never changes the bodies' positions.
    pub gravity_work: f64,
    /// Auxiliary energy introduced by the two rounded gravity half-kicks.
    pub kick_energy: f64,
    /// Equal to kick_energy; this auxiliary loss is not physical material work.
    pub projection_energy: f64,
}

/// Physical/accounting payload in the step's single execution-ordered timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum GravityRecord {
    Flight(GravityFlight),
    Episode(EpisodeEvent),
    Impact(GravityImpact),
    Settlement(GravitySettlement),
    Support(GravitySupport),
}

/// Stable identities and location of one timeline record.
#[derive(Debug, Clone, PartialEq)]
pub struct GravityTimedRecord {
    /// Frame-relative seconds: flight/End use the interval endpoint, Support
    /// uses its start, and instantaneous contact/settlement uses the solve time.
    pub at: f64,
    /// Index into [`GravityStep::advances`]. Support refers to its first
    /// projection, followed by stationary drift and the second projection.
    pub phase: usize,
    /// Dynamic then fixed role order, independent of source or insertion order.
    pub bodies: [BodyId; 2],
    /// Same role order as bodies. Episode payloads instead use canonical pair order.
    pub sources: [ContactSource; 2],
    pub record: GravityRecord,
}

/// Committed results for one complete gravity call. Compare records, durations
/// and numerical summaries for replay; backend timings are observational.
#[derive(Debug, Clone, Default)]
pub struct GravityStep {
    /// Positive flight/held durations whose exact sum equals the requested f32
    /// time. Instantaneous solves consume no duration; pause returns an empty list.
    pub durations: Vec<f32>,
    /// Execution order: capture transition before Impact/initial Support;
    /// Flight before proved End; Settlement before subsequent held intervals.
    pub records: Vec<GravityTimedRecord>,
    /// None only when pausing a world that has never enrolled. Pause also reports
    /// retained errors for an invalidated continuation without making it resumable.
    pub numerical: Option<GravityNumericalSummary>,
    /// One measurement per backend drift/solve, including zero-duration work.
    pub advances: Vec<PhysicsStepMetrics>,
    /// Complete call time, including admission, snapshots and validation.
    pub wall_time: Duration,
}

impl PhysicsWorld {
    /// Advance an admitted two-plate scene with constant normal gravity, material
    /// impacts and safe rigid support. The settling policy is explicit and is
    /// frozen by the first successful positive call.
    ///
    /// Requires exactly two axis-aligned plates of half-extents (0.5, 1): one
    /// dynamic body of refreshed mass 20, rotation locked and sleep disabled,
    /// and one fixed body centered at (1, 0). Both have zero tangent/angular
    /// motion. The dynamic center is at x<=1e-6, y=0; gravity is (a, 0), a>0,
    /// with scale 1. Each body uses one/two/four uniform tangent slices and
    /// one/two coincident copies, all bound to its distinct live material region.
    /// Capture's touch tolerance must be 1e-5. No other force/torque, friction,
    /// damping, joints, CCD/soft CCD, skin, sensor, custom filter or extra body.
    /// Failed materials and support overload refuse; failure consequences and
    /// physical overloaded deformation are outside this operation.
    ///
    /// Success consumes dt exactly, including positive tails and arrivals at
    /// the frame endpoint. Every refusal preserves the original world and
    /// previous outputs; there is no ordinary-physics fallback. Work/record
    /// limits and numerical bounds can refuse otherwise admitted scenes.
    ///
    /// dt=0 is a pause: clear ephemeral outputs without enrolling, solving,
    /// changing policy, or resetting history. Invalid time preserves outputs.
    /// Positive enrollment persists in snapshot v6. Call this operation again
    /// after restore; ordinary positive stepping and external edits invalidate
    /// continuation permanently while retaining its numerical ledger.
    ///
    /// # Example
    ///
    /// ```
    /// use engine_rapier::world::{PhysicsWorld, loading::response::gravity::{
    ///     GravityRecord, GravityRejection, GravitySettling,
    /// }};
    ///
    /// fn advance(world: &mut PhysicsWorld, dt: f32) -> Result<(), GravityRejection> {
    ///     let step = world.step_material_flat_gravity(dt, GravitySettling::Refuse)?;
    ///     for entry in &step.records {
    ///         if let GravityRecord::Impact(impact) = &entry.record {
    ///             let batch = &world.material_response_batches()[impact.response_batch];
    ///             assert_eq!(batch.result.as_ref().unwrap(), &impact.response);
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn step_material_flat_gravity(
        &mut self,
        dt: f32,
        settling: GravitySettling,
    ) -> Result<GravityStep, GravityRejection> {
        phase::schedule::step(self, dt, settling)
    }
}
