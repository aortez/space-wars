//! Shared vector/raster primitives derived from the physiological sunlight query.
use super::*;
use engine_core::lighting::Receiver;

mod beam;

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    if state.ceiling.phase == crate::ceiling::Phase::Opening {
        let progress = state.ceiling.fraction();
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
            let (left, right) = crate::ceiling::Ceiling::region(layout);
            let center = (left + right) * 0.5;
            let x = center + (target.x - center) * progress;
            let y = layout.canopy_y + (target.y - layout.canopy_y) * progress * progress;
            frame.push_primitive(
                6,
                rectangle(
                    RenderPoint::new(x - 0.8, y - 0.8),
                    RenderPoint::new(x + 0.8, y + 0.8),
                    RenderColor::rgb(0.38, 0.23, 0.10),
                    None,
                ),
            );
        }
    }
    let Some(source) = state
        .ceiling
        .source(layout)
        .filter(|_| state.config.ceiling_sunlight)
    else {
        return;
    };
    let world = crate::grass::surfaces::world(&state.active_event, state.duck_visit.as_deref());
    let geometry = world.map(|w| w.query_snapshot());
    let transparent: Vec<_> = world
        .into_iter()
        .flat_map(|w| w.collider_ids())
        .filter(|id| id.entity.value() == crate::grass::SOIL_PHYSICS_ID)
        .collect();
    let canonical = match geometry
        .as_ref()
        .map(|g| g.lighting_occluder_silhouettes(&transparent))
        .transpose()
    {
        Ok(polygons) => polygons.unwrap_or_default(),
        Err(_) => return,
    };
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
    // Validate the same bounded publication as physiology before drawing any
    // light. Canonical silhouettes are copied from this immutable query snapshot;
    // the material/tissue polygons are exactly those supplied to query_light.
    if crate::grass::residents::query_light(
        state.simulation_tick(),
        0,
        Some(source),
        &[],
        polygons.clone(),
        geometry.as_ref(),
        &transparent,
        &[Receiver {
            position: Vec2::new((source.left + source.right) * 0.5, layout.floor_y),
            surface: None,
        }],
    )
    .is_err()
    {
        return;
    }
    let mut silhouettes: Vec<_> = polygons.into_iter().map(|(_, p)| p).collect();
    silhouettes.extend(canonical);
    let Some(beam) = beam::polygons(source, layout.floor_y, &silhouettes) else {
        return;
    };
    let points = beam::outline(&beam);
    if !points.is_empty() {
        frame.push_primitive(
            2,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points,
                RenderColor::rgba(1.0, 0.87, 0.46, 0.12),
            )),
        );
    }
    // The actual opening covers the corresponding portion of the canopy.
    frame.push_primitive(
        2,
        rectangle(
            RenderPoint::new(source.left, layout.canopy_y),
            RenderPoint::new(source.right, layout.bounds_max.y),
            RenderColor::rgb(0.65, 0.77, 0.92),
            None,
        ),
    );
}

#[cfg(test)]
mod ceiling_light_contract;

/// Fresh diagnostic receivers, never the biology cache from an earlier pose.
pub(crate) fn optical_evidence(state: &ClockState) -> Option<engine_common::ClockCeilingOptics> {
    let layout = Layout::new(state.aspect_ratio());
    let source = state
        .ceiling
        .source(layout)
        .filter(|_| state.config.ceiling_sunlight)?;
    let world = crate::grass::surfaces::world(&state.active_event, state.duck_visit.as_deref());
    let geometry = world.map(|w| w.query_snapshot());
    let transparent: Vec<_> = world
        .into_iter()
        .flat_map(|w| w.collider_ids())
        .filter(|id| id.entity.value() == crate::grass::SOIL_PHYSICS_ID)
        .collect();
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
    // Fixed grid includes both aperture edges and an outside control. Two rows
    // expose low moving actors and higher course solids without soil interiors.
    let receivers: Vec<_> = [layout.floor_y + 10.0, layout.floor_y + 30.0]
        .into_iter()
        .flat_map(|y| {
            (0..32).map(move |i| Receiver {
                position: Vec2::new(
                    source.left - 2.0 + (source.right - source.left + 4.0) * i as f32 / 31.0,
                    y,
                ),
                surface: None,
            })
        })
        .collect();
    let samples = crate::grass::residents::query_light(
        state.simulation_tick(),
        state.plants.intensity,
        Some(source),
        &[],
        polygons.clone(),
        geometry.as_ref(),
        &transparent,
        &receivers,
    )
    .ok()?;
    let mut silhouettes: Vec<_> = polygons.into_iter().map(|(_, p)| p).collect();
    if let Some(g) = &geometry {
        silhouettes.extend(g.lighting_occluder_silhouettes(&transparent).ok()?);
    }
    let milli = |v: f32| (v * 1000.0).round() as i32;
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
        collider_generations: geometry
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
