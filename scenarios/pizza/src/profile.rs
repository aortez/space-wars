//! Bounded callers capture these after normal steps, outside simulation timing.
use super::*;
use serde::Serialize;

pub fn size_band(radius: f32) -> usize {
    if radius < 0.005 {
        0
    } else if radius < 0.01 {
        1
    } else {
        2
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub tick: u64,
    pub population: PizzaPopulation,
    pub sizes: [usize; 3],
    pub generations: [usize; 4],
    pub fingerprint: u64,
    pub event: PizzaEventState,
    pub step_us: f64,
    pub spawning_us: f64,
    pub gravity_us: f64,
    pub physics_us: f64,
    pub snapshot_us: f64,
    pub damage_us: f64,
    pub lifecycle_us: f64,
    pub contacts: usize,
    pub contact_pairs: usize,
    pub candidate_pairs: usize,
    pub capture_us: f64,
}

impl Sample {
    pub fn capture(state: &PizzaState) -> Self {
        let started = Instant::now();
        let mut sizes = [0; 3];
        let mut generations = [0; 4];
        let mut fingerprint = 0xcbf29ce484222325u64;
        let mut hash = |bytes: &[u8]| {
            for byte in bytes {
                fingerprint = (fingerprint ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        };
        for ball in &state.balls {
            sizes[size_band(ball.radius)] += 1;
            generations[usize::from(ball.fragment_generation).min(3)] += 1;
            hash(&ball.id.to_le_bytes());
            for value in [
                ball.position.x,
                ball.position.y,
                ball.velocity.x,
                ball.velocity.y,
                ball.hp,
                ball.radius,
                ball.color.r,
                ball.color.g,
                ball.color.b,
                ball.color.a,
            ] {
                hash(&value.to_bits().to_le_bytes());
            }
            hash(&[
                ball.fragment_generation,
                u8::from(ball.moving),
                u8::from(ball.invincible),
            ]);
        }
        let m = state.last_step_metrics;
        let us = |d: Duration| d.as_secs_f64() * 1e6;
        Self {
            tick: state.tick,
            population: state.population(),
            sizes,
            generations,
            fingerprint,
            event: state.event_state(),
            step_us: us(m.total_time),
            spawning_us: us(m.spawning_time),
            gravity_us: us(m.gravity_time),
            physics_us: us(m.physics_time),
            snapshot_us: us(m.snapshot_time),
            damage_us: us(m.damage_time),
            lifecycle_us: us(m.lifecycle_time),
            contacts: m.rapier.contacts,
            contact_pairs: m.rapier.contact_pairs,
            candidate_pairs: m.rapier.candidate_pairs,
            capture_us: us(started.elapsed()),
        }
    }
}
