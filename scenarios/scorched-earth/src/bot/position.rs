//! Small local firing-position estimates, using current collision surfaces.
//! These are terrain probes, not simulated shots or predicted enemy attacks.
use super::*;

#[derive(Clone, Serialize)]
pub(super) struct RelocationSite {
    pub x: f32,
    pub range: f32,
    pub pitch_degrees: Option<f32>,
    pub roughness: Option<f32>,
    pub score: Option<f32>,
    pub rejection: Option<&'static str>,
}

impl ScorchedState {
    pub(super) fn relocation_sites(&self, player: usize, elevation: f32) -> Vec<RelocationSite> {
        let geometry = Geometry::read(self, player);
        let toward = (geometry.target.x - geometry.shooter.x).signum();
        let mut sites = Vec::new();
        for direction in [toward, -toward] {
            let ground = self.check_drive_direction(player, direction);
            for distance in [2.5, 2.0] {
                let x = geometry.shooter.x + direction * distance;
                let mut site = RelocationSite {
                    x,
                    range: (geometry.target.x - x).abs(),
                    pitch_degrees: None,
                    roughness: None,
                    score: None,
                    rejection: None,
                };
                site.rejection = if !self.has_track_support(player) {
                    Some("no support")
                } else if x.abs() > 44.0 || !(12.0..=40.0).contains(&site.range) {
                    Some("firing range")
                } else if ground.reason.is_some() {
                    ground.reason
                } else {
                    self.assess_relocation_site(player, &ground, distance, elevation, &mut site)
                };
                sites.push(site);
            }
        }
        sites
    }

    fn assess_relocation_site(
        &self,
        player: usize,
        ground: &driving_lab::GroundCheck,
        distance: f32,
        elevation: f32,
        site: &mut RelocationSite,
    ) -> Option<&'static str> {
        // A three-unit footprint stays entirely inside the existing four-unit
        // route check. This covers both wheel supports and the rigid hull.
        let begin = ((distance - 1.5) * 2.0) as usize;
        let points: Vec<_> = ground.probes[begin..=begin + 6]
            .iter()
            .map(|p| p.unwrap().point.unwrap())
            .collect();
        let first = points[0];
        let last = points[6];
        let grade = (last[1] - first[1]) / (last[0] - first[0]);
        let pitch = grade.atan();
        let roughness = points
            .iter()
            .map(|p| (p[1] - (first[1] + grade * (p[0] - first[0]))).abs())
            .fold(0.0_f32, f32::max);
        site.pitch_degrees = Some(pitch.to_degrees());
        site.roughness = Some(roughness);
        if pitch.abs() > 18.0_f32.to_radians() || roughness > 0.35 {
            return Some("uneven firing footing");
        }
        for point in &points {
            let hit = self.physics.cast_ray(
                Vec2::new(point[0], point[1] + 0.1),
                -Vec2::Y,
                RayCastOptions {
                    max_distance: 0.2,
                    exclude_entity: Some(self.tanks[player].body.entity),
                    ..Default::default()
                },
            );
            let fixed_terrain = hit.is_some_and(|hit| {
                self.terrain.iter().any(|fragment| {
                    fragment.id == hit.collider.entity
                        && self.physics.body_kind(fragment.assembly.body()) == Some(BodyKind::Fixed)
                })
            });
            if !fixed_terrain {
                return Some("loose firing footing");
            }
        }
        let clearance = if self.running_gear[player].is_some() {
            1.2 + self.config.wheel_tuning.radius - WheelTuning::default().radius
        } else {
            0.8
        };
        let y = first[1] + grade * (site.x - first[0]);
        let pivot =
            Vec2::new(site.x, y + clearance * pitch.cos()) + Vec2::Y.rotate_radians(pitch) * 0.85;
        let facing = (Geometry::read(self, player).target.x - site.x).signum();
        let barrel = Vec2::new(facing * elevation.cos(), elevation.sin()) * 2.8;
        if self
            .physics
            .cast_ray(
                pivot,
                barrel,
                RayCastOptions {
                    max_distance: barrel.length(),
                    exclude_entity: Some(self.tanks[player].body.entity),
                    ..Default::default()
                },
            )
            .is_some()
        {
            return Some("blocked firing barrel");
        }
        let range_penalty = (18.0 - site.range).max(0.0) + (site.range - 32.0).max(0.0);
        // Prefer a level, smooth patch in ordinary firing range; all else
        // equal, get farther from the position that was just hit.
        site.score = Some(
            range_penalty * 0.5
                + pitch.to_degrees().abs() / 10.0
                + roughness * 4.0
                + (2.5 - distance) * 0.15,
        );
        None
    }
}
