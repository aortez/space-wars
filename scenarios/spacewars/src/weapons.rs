//! Ship-owned ammunition and energy. The material combat presets opt in;
//! projectile damage, collision geometry and laser tracing remain shared.
use super::*;

pub mod fixture;

pub const ENERGY_CAPACITY: f32 = 100.0;
pub const ENERGY_REGEN_PER_SECOND: f32 = 10.0;
pub const LASER_DRAW_PER_SECOND: f32 = 12.0;
pub const ROUND_ENERGY_COST: f32 = 25.0;
pub const ROUND_RELOAD_SECONDS: f32 = 2.0;
pub const ROUND_CAPACITY: usize = 2;
/// Supplied missiles weigh about 3% of a full ship. Their explosive damage is
/// independent of launching a survivor at almost the incoming missile speed.
pub const ROUND_MASS: f32 = 1.0;
const LASER_RESTART_ENERGY: f32 = 10.0;
const RELOAD_EPSILON: f32 = 1.0e-5;

// Fixed side rails straddle the fuselage edges, staying visibly attached while
// the wings sweep. Launch positions use these same centers so a fired round
// never jumps away from its rail.
const MOUNTS: [Vec2; ROUND_CAPACITY] = [Vec2::new(0.75, 2.0), Vec2::new(4.25, 2.0)];
// Presentation only: half the old length, with narrower shoulders and fins.
// The same silhouette is used on the rails, during reload and in flight.
const ROUND_SHAPE: [Vec2; 7] = [
    Vec2::new(0.0, 1.0),
    Vec2::new(0.26, 0.45),
    Vec2::new(0.26, -0.55),
    Vec2::new(0.4, -1.0),
    Vec2::new(-0.4, -1.0),
    Vec2::new(-0.26, -0.55),
    Vec2::new(-0.26, 0.45),
];
const ROUND_TIP: [Vec2; 3] = [ROUND_SHAPE[6], ROUND_SHAPE[0], ROUND_SHAPE[1]];
const ROUND_COLOR: RenderColor = RenderColor::rgb(0.66, 0.69, 0.72);
const ROUND_NOSE: RenderColor = RenderColor::rgb(1.0, 0.62, 0.18);

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeaponSupplyObservation {
    pub energy_percent: f32,
    pub rounds_loaded: u8,
    /// One reload at a time; energy is paid when this operation starts.
    pub reload_progress: Option<f32>,
    pub laser_recharging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Reload {
    mount: usize,
    remaining: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShipArmament {
    energy: f32,
    loaded: [bool; ROUND_CAPACITY],
    reload: Option<Reload>,
    next_mount: usize,
    laser_recharging: bool,
    pub(super) fired_this_step: bool,
}

impl Default for ShipArmament {
    fn default() -> Self {
        Self {
            energy: ENERGY_CAPACITY,
            loaded: [true; ROUND_CAPACITY],
            reload: None,
            next_mount: 0,
            laser_recharging: false,
            fired_this_step: false,
        }
    }
}

impl ShipArmament {
    pub(super) fn observation(&self) -> WeaponSupplyObservation {
        WeaponSupplyObservation {
            energy_percent: self.energy,
            rounds_loaded: self.loaded.iter().filter(|loaded| **loaded).count() as u8,
            reload_progress: self
                .reload
                .map(|r| (1.0 - r.remaining / ROUND_RELOAD_SECONDS).clamp(0.0, 1.0)),
            laser_recharging: self.laser_recharging,
        }
    }

    /// Called once per simulation step, including while the ship is parked.
    pub(super) fn advance(&mut self, dt: f32) {
        self.fired_this_step = false;
        self.energy = (self.energy + ENERGY_REGEN_PER_SECOND * dt).min(ENERGY_CAPACITY);
        if let Some(reload) = &mut self.reload {
            reload.remaining = (reload.remaining - dt).max(0.0);
            if reload.remaining <= RELOAD_EPSILON {
                self.loaded[reload.mount] = true;
                self.reload = None;
            }
        }
        if self.reload.is_none()
            && self.energy >= ROUND_ENERGY_COST
            && let Some(mount) = self.loaded.iter().position(|loaded| !loaded)
        {
            self.energy -= ROUND_ENERGY_COST;
            self.reload = Some(Reload {
                mount,
                remaining: ROUND_RELOAD_SECONDS,
            });
        }
        // A held trigger must not produce a frame-by-frame flicker at zero.
        // Releasing the trigger does not bypass the same recharge requirement.
        if self.energy < LASER_DRAW_PER_SECOND * dt {
            self.laser_recharging = true;
        } else if self.laser_recharging && self.energy >= LASER_RESTART_ENERGY {
            self.laser_recharging = false;
        }
    }

    pub(super) fn cannon_ready(&self) -> bool {
        self.loaded.iter().any(|loaded| *loaded)
    }

    pub(super) fn take_round(&mut self) -> Option<usize> {
        let mount = (0..ROUND_CAPACITY)
            .map(|offset| (self.next_mount + offset) % ROUND_CAPACITY)
            .find(|mount| self.loaded[*mount])?;
        self.loaded[mount] = false;
        self.next_mount = (mount + 1) % ROUND_CAPACITY;
        self.fired_this_step = true;
        Some(mount)
    }

    pub(super) fn laser_ready(&self, dt: f32) -> bool {
        !self.laser_recharging && self.energy >= LASER_DRAW_PER_SECOND * dt
    }

    pub(super) fn power_laser(&mut self, dt: f32) -> bool {
        if self.laser_recharging {
            return false;
        }
        let cost = LASER_DRAW_PER_SECOND * dt;
        if self.energy < cost {
            self.laser_recharging = true;
            return false;
        }
        self.energy = (self.energy - cost).max(0.0);
        if self.energy < cost {
            self.laser_recharging = true;
        }
        true
    }
}

impl ShipState {
    pub fn weapon_supply(&self) -> Option<WeaponSupplyObservation> {
        self.armament.as_ref().map(ShipArmament::observation)
    }

    pub(crate) fn enable_weapon_supply(&mut self) {
        self.armament = Some(ShipArmament::default());
    }
}

pub(super) fn mount_position(ship: &ShipState, mount: usize) -> Vec2 {
    ship_transform(ship).transform_point(MOUNTS[mount])
}

pub(super) fn render_round(
    frame: &mut RenderFrame,
    layer: i32,
    transform: Transform2,
    brightness: f32,
) {
    let outline = Stroke::new(RenderColor::rgb(0.12, 0.14, 0.17), 0.06);
    hardware_polygon(
        frame,
        layer,
        transform,
        &ROUND_SHAPE,
        dim(ROUND_COLOR, brightness),
        outline,
    );
    hardware_polygon(
        frame,
        layer,
        transform,
        &ROUND_TIP,
        dim(ROUND_NOSE, brightness),
        outline,
    );
}

fn hardware_polygon(
    frame: &mut RenderFrame,
    layer: i32,
    transform: Transform2,
    points: &[Vec2],
    fill: RenderColor,
    outline: Stroke,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points
                .iter()
                .map(|p| render_point(transform.transform_point(*p)))
                .collect(),
            fill: Some(Fill::new(fill)),
            stroke: Some(outline),
        }),
    );
}

pub(super) fn render_mounts(frame: &mut RenderFrame, ship: &ShipState) {
    let Some(armament) = &ship.armament else {
        return;
    };
    for (mount, center) in MOUNTS.into_iter().enumerate() {
        let rail = [
            center + Vec2::new(-0.175, -1.2),
            center + Vec2::new(0.175, -1.2),
            center + Vec2::new(0.175, 0.85),
            center + Vec2::new(-0.175, 0.85),
        ];
        hardware_polygon(
            frame,
            SHIP_LAYER,
            ship_transform(ship),
            &rail,
            RenderColor::rgb(0.15, 0.17, 0.19),
            Stroke::new(RenderColor::rgb(0.42, 0.46, 0.5), 0.06),
        );
        let progress = if armament.loaded[mount] {
            Some(1.0)
        } else {
            armament
                .reload
                .filter(|r| r.mount == mount)
                .map(|r| (1.0 - r.remaining / ROUND_RELOAD_SECONDS).clamp(0.0, 1.0))
        };
        if let Some(progress) = progress {
            // The replacement feeds forward along the rail as it is loaded.
            let center = center - Vec2::Y * (1.0 - progress) * 1.5;
            render_round(
                frame,
                SHIP_LAYER,
                Transform2 {
                    translation: ship_transform(ship).transform_point(center),
                    rotation_radians: ship.rotation_radians,
                    ..Transform2::IDENTITY
                },
                if armament.loaded[mount] { 1.0 } else { 0.45 },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1.0 / 60.0;

    fn armed_ship() -> ShipState {
        let mut ship = ShipState::new_with_default_life(0, Vec2::ZERO, Color::WHITE, DT);
        ship.enable_weapon_supply();
        ship
    }

    fn polygons(frame: &RenderFrame) -> Vec<&RenderPolygon> {
        frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .map(|primitive| {
                let RenderPrimitive::Polygon(polygon) = primitive else {
                    panic!("hardware should only emit polygons");
                };
                polygon
            })
            .collect()
    }

    #[test]
    fn missile_art_is_half_length_slim_gray_with_the_original_warm_tip() {
        let mut frame = RenderFrame::default();
        render_round(&mut frame, DEBRIS_LAYER, Transform2::IDENTITY, 1.0);
        let parts = polygons(&frame);
        assert_eq!(parts.len(), 2, "one body and one tip; no extra effects");
        let body = parts[0];
        let span = |axis: fn(&RenderPoint) -> f32| {
            body.points
                .iter()
                .map(axis)
                .fold(f32::NEG_INFINITY, f32::max)
                - body.points.iter().map(axis).fold(f32::INFINITY, f32::min)
        };
        assert!(
            (span(|p| p.y) - 2.0).abs() < 1e-6,
            "old rounds were four units long"
        );
        assert!(
            span(|p| p.x) <= 0.8,
            "fins must not restore the old broad silhouette"
        );
        let gray = body.fill.unwrap().color;
        assert!(gray.r > 0.5 && (gray.r - gray.g).abs() < 0.1 && (gray.r - gray.b).abs() < 0.1);
        let nose = parts[1];
        assert_eq!(nose.fill.unwrap().color, RenderColor::rgb(1.0, 0.62, 0.18));
        assert!(
            nose.points.iter().all(|p| p.y >= 0.4),
            "only the forward tip has the warm accent"
        );
    }

    #[test]
    fn missile_racks_overlap_the_fuselage_regardless_of_wing_sweep() {
        for angle in [0.0, 0.65, -2.4] {
            let mut ship = armed_ship();
            ship.position = Vec2::new(40.0, -20.0);
            ship.rotation_radians = angle;
            let hull = SHIP_BODY.map(|point| ship_transform(&ship).transform_point(point));
            // The fuselage triangle is counterclockwise. Test the actual
            // transformed artwork, independently of the moving wings.
            let inside_hull = |point: &RenderPoint| {
                (0..hull.len()).all(|index| {
                    let start = hull[index];
                    let edge = hull[(index + 1) % hull.len()] - start;
                    let offset = Vec2::new(point.x, point.y) - start;
                    edge.x * offset.y - edge.y * offset.x >= -1e-5
                })
            };
            let mut open = RenderFrame::default();
            render_mounts(&mut open, &ship);
            for sweep in [0.0, MAX_WING_THETA * 0.5, MAX_WING_THETA] {
                ship.wing_theta = sweep;
                let mut frame = RenderFrame::default();
                render_mounts(&mut frame, &ship);
                let parts = polygons(&frame);
                for (part, fixed) in parts.iter().zip(polygons(&open)) {
                    assert_eq!(part.points, fixed.points, "racks must not follow the wings");
                }
                for mount in 0..ROUND_CAPACITY {
                    let rail = parts[mount * 3];
                    let body = parts[mount * 3 + 1];
                    assert!(
                        rail.points
                            .iter()
                            .filter(|point| inside_hull(point))
                            .count()
                            >= 2,
                        "rail {mount} must be anchored to the hull at sweep {sweep}"
                    );
                    let overlapping = body
                        .points
                        .iter()
                        .filter(|point| inside_hull(point))
                        .count();
                    assert!(
                        overlapping >= 2 && overlapping < body.points.len(),
                        "missile {mount} must straddle the hull edge at sweep {sweep}"
                    );
                    assert!(inside_hull(&render_point(mount_position(&ship, mount))));
                    assert_eq!(MOUNTS[mount].y, 2.0, "keep the rearward rail position");
                }
            }
        }
    }

    #[test]
    fn missile_nose_points_along_velocity_in_every_quadrant() {
        for velocity in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y, Vec2::new(-3.0, 4.0)] {
            let mut shell =
                DebrisState::new_shell(0, 0, Vec2::new(10.0, -20.0), velocity * 60.0, 1.7);
            shell.rail_launched = true;
            let mut frame = RenderFrame::default();
            render_debris(&mut frame, &shell);
            let parts = polygons(&frame);
            let nose = &parts[1].points;
            let center = nose
                .iter()
                .fold(Vec2::ZERO, |sum, p| sum + Vec2::new(p.x, p.y))
                / nose.len() as f32;
            let direction = (center - shell.position).normalized();
            assert!(
                direction.dot(velocity.normalized()) > 0.9999,
                "nose faces away from flight: {velocity:?}"
            );
        }
    }

    #[test]
    fn missile_art_matches_between_rails_and_flight_without_changing_weapon_stats() {
        for angle in [0.0, 0.65, 1.7, -2.4] {
            for sweep in [0.0, MAX_WING_THETA] {
                let mut ship = armed_ship();
                ship.rotation_radians = angle;
                ship.direction = direction_from_rotation(angle);
                ship.wing_theta = sweep;
                let mut loaded = RenderFrame::default();
                render_mounts(&mut loaded, &ship);
                ship.set_cannon(true);
                let shell = ship.update_cannon_with_recoil(DT, 0, 8.0).unwrap();
                assert_eq!(shell.mass(), ROUND_MASS);
                assert_eq!(shell.radius, CANNON_SHELL_RADIUS);
                assert_eq!(shell.damage_scalar, CANNON_SHELL_DAMAGE_SCALAR);
                assert!((shell.velocity.length() - CANNON_SHELL_SPEED).abs() < 1e-4);
                let mut launched = RenderFrame::default();
                render_debris(&mut launched, &shell);
                let loaded = polygons(&loaded);
                let launched = polygons(&launched);
                assert_eq!(loaded.len(), 6, "two rails, two bodies, two tips");
                for (mounted, flying) in loaded[1..3].iter().zip(launched) {
                    assert_eq!(mounted.fill, flying.fill);
                    assert_eq!(mounted.stroke, flying.stroke);
                    assert_eq!(mounted.points.len(), flying.points.len());
                    for (a, b) in mounted.points.iter().zip(&flying.points) {
                        assert!((a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5);
                    }
                }
            }
        }
    }

    #[test]
    fn loaded_rounds_launch_from_alternating_visible_rails_without_energy() {
        for angle in [0.0, 1.2, -2.4] {
            for sweep in [0.0, MAX_WING_THETA] {
                let mut ship = armed_ship();
                ship.position = Vec2::new(40.0, -20.0);
                ship.rotation_radians = angle;
                ship.direction = direction_from_rotation(angle);
                ship.wing_theta = sweep;
                ship.armament.as_mut().unwrap().energy = 0.0;
                ship.cannon_firing = true;
                for mount in 0..ROUND_CAPACITY {
                    let expected = mount_position(&ship, mount);
                    ship.cannon_cooldown_remaining = 0.0;
                    let shell = ship.update_cannon_with_recoil(DT, 0, 8.0).unwrap();
                    assert_eq!(shell.position, expected);
                    assert!(shell.rail_launched);
                    assert_eq!(ship.weapon_supply().unwrap().energy_percent, 0.0);
                }
                ship.cannon_cooldown_remaining = 0.0;
                assert!(ship.update_cannon(DT, 0).is_none());
            }
        }
    }

    #[test]
    fn supplied_missile_breakup_preserves_inertia_and_legacy_shells_keep_their_mass() {
        let mut ship = armed_ship();
        ship.cannon_firing = true;
        let missile = ship.update_cannon_with_recoil(DT, 0, 8.0).unwrap();
        let legacy = DebrisState::new_shell(0, 0, Vec2::ZERO, Vec2::X * 300.0, 0.0);
        assert_eq!(missile.mass(), ROUND_MASS);
        assert_eq!(legacy.mass(), debris_mass(CANNON_SHELL_RADIUS));
        assert!(missile.mass() < legacy.mass() / 10.0);
        let pieces = debris_breakup_fragments(&missile, 42, 1, 0, 0);
        assert!(!pieces.is_empty());
        assert!((pieces.iter().map(|p| p.mass()).sum::<f32>() - missile.mass()).abs() < 1e-6);
        assert!(
            pieces
                .iter()
                .all(|p| p.mass() > 0.0 && p.damage_scalar == 0.0 && p.velocity.length() > 0.0)
        );
    }

    #[test]
    fn reload_is_paid_once_takes_two_seconds_and_serializes_empty_slots() {
        let mut a = ShipArmament::default();
        assert_eq!(a.take_round(), Some(0));
        assert_eq!(a.take_round(), Some(1));
        assert_eq!(a.energy, 100.0, "launches spend ammunition");
        a.advance(DT);
        assert_eq!(a.energy, 75.0);
        assert_eq!(a.observation().reload_progress, Some(0.0));
        for _ in 0..119 {
            a.advance(DT);
            assert!(!a.cannon_ready());
        }
        a.advance(DT);
        assert_eq!(a.loaded, [true, false]);
        assert!(
            (a.energy - 70.0).abs() < 0.01,
            "second reload must pay another 25%"
        );
        assert_eq!(a.reload.unwrap().mount, 1);
        for _ in 0..120 {
            a.advance(DT);
        }
        assert_eq!(a.loaded, [true, true]);
        assert!(a.reload.is_none());
        assert!((a.energy - 90.0).abs() < 0.01);
    }

    #[test]
    fn energy_below_reload_cost_waits_and_a_paid_reload_survives_laser_drain() {
        let mut a = ShipArmament {
            energy: 24.0,
            loaded: [false, true],
            ..Default::default()
        };
        for _ in 0..5 {
            a.advance(DT);
            assert!(a.reload.is_none());
        }
        for _ in 0..2 {
            a.advance(DT);
        }
        assert!(a.reload.is_some());
        assert!(a.energy < 1.0);
        for _ in 0..121 {
            a.advance(DT);
            a.power_laser(DT);
        }
        assert_eq!(a.loaded, [true, true]);
    }

    #[test]
    fn laser_net_draw_and_recharge_are_time_based_and_bounded() {
        for hz in [30, 60, 120] {
            let dt = 1.0 / hz as f32;
            let mut a = ShipArmament {
                energy: 50.0,
                ..Default::default()
            };
            for _ in 0..15 * hz {
                a.advance(dt);
                assert!(a.power_laser(dt));
            }
            assert!((a.energy - 20.0).abs() < 0.02, "{hz} Hz: {}", a.energy);
            for _ in 0..10 * hz {
                a.advance(dt);
            }
            assert_eq!(a.energy, 100.0);
        }
    }

    #[test]
    fn laser_depletion_has_the_same_recharge_gate_for_held_and_readiness_driven_fire() {
        for follow_readiness in [false, true] {
            let mut a = ShipArmament {
                energy: 1.0,
                ..Default::default()
            };
            let mut first_depletion = None;
            let mut resumed = None;
            for tick in 0..300 {
                a.advance(DT);
                if first_depletion.is_some() && !a.laser_recharging {
                    assert!(a.energy >= LASER_RESTART_ENERGY);
                    resumed = Some(tick);
                    break;
                }
                if !follow_readiness || a.laser_ready(DT) {
                    a.power_laser(DT);
                }
                if a.laser_recharging && first_depletion.is_none() {
                    first_depletion = Some(tick);
                    assert!(
                        a.take_round().is_some(),
                        "loaded ammunition works at depletion"
                    );
                }
                assert!((0.0..=100.0).contains(&a.energy));
            }
            let gap = resumed.unwrap() - first_depletion.unwrap();
            assert!((58..=61).contains(&gap), "recharge gap {gap}");
        }
    }

    #[test]
    fn three_minutes_of_held_fire_respects_reload_time_and_energy_budget() {
        for laser in [false, true] {
            let mut ship = armed_ship();
            ship.cannon_firing = true;
            ship.laser_firing = laser;
            let mut launches = Vec::new();
            let mut laser_ticks = 0;
            for tick in 0..180 * 60 {
                ship.armament.as_mut().unwrap().advance(DT);
                if ship.update_cannon_with_recoil(DT, tick, 0.0).is_some() {
                    launches.push(tick);
                } else if laser {
                    ship.update_laser(DT);
                    laser_ticks += u32::from(ship.laser_beam.is_some());
                }
                let s = ship.weapon_supply().unwrap();
                assert!((0.0..=100.0).contains(&s.energy_percent));
                assert!(s.rounds_loaded <= 2);
            }
            // Two initially loaded rounds; all replacements must be paid for
            // out of the initial battery plus at most 180 seconds of generation.
            let reload_energy = (launches.len() - 2) as f32 * ROUND_ENERGY_COST;
            let laser_energy = laser_ticks as f32 * DT * LASER_DRAW_PER_SECOND;
            assert!(reload_energy + laser_energy <= 100.0 + 180.0 * 10.0 + 0.1);
            assert!(launches.len() <= 78);
            assert!(launches[1] - launches[0] >= 30);
            for pair in launches[1..].windows(2) {
                assert!(pair[1] - pair[0] >= 89, "no immediate replacement salvo");
            }
            if laser {
                assert!(laser_ticks > 0);
                assert!(launches.len() < 40, "laser must compete with replenishment");
            }
        }
    }
}
