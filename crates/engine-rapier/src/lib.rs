//! Rapier-backed mechanics for physical Spacewars scenarios.
//!
//! [`world::PhysicsWorld`] is the sole owner of raw Rapier state. Scenario code
//! supplies stable IDs, gameplay intent, kinematic targets, and external force
//! fields, then reads authoritative motion and normalized events.

pub mod rover;
pub mod world;
