//! Shared vector/raster primitives derived from the physiological sunlight query.
use super::*;
use crate::grass::residents::OpticalScene;
use engine_core::lighting::{LightId, LightingError, Receiver};
use engine_rapier::world::{ColliderId, QuerySnapshot};

mod beam;

/// Falling parcels draw above the face labels and below the wooden frame.
const PARCEL_LAYER: i32 = 6;

/// Host geometry copied once at the completed simulation boundary: canonical
/// solids, transparent soil identities, and the face/actor/soil/tissue
/// polygons that physiology also treats as opaque.
pub(crate) struct HostOptics {
    geometry: Option<QuerySnapshot>,
    transparent: Vec<ColliderId>,
    polygons: Vec<(LightId, Vec<Vec2>)>,
}
impl HostOptics {
    pub(crate) fn capture(state: &ClockState) -> Self {
        let world = crate::arena::canonical_world(&state.active_event, state.duck_visit.as_deref());
        let mut quads = plant_face_polygons(state);
        quads.extend(plant_actor_polygons(state));
        if let Some(soil) = &state.soil {
            quads.extend(soil.quads().into_iter().map(|q| q.to_vec()));
        }
        let mut polygons = crate::grass::residents::optical_polygons(quads);
        if let Some(soil) = &state.soil {
            polygons.extend(crate::grass::residents::tissue_polygons(
                &state.plants.plants.observe(),
                soil,
            ));
        }
        Self {
            geometry: world.map(|w| w.query_snapshot()),
            transparent: world
                .into_iter()
                .flat_map(|w| w.collider_ids())
                .filter(|id| id.entity.value() == crate::grass::SOIL_PHYSICS_ID)
                .collect(),
            polygons,
        }
    }
    pub(crate) fn scene(&self) -> OpticalScene<'_> {
        OpticalScene {
            footprints: &[],
            polygons: &self.polygons,
            geometry: self.geometry.as_ref(),
            transparent: &self.transparent,
        }
    }
    /// Material/tissue polygons plus canonical silhouettes from the same copy.
    fn silhouettes(&self) -> Result<Vec<Vec<Vec2>>, LightingError> {
        let mut silhouettes: Vec<_> = self.polygons.iter().map(|(_, p)| p.clone()).collect();
        if let Some(geometry) = &self.geometry {
            silhouettes.extend(geometry.lighting_occluder_silhouettes(&self.transparent)?);
        }
        Ok(silhouettes)
    }
}

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    if state.ceiling.phase == crate::ceiling::Phase::Opening {
        let progress = state.ceiling.fraction();
        let openings = state.ceiling.sources(layout);
        let span = crate::ceiling::Ceiling::span(layout);
        // Each parcel falls from the opening it is nearest, not from one
        // shared point, so several holes read as several deliveries.
        let origin = |x: f32| {
            openings
                .iter()
                .map(|a| (a.left + a.right) * 0.5)
                .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
                .unwrap_or((span.0 + span.1) * 0.5)
        };
        // Each displayed parcel references an owned pending site. No decorative
        // rain can imply unbudgeted material; cancellation clears these sites.
        for site in state
            .ceiling
            .material
            .iter()
            .step_by((state.ceiling.material.len() / 32).max(1))
            .take(32)
        {
            let target = (site.min + site.max) * 0.5;
            let center = origin(target.x);
            let x = center + (target.x - center) * progress;
            let y = layout.canopy_y + (target.y - layout.canopy_y) * progress * progress;
            frame.push_primitive(
                PARCEL_LAYER,
                rectangle(
                    RenderPoint::new(x - 0.8, y - 0.8),
                    RenderPoint::new(x + 0.8, y + 0.8),
                    super::SOIL_COLOR,
                    None,
                ),
            );
        }
    }
    if !state.config.ceiling_sunlight {
        return;
    }
    let sources = state.ceiling.sources(layout);
    if sources.is_empty() {
        return;
    }
    let optics = HostOptics::capture(state);
    // Validate the same bounded publication as physiology before drawing any
    // light. Capacity or invalid-input refusal draws no unverified beam.
    let probes: Vec<_> = sources
        .iter()
        .map(|source| Receiver {
            position: Vec2::new((source.left + source.right) * 0.5, layout.floor_y),
            surface: None,
        })
        .collect();
    let validation = optics
        .scene()
        .query_light(state.simulation_tick(), 0, &sources, &probes);
    if validation.is_err() {
        return;
    }
    let Ok(silhouettes) = optics.silhouettes() else {
        return;
    };
    for source in &sources {
        let Some(beam) = beam::polygons(*source, layout.floor_y, &silhouettes) else {
            continue;
        };
        let points = beam::outline(&beam);
        if !points.is_empty() {
            frame.push_primitive(
                GLOW_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    points,
                    RenderColor::rgba(1.0, 0.87, 0.46, 0.12),
                )),
            );
        }
        // The actual opening covers the corresponding portion of the canopy.
        frame.push_primitive(
            GLOW_LAYER,
            rectangle(
                RenderPoint::new(source.left, layout.canopy_y),
                RenderPoint::new(source.right, layout.bounds_max.y),
                RenderColor::rgb(0.65, 0.77, 0.92),
                None,
            ),
        );
    }
}

#[cfg(test)]
mod ceiling_light_contract;

/// Fresh diagnostic receivers, never the biology cache from an earlier pose.
/// Only diagnostic sessions pay for this per-frame geometry query.
pub(crate) fn optical_evidence(state: &ClockState) -> Option<engine_common::ClockCeilingOptics> {
    if !state.config.ceiling_optics {
        return None;
    }
    let layout = Layout::new(state.aspect_ratio());
    if !state.config.ceiling_sunlight {
        return None;
    }
    let sources = state.ceiling.sources(layout);
    let source = *sources.first()?;
    let optics = HostOptics::capture(state);
    // Fixed grid spans every opening's edges and an outside control at each.
    // Two rows expose low moving actors and higher course solids without soil
    // interiors. The budget is shared, so more openings sample each in less
    // detail rather than exceeding MAX_RECEIVERS.
    let per_opening = (32 / sources.len()).max(2);
    let receivers: Vec<_> = [layout.floor_y + 10.0, layout.floor_y + 30.0]
        .into_iter()
        .flat_map(|y| {
            sources.iter().flat_map(move |source| {
                (0..per_opening).map(move |i| Receiver {
                    position: Vec2::new(
                        source.left - 2.0
                            + (source.right - source.left + 4.0) * i as f32
                                / (per_opening - 1) as f32,
                        y,
                    ),
                    surface: None,
                })
            })
        })
        .collect();
    let samples = optics
        .scene()
        .query_light(
            state.simulation_tick(),
            state.plants.intensity,
            &sources,
            &receivers,
        )
        .ok()?;
    let silhouettes = optics.silhouettes().ok()?;
    let milli = |v: f32| (v * 1000.0).round() as i32;
    let world = crate::arena::canonical_world(&state.active_event, state.duck_visit.as_deref());
    Some(engine_common::ClockCeilingOptics {
        simulation_tick: state.simulation_tick(),
        // query_light publishes generation == completed simulation tick.
        geometry_generation: state.simulation_tick(),
        source_id: [source.id.0, source.id.1],
        aperture_milli: [
            milli(source.left),
            milli(source.right),
            milli(source.ceiling_y),
        ],
        apertures: sources
            .iter()
            .map(|a| engine_common::ClockCeilingAperture {
                source_id: [a.id.0, a.id.1],
                aperture_milli: [milli(a.left), milli(a.right), milli(a.ceiling_y)],
            })
            .collect(),
        collider_generations: optics
            .geometry
            .as_ref()
            .map(|g| g.lighting_collider_generations())
            .unwrap_or_default(),
        moving_blocker: world
            .and_then(|w| {
                w.collider_ids()
                    .find(|id| *id == crate::events::duck::DUCK_COLLIDER)
            })
            .map(|id| {
                let id = engine_rapier::world::light_collider_id(id);
                [id.0, id.1]
            }),
        silhouettes_milli: silhouettes
            .into_iter()
            .map(|p| p.into_iter().map(|v| [milli(v.x), milli(v.y)]).collect())
            .collect(),
        samples: receivers
            .iter()
            .zip(samples)
            .map(|(r, e)| engine_common::ClockCeilingSample {
                position_milli: [milli(r.position.x), milli(r.position.y)],
                intensity: e.intensity.floor() as u16,
                sunlight: e.local.floor() as u16,
                first_blocker: e.first_blocker.map(|id| [id.0, id.1]),
            })
            .collect(),
    })
}
