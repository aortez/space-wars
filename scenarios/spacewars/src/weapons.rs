//! Ship-owned ammunition and energy. The material combat presets opt in;
//! projectile damage, collision geometry and laser tracing remain shared.
use super::*;

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

// Fixed rails beside the wing roots keep rounds facing forward while sweeping.
const MOUNTS: [Vec2; ROUND_CAPACITY] = [Vec2::new(-0.65, 3.5), Vec2::new(5.65, 3.5)];
const ROUND_SHAPE: [Vec2; 7] = [
    Vec2::new(0.0, 2.0),
    Vec2::new(0.65, 0.9),
    Vec2::new(0.65, -1.1),
    Vec2::new(1.0, -2.0),
    Vec2::new(-1.0, -2.0),
    Vec2::new(-0.65, -1.1),
    Vec2::new(-0.65, 0.9),
];
const ROUND_COLOR: RenderColor = RenderColor::rgb(0.96, 0.94, 0.76);
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
    let outline = RenderColor::rgb(0.12, 0.16, 0.2);
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
        &[
            Vec2::new(-0.65, 0.9),
            Vec2::new(0.0, 2.0),
            Vec2::new(0.65, 0.9),
        ],
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
    outline: RenderColor,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points
                .iter()
                .map(|p| render_point(transform.transform_point(*p)))
                .collect(),
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(outline, 0.12)),
        }),
    );
}

pub(super) fn render_mounts(frame: &mut RenderFrame, ship: &ShipState) {
    let Some(armament) = &ship.armament else {
        return;
    };
    for (mount, center) in MOUNTS.into_iter().enumerate() {
        let rail = [
            center + Vec2::new(-0.35, -2.4),
            center + Vec2::new(0.35, -2.4),
            center + Vec2::new(0.35, 1.7),
            center + Vec2::new(-0.35, 1.7),
        ];
        hardware_polygon(
            frame,
            SHIP_LAYER,
            ship_transform(ship),
            &rail,
            RenderColor::rgb(0.14, 0.19, 0.23),
            RenderColor::rgb(0.52, 0.59, 0.64),
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
            let center = center - Vec2::Y * (1.0 - progress) * 3.0;
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
