//! Simulation-owned direct illumination. See `docs/lighting.md` for units and limits.
use crate::{Color, Vec2};

pub const MAX_SOURCES: usize = 256;
pub const MAX_FOOTPRINT: usize = 8;
pub const MAX_RECEIVERS: usize = 64;
pub const MAX_AGE_TICKS: u64 = 1;
pub const FULL_SUN: f32 = 1000.0;
pub const ENDPOINT_TOLERANCE: f32 = 0.001;
pub const DIRECTIONAL_DISTANCE: f32 = 100_000.0;

/// Stable owner-scoped identity. Reused slots must encode a new lifetime;
/// owners reserve disjoint namespaces for physical and lightweight surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LightId(pub u64, pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmissionPoint {
    pub position: Vec2,
    /// Identity of the local emitting geometry, never an entire host.
    pub surface: Option<LightId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalSource {
    pub id: LightId,
    pub active: bool,
    pub color: Color,
    /// Total footprint energy in full-sun-relative units, shared equally by points.
    pub strength: f32,
    pub range: f32,
    pub footprint: Vec<EmissionPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalSource {
    /// Unit vector from receiver toward the overhead source.
    pub toward_source: Vec2,
    pub strength: f32,
    pub color: Color,
}

/// Vertical parallel sunlight through a finite horizontal opening. Bounds are
/// inclusive; receivers at or above the ceiling receive no aperture light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApertureSource {
    pub id: LightId,
    pub left: f32,
    pub right: f32,
    pub ceiling_y: f32,
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Receiver {
    pub position: Vec2,
    pub surface: Option<LightId>,
}

/// A finite segment with strictly local endpoint identities.
#[derive(Clone, Copy, Debug)]
pub struct LightRay {
    pub receiver: Receiver,
    pub endpoint: EmissionPoint,
}

/// Must query the same immutable geometry generation as the lighting snapshot.
/// None means visible; Some identifies an opaque blocker.
pub trait Visibility {
    fn blocker(&self, ray: LightRay) -> Option<LightId>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightingError {
    Capacity,
    InvalidInput,
    ConflictingIdentity,
    Stale,
    GeometryGeneration,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exposure {
    pub intensity: f32,
    pub directional: f32,
    pub local: f32,
    pub rays: usize,
    pub blocked_rays: usize,
    pub first_blocker: Option<LightId>,
}

/// Immutable publication; no render frame, camera, wall clock or graphics state.
#[derive(Clone, Debug)]
pub struct LightingSnapshot {
    tick: u64,
    geometry_generation: u64,
    directional: DirectionalSource,
    sources: Vec<LocalSource>,
    duplicate_copies: usize,
    aperture: Option<ApertureSource>,
}

fn finite(p: Vec2) -> bool {
    p.x.is_finite()
        && p.y.is_finite()
        && p.x.abs() <= DIRECTIONAL_DISTANCE
        && p.y.abs() <= DIRECTIONAL_DISTANCE
}
fn valid_color(c: Color) -> bool {
    [c.r, c.g, c.b, c.a]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
fn strength(s: f32) -> bool {
    s.is_finite() && (0.0..=FULL_SUN).contains(&s)
}

impl LightingSnapshot {
    /// Rejects over-capacity publications atomically instead of silently losing energy.
    /// Duplicate draw copies are removed by identity, with conflicting copies rejected.
    pub fn new(
        tick: u64,
        geometry_generation: u64,
        directional: DirectionalSource,
        sources: &[LocalSource],
    ) -> Result<Self, LightingError> {
        // Also bounds deduplication work and temporary memory.
        if sources.len() > MAX_SOURCES * MAX_FOOTPRINT {
            return Err(LightingError::Capacity);
        }
        if !finite(directional.toward_source)
            || (directional.toward_source.length_squared() - 1.0).abs() > 0.0001
            || !strength(directional.strength)
            || !valid_color(directional.color)
        {
            return Err(LightingError::InvalidInput);
        }
        let mut unique = std::collections::BTreeMap::new();
        for source in sources {
            if source.footprint.is_empty() || source.footprint.len() > MAX_FOOTPRINT {
                return Err(LightingError::Capacity);
            }
            if !strength(source.strength)
                || !valid_color(source.color)
                || !source.range.is_finite()
                || source.range <= 0.0
                || source.range > DIRECTIONAL_DISTANCE
                || source.footprint.iter().any(|p| !finite(p.position))
            {
                return Err(LightingError::InvalidInput);
            }
            if let Some(previous) = unique.get(&source.id) {
                if *previous != source {
                    return Err(LightingError::ConflictingIdentity);
                }
            } else {
                if unique.len() == MAX_SOURCES {
                    return Err(LightingError::Capacity);
                }
                unique.insert(source.id, source);
            }
        }
        let duplicate_copies = sources.len() - unique.len();
        Ok(Self {
            tick,
            geometry_generation,
            directional,
            sources: unique.into_values().cloned().collect(),
            duplicate_copies,
            aperture: None,
        })
    }

    pub fn with_aperture(mut self, aperture: ApertureSource) -> Result<Self, LightingError> {
        if !finite(Vec2::new(aperture.left, aperture.ceiling_y))
            || !finite(Vec2::new(aperture.right, aperture.ceiling_y))
            || aperture.left >= aperture.right
            || !strength(aperture.strength)
            || self.sources.iter().any(|s| s.id == aperture.id)
        {
            return Err(LightingError::InvalidInput);
        }
        if self.sources.len() == MAX_SOURCES {
            return Err(LightingError::Capacity);
        }
        self.aperture = Some(aperture);
        Ok(self)
    }

    pub fn aperture(&self) -> Option<ApertureSource> {
        self.aperture
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }
    pub fn sources(&self) -> &[LocalSource] {
        &self.sources
    }
    pub fn duplicate_copies(&self) -> usize {
        self.duplicate_copies
    }

    /// At most 64 * (1 + 256 * 8) visibility calls. No persistent query cache.
    pub fn query(
        &self,
        tick: u64,
        geometry_generation: u64,
        receivers: &[Receiver],
        visibility: &impl Visibility,
    ) -> Result<Vec<Exposure>, LightingError> {
        if tick < self.tick || tick - self.tick > MAX_AGE_TICKS {
            return Err(LightingError::Stale);
        }
        if geometry_generation != self.geometry_generation {
            return Err(LightingError::GeometryGeneration);
        }
        if receivers.len() > MAX_RECEIVERS {
            return Err(LightingError::Capacity);
        }
        if receivers.iter().any(|r| !finite(r.position)) {
            return Err(LightingError::InvalidInput);
        }
        Ok(receivers
            .iter()
            .map(|&receiver| {
                let mut result = Exposure::default();
                let mut sample = |endpoint: EmissionPoint, energy: f32| {
                    if energy == 0.0 {
                        return 0.0;
                    }
                    result.rays += 1;
                    if let Some(blocker) = visibility.blocker(LightRay { receiver, endpoint }) {
                        result.blocked_rays += 1;
                        result.first_blocker.get_or_insert(blocker);
                        0.0
                    } else {
                        energy
                    }
                };
                result.directional = sample(
                    EmissionPoint {
                        position: receiver.position
                            + self.directional.toward_source * DIRECTIONAL_DISTANCE,
                        surface: None,
                    },
                    self.directional.strength,
                );
                for source in self.sources.iter().filter(|s| s.active) {
                    for point in &source.footprint {
                        let distance = receiver.position.distance_to(point.position);
                        // Squared compact support is finite at contact and C1 at cutoff.
                        let falloff = (1.0 - distance / source.range).max(0.0).powi(2);
                        result.local += sample(
                            *point,
                            source.strength / source.footprint.len() as f32 * falloff,
                        );
                    }
                }
                if let Some(source) = self.aperture
                    && (source.left..=source.right).contains(&receiver.position.x)
                    && receiver.position.y < source.ceiling_y
                {
                    result.local += sample(
                        EmissionPoint {
                            position: Vec2::new(receiver.position.x, source.ceiling_y),
                            surface: Some(source.id),
                        },
                        source.strength,
                    );
                }
                result.intensity = (result.directional + result.local).min(FULL_SUN);
                result
            })
            .collect())
    }
}

#[cfg(test)]
mod lighting_contract;

/// Simulation-owned publication diagnostics. Rejected updates retain the old
/// immutable snapshot, whose age checks still apply; failures never mean darkness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightingCounters {
    pub published: u64,
    pub rejected: u64,
    pub capacity_rejections: u64,
}
#[derive(Default)]
pub struct LightingPublisher {
    current: Option<std::sync::Arc<LightingSnapshot>>,
    counters: LightingCounters,
}
impl LightingPublisher {
    pub fn snapshot(&self) -> Option<std::sync::Arc<LightingSnapshot>> {
        self.current.clone()
    }
    pub fn counters(&self) -> LightingCounters {
        self.counters
    }
    /// Call once at the completed simulation tick, after canonical query updates.
    /// Pause does not publish. Rewinding requires a new publisher/session.
    pub fn publish(
        &mut self,
        tick: u64,
        geometry_generation: u64,
        directional: DirectionalSource,
        sources: &[LocalSource],
    ) -> Result<(), LightingError> {
        let result = if self
            .current
            .as_ref()
            .is_some_and(|s| tick <= s.tick || geometry_generation < s.geometry_generation)
        {
            Err(LightingError::Stale)
        } else {
            LightingSnapshot::new(tick, geometry_generation, directional, sources)
        };
        match result {
            Ok(snapshot) => {
                self.current = Some(std::sync::Arc::new(snapshot));
                self.counters.published += 1;
                Ok(())
            }
            Err(error) => {
                self.counters.rejected += 1;
                if error == LightingError::Capacity {
                    self.counters.capacity_rejections += 1;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod ceiling_light_contract {
    use super::*;
    struct Clear;
    impl Visibility for Clear {
        fn blocker(&self, _: LightRay) -> Option<LightId> {
            None
        }
    }
    struct Block;
    impl Visibility for Block {
        fn blocker(&self, _: LightRay) -> Option<LightId> {
            Some(LightId(9, 9))
        }
    }
    #[test]
    fn ceiling_light_contract_bounds_occlusion_generations_and_caps() {
        let s = LightingSnapshot::new(
            10,
            20,
            DirectionalSource {
                toward_source: Vec2::Y,
                strength: 50.0,
                color: Color::WHITE,
            },
            &[],
        )
        .unwrap()
        .with_aperture(ApertureSource {
            id: LightId(5, 1),
            left: -10.0,
            right: 10.0,
            ceiling_y: 100.0,
            strength: 950.0,
        })
        .unwrap();
        let receivers: Vec<_> = [-10.01, -10.0, 0.0, 10.0, 10.01]
            .into_iter()
            .map(|x| Receiver {
                position: Vec2::new(x, 0.0),
                surface: None,
            })
            .collect();
        let e = s.query(10, 20, &receivers, &Clear).unwrap();
        assert_eq!(
            e.iter().map(|e| e.intensity).collect::<Vec<_>>(),
            [50.0, 1000.0, 1000.0, 1000.0, 50.0]
        );
        assert!(
            s.query(10, 20, &receivers, &Block)
                .unwrap()
                .iter()
                .all(|e| e.intensity == 0.0)
        );
        assert_eq!(
            s.query(12, 20, &receivers, &Clear),
            Err(LightingError::Stale)
        );
        assert_eq!(
            s.query(10, 21, &receivers, &Clear),
            Err(LightingError::GeometryGeneration)
        );
        assert_eq!(
            s.query(10, 20, &vec![receivers[0]; 65], &Clear),
            Err(LightingError::Capacity)
        );
    }
}
