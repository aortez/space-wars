//! Diagnostic envelopes, not a route certificate or an action permission.
use super::*;
use engine_rapier::world::{AreaValidation, RegionChanges};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagSurveyEnvelope {
    pub name: &'static str,
    pub minimum: Vec2,
    pub maximum: Vec2,
}
impl FlagSurveyEnvelope {
    fn new(name: &'static str, points: impl IntoIterator<Item = Vec2>, margin: f32) -> Self {
        let mut points = points.into_iter();
        let first = points.next().unwrap();
        let mut result = Self {
            name,
            minimum: first,
            maximum: first,
        };
        for point in points {
            result.minimum.x = result.minimum.x.min(point.x);
            result.minimum.y = result.minimum.y.min(point.y);
            result.maximum.x = result.maximum.x.max(point.x);
            result.maximum.y = result.maximum.y.max(point.y);
        }
        let margin = Vec2::new(margin + 0.004, margin + 0.004);
        result.minimum -= margin;
        result.maximum += margin;
        result
    }
    fn area(&self) -> QueryArea {
        QueryArea {
            minimum: self.minimum,
            maximum: self.maximum,
            // Broad relevance only: individual queries can have narrower filters.
            groups: CollisionGroups::ALL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagSurveyGeometry {
    pub model: &'static str,
    pub material_queries_dirty: bool,
    pub acceptance_prefix: AreaValidation,
    pub envelopes: Vec<FlagSurveyEnvelope>,
    pub report: RegionChanges,
    pub entity_kinds: BTreeMap<u64, &'static str>,
}

pub(super) fn envelopes(
    measurement: &CoverMeasurement,
    objective: LandingObjective,
    radius: f32,
    hull_radius: f32,
) -> Vec<FlagSurveyEnvelope> {
    let site = measurement.site.unwrap();
    let frame = measurement.planet;
    let local = |point: Vec2| (point - frame.position).rotate_radians(-frame.angle);
    let center = local(site.vehicle_position);
    let normal = site.normal.rotate_radians(-frame.angle);
    let spec = SurfaceSortieState::spec();
    let up = Vec2::Y.rotate_radians(f32::from(site.id.bearing) * std::f32::consts::TAU / 64.0);
    let right = Vec2::new(up.y, -up.x);
    // Full foot rays, including any depth beyond the first retained hit, plus
    // the belly ray. These broad boxes do not identify the deciding ray.
    let mut landing = vec![center, center - normal * 8.0];
    for offset in [-3.0, 3.0] {
        let origin = up * (radius + 10.0) + right * offset;
        landing.extend([origin, origin - up * 35.0]);
    }
    let mut areas = vec![
        FlagSurveyEnvelope::new("landing_material", landing, 0.0),
        FlagSurveyEnvelope::new("landing_hull", [center], hull_radius + 0.95),
        // Rotation-independent envelope for both hatches: ±0.75 drift, ±1
        // floor search, rays from +2 to -3, both capsule axes and ±0.1 tilt.
        FlagSurveyEnvelope::new(
            "boarding_probe_envelope",
            [center],
            hatch_offset(ShipForm::Ship, 0).length()
                + 0.75
                + 1.0
                + 3.0
                + 2.0 * spec.half_height()
                + 0.12,
        ),
    ];
    for (name, height) in [("climb_7", 7.0), ("climb_30", 30.0), ("climb_60", 60.0)] {
        areas.push(FlagSurveyEnvelope::new(
            name,
            [center + normal * height],
            hull_radius,
        ));
    }
    let count = ground_navigation::GROUND_SAMPLES as u16;
    let bearing = (((-objective.position.x)
        .atan2(objective.position.y)
        .rem_euclid(std::f32::consts::TAU)
        * f32::from(count)
        / std::f32::consts::TAU)
        .round() as u16)
        % count;
    // The entire sampled patch, including unsuccessful rays to the center and
    // unused edges. It must never be described as the selected walking path.
    let patch = std::iter::once(Vec2::ZERO).chain((0..=2 * PATCH_HALF_WIDTH).map(|i| {
        let id = (bearing + count - PATCH_HALF_WIDTH + i) % count;
        Vec2::Y.rotate_radians(f32::from(id) * std::f32::consts::TAU / f32::from(count))
            * (radius + 8.0)
    }));
    areas.push(FlagSurveyEnvelope::new(
        "walking_patch",
        patch,
        2.0 * spec.half_height() + 1.0,
    ));
    // Only a spatial hint for the separate body-center proximity rule. Collider
    // overlap here does not evaluate that non-query landing predicate.
    areas.push(FlagSurveyEnvelope::new(
        "vehicle_proximity_envelope",
        [center],
        16.0,
    ));
    areas
}

pub(super) fn diagnose(
    state: &SurfaceSortieState,
    pending: &Pending,
    region: QueryRegion<'_>,
    acceptance_prefix: AreaValidation,
) -> FlagSurveyGeometry {
    let areas: Vec<_> = pending
        .envelopes
        .iter()
        .map(FlagSurveyEnvelope::area)
        .collect();
    let report = pending
        .snapshot
        .diagnose_region(&state.world.physics.world, region, &areas);
    let mut entity_kinds = BTreeMap::new();
    for changed in &report.changes {
        for pose in changed.previous.iter().chain(&changed.current) {
            let Some(id) = pose.collider else { continue };
            let kind = match physics::classify_entity(id.entity) {
                Some(MechanicalEntity::World) => "arena boundary",
                Some(MechanicalEntity::Body(BodyId::Sun)) => "sun",
                Some(MechanicalEntity::Body(BodyId::Planet(_))) => "planet",
                Some(MechanicalEntity::TerrainFragment(_)) => "terrain fragment",
                Some(MechanicalEntity::Ship(_)) => "ship or pod",
                Some(MechanicalEntity::Spaceling(_)) => "spaceling",
                Some(MechanicalEntity::Rover(_)) => "rover",
                Some(MechanicalEntity::Debris(_)) => "debris",
                None => "unclassified",
            };
            entity_kinds.insert(id.entity.value(), kind);
        }
    }
    FlagSurveyGeometry {
        model: "source_envelope_overlaps_v1",
        material_queries_dirty: state.world.physics.material_queries_dirty,
        acceptance_prefix,
        envelopes: pending.envelopes.clone(),
        report,
        entity_kinds,
    }
}
