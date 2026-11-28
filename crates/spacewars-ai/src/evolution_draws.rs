use sha2::{Digest, Sha256};

/// Keyed counters pin the draw algorithm without serializing an RNG library's
/// internal layout. Each candidate has its own stream, independent of scheduling.
pub struct EvolutionDraws {
    seed: u64,
    generation: u64,
    candidate: u64,
    domain: &'static [u8],
    counter: u64,
}
impl EvolutionDraws {
    pub fn new(seed: u64, generation: usize, candidate: usize, domain: &'static [u8]) -> Self {
        Self {
            seed,
            generation: generation as u64,
            candidate: candidate as u64,
            domain,
            counter: 0,
        }
    }
    fn next(&mut self) -> u64 {
        let mut hash = Sha256::new();
        hash.update(b"spacewars.evolution.draw.v1\0");
        hash.update(self.domain);
        for value in [self.seed, self.generation, self.candidate, self.counter] {
            hash.update(value.to_le_bytes());
        }
        self.counter += 1;
        u64::from_le_bytes(hash.finalize()[..8].try_into().expect("eight digest bytes"))
    }
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
    pub fn index(&mut self, count: usize) -> usize {
        (self.next() % count as u64) as usize
    }
    /// Iterate in the genome format's canonical parameter order.
    pub fn mutate<'a>(
        &mut self,
        parameters: impl Iterator<Item = &'a mut f32>,
        probability: f32,
        scale: f32,
        limit: f32,
    ) {
        for value in parameters {
            if self.unit() < probability {
                *value = (*value + self.signed() * scale).clamp(-limit, limit);
            }
        }
    }
}
