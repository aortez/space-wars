//! Compatibility names for the restricted horizontal three-ball operation.
//! The scheduler and snapshot evidence are shared with `ball_arrival`.
pub use super::ball_arrival::{
    BallArrivalRejection as LineArrivalRejection, BallArrivalStep as LineArrivalStep,
    BallEpisodeEvent as LineEpisodeEvent, BallEventPhase as LineEventPhase,
    MAX_BALL_ADVANCES as MAX_LINE_ADVANCES,
};
