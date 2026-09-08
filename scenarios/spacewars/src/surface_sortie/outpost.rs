//! Scenario-owned capture and repair policy, independent of landing mechanics.

use super::*;

pub(super) const CAPTURE_TIME: Duration = Duration::from_secs(3);
pub(super) const CAPTURE_RANGE: f32 = 3.0;
pub(super) const CAPTURE_MAX_SPEED: f32 = 1.0;
pub(super) const REPAIR_RANGE: f32 = 24.0;
pub(super) const REPAIR_FRACTION_PER_SECOND: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutpostId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Aboard,
    TooFar,
    NeedSupport,
    NeedBalance,
    NeedSettle,
    Capturing,
    Contested,
    Secured,
}

impl CaptureStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Aboard => "disembark to capture",
            Self::TooFar => "walk to the amber terminal",
            Self::NeedSupport => "land beside the terminal",
            Self::NeedBalance => "recover your balance",
            Self::NeedSettle => "stand still to capture",
            Self::Capturing => "capturing; stay beside the terminal",
            Self::Contested => "contested; capture paused",
            Self::Secured => "secured",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStatus {
    NeedsCapture,
    NotFriendly,
    VehicleUnavailable,
    NeedLanding,
    OutOfRange,
    Repairing,
    Ready,
}

impl RepairStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsCapture => "capture the outpost",
            Self::NotFriendly => "outpost is not friendly",
            Self::VehicleUnavailable => "vehicle unavailable",
            Self::NeedLanding => "land to repair",
            Self::OutOfRange => "land closer to the outpost",
            Self::Repairing => "repairing +5%/s",
            Self::Ready => "ship ready for departure",
        }
    }
}

pub(super) struct SurfaceOutpost {
    pub id: OutpostId,
    pub planet: usize,
    pub local_angle: f32,
    pub owner: Option<PlayerId>,
    capture_elapsed: Duration,
    capturing_player: Option<PlayerId>,
    capture_status: [CaptureStatus; SPACEWARS_PLAYER_COUNT],
    captures: u64,
    repair_status: [RepairStatus; SPACEWARS_PLAYER_COUNT],
    repaired_health: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutpostObservation {
    pub id: OutpostId,
    pub planet: usize,
    pub position: Vec2,
    pub surface_normal: Vec2,
    pub owner: Option<PlayerId>,
    pub capturing_player: Option<PlayerId>,
    pub capture_status: CaptureStatus,
    pub capture_progress: f32,
    pub capture_required_seconds: f32,
    pub capture_range: f32,
    pub captures: u64,
    pub repair_status: RepairStatus,
    pub repair_range: f32,
    pub repaired_health: f32,
}

impl SurfaceOutpost {
    pub(super) fn new(id: OutpostId, planet: usize, local_angle: f32) -> Self {
        Self {
            id,
            planet,
            local_angle,
            owner: None,
            capture_elapsed: Duration::ZERO,
            capturing_player: None,
            capture_status: [CaptureStatus::Aboard; SPACEWARS_PLAYER_COUNT],
            captures: 0,
            repair_status: [RepairStatus::NeedsCapture; SPACEWARS_PLAYER_COUNT],
            repaired_health: 0.0,
        }
    }

    pub(super) fn up(&self, planet: &PlanetState) -> Vec2 {
        Vec2::from_radians(planet.wrapper_angle + self.local_angle)
    }

    pub(super) fn position(&self, planet: &PlanetState) -> Vec2 {
        planet.position + self.up(planet) * (planet.radius * BODY_BOUNDS_RADIUS_SCALE)
    }

    pub(super) fn observation(&self, planet: &PlanetState, player: usize) -> OutpostObservation {
        OutpostObservation {
            id: self.id,
            planet: self.planet,
            position: self.position(planet),
            surface_normal: self.up(planet),
            owner: self.owner,
            capturing_player: self.capturing_player,
            capture_status: self.capture_status[player],
            capture_progress: self.capture_elapsed.as_secs_f32() / CAPTURE_TIME.as_secs_f32(),
            capture_required_seconds: CAPTURE_TIME.as_secs_f32(),
            capture_range: CAPTURE_RANGE,
            captures: self.captures,
            repair_status: self.repair_status[player],
            repair_range: REPAIR_RANGE,
            repaired_health: self.repaired_health,
        }
    }

    /// Evaluate the complete set before changing ownership; iteration order cannot win a race.
    pub(super) fn update_capture(&mut self, statuses: &[(PlayerId, CaptureStatus)], dt: Duration) {
        let mut eligible = [None; SPACEWARS_PLAYER_COUNT];
        let mut count = 0;
        for &(player, status) in statuses {
            self.capture_status[player.index()] = if self.owner == Some(player) {
                CaptureStatus::Secured
            } else {
                status
            };
            if status == CaptureStatus::Capturing {
                eligible[count] = Some(player);
                count += 1;
            }
        }
        if count > 1 {
            for player in eligible.into_iter().flatten() {
                self.capture_status[player.index()] = CaptureStatus::Contested;
            }
            // Preserve an active claim only while that claimant still qualifies.
            if !eligible.contains(&self.capturing_player) {
                self.capturing_player = None;
                self.capture_elapsed = Duration::ZERO;
            }
            return;
        }
        let Some(claimant) = eligible[0] else {
            self.capturing_player = None;
            self.capture_elapsed = if self.owner.is_some() {
                CAPTURE_TIME
            } else {
                Duration::ZERO
            };
            return;
        };
        if self.owner == Some(claimant) {
            self.capturing_player = None;
            self.capture_elapsed = CAPTURE_TIME;
            return;
        }
        if self.capturing_player != Some(claimant) {
            self.capture_elapsed = Duration::ZERO;
            self.capturing_player = Some(claimant);
        }
        self.capture_elapsed = self.capture_elapsed.saturating_add(dt).min(CAPTURE_TIME);
        if self.capture_elapsed == CAPTURE_TIME {
            self.owner = Some(claimant);
            self.capturing_player = None;
            // Refresh both views immediately: the previous owner must not
            // report "secured" for one extra tick after ownership changes.
            for &(player, status) in statuses {
                self.capture_status[player.index()] = if player == claimant {
                    CaptureStatus::Secured
                } else {
                    status
                };
            }
            self.captures += 1;
        }
    }

    pub(super) fn repair_ship(
        &mut self,
        ship: &mut ShipState,
        landed: bool,
        distance: f32,
        dt: Duration,
    ) {
        self.repair_status[ship.owner_id] = if self.owner.is_none() {
            RepairStatus::NeedsCapture
        } else if ship.dead || ship.life <= 0.0 || ship.form != ShipForm::Ship {
            RepairStatus::VehicleUnavailable
        } else if self.owner.map(PlayerId::index) != Some(ship.owner_id) {
            RepairStatus::NotFriendly
        } else if !landed {
            RepairStatus::NeedLanding
        } else if distance > REPAIR_RANGE {
            RepairStatus::OutOfRange
        } else if ship.life >= ship.life_max {
            RepairStatus::Ready
        } else {
            let before = ship.life;
            ship.life = (ship.life + ship.life_max * REPAIR_FRACTION_PER_SECOND * dt.as_secs_f32())
                .min(ship.life_max);
            self.repaired_health += ship.life - before;
            if ship.life >= ship.life_max {
                RepairStatus::Ready
            } else {
                RepairStatus::Repairing
            }
        };
    }
}

impl SurfaceSortieState {
    fn capture_status(&self, player: usize, outpost: &SurfaceOutpost) -> CaptureStatus {
        let Some(snapshot) = self.spaceling_snapshot(player) else {
            return CaptureStatus::Aboard;
        };
        let planet = &self.world.planets[outpost.planet];
        if snapshot
            .motion
            .position
            .distance_to(outpost.position(planet))
            > CAPTURE_RANGE
        {
            return CaptureStatus::TooFar;
        }
        let Some(support) = snapshot
            .support
            .filter(|support| physics::is_planet_surface_support(support.collider, outpost.planet))
        else {
            return CaptureStatus::NeedSupport;
        };
        if snapshot.balance != SpacelingBalance::Balanced {
            return CaptureStatus::NeedBalance;
        }
        let offset = support.position - snapshot.motion.position;
        let relative = snapshot.motion.linear_velocity
            + Vec2::new(-offset.y, offset.x) * snapshot.motion.angular_velocity
            - support.velocity;
        if relative.length() > CAPTURE_MAX_SPEED {
            return CaptureStatus::NeedSettle;
        }
        CaptureStatus::Capturing
    }

    pub(super) fn update_outpost(&mut self, dt: Duration) {
        let player_count = self.player_count();
        for index in 0..self.outposts.len() {
            let statuses: [_; SPACEWARS_PLAYER_COUNT] = std::array::from_fn(|player| {
                let owner = PlayerId::from_index(player).expect("bounded player index");
                (
                    owner,
                    if player < self.player_count() {
                        self.capture_status(player, &self.outposts[index])
                    } else {
                        CaptureStatus::Aboard
                    },
                )
            });
            self.outposts[index].update_capture(&statuses[..player_count], dt);
            for player in 0..self.player_count() {
                let landed = self.vehicle_settled(player)
                    && self.pilots[player].landing.planet == Some(self.outposts[index].planet);
                let post = &mut self.outposts[index];
                let position = post.position(&self.world.planets[post.planet]);
                let ship = &mut self.world.ships[self.pilots[player].vehicle.0];
                post.repair_ship(
                    ship,
                    landed,
                    (ship.position + SHIP_PIVOT).distance_to(position),
                    dt,
                );
            }
        }
    }
}
