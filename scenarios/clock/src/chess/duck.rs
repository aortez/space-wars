//! The duck at the chess board.
//!
//! The duck carries the packed board to its place and away again, and plays
//! from its seat in the room beside it. It is the Clock's duck in another
//! role: the same round body under the same silhouette, walking with the
//! same actuator. Nothing of a duck's own visit comes with it. It has no
//! course and no doors in the arena, it keeps the size the board gives it,
//! and nobody takes it over.
//!
//! Its body is a disc in the arena, in the chess visit's claim. Like the
//! crow's it feels none of the world's gravity, so that whoever sets the
//! world's does not change how it walks. Over the floor its controller
//! brings its own pull, and the floor holds it up. Beyond the edge of the
//! view, where there is no floor, it brings none and the duck walks level.
//! Before each step the controller steers the body toward where the duck is
//! going. The step moves it, and whatever it meets stops it, pushes it or is
//! pushed by it. It is one of the arena's birds, which the walls at the
//! edges of the view let through.
//!
//! The duck comes and goes through the door in the frame on its side. That
//! door is drawn, as the frame is, and is no body. It opens before the duck
//! reaches it and closes behind it.
//!
//! The duck holds the case before it, just off the ground, and it has no
//! body while the duck has it. Where the board is to stand the duck sets the
//! case down and steps back to its seat. To take it away the duck steps up
//! to it, picks it up, turns with it and walks out.
//!
//! Every walk has a time to be done in: half as long again as it takes with
//! nothing in the way, and two seconds more. The visit decides what a duck
//! that is late does next.

use std::f32::consts::{FRAC_PI_2, PI};

use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId, PhysicsWorld,
};

use super::Pose;
use super::stage::{Area, Side, Stage};
use crate::arena::ids::canonical;
use crate::events::FIXED_HZ;
use crate::events::duck::flaps::DoorPanel;
use crate::events::duck::{Command, DT, Gait, Movement, Observation};
use crate::layout::Layout;

#[cfg(test)]
mod tests;

/// The duck's identity in the arena.
pub(crate) const ENTITY: PhysicsId = PhysicsId::new(canonical::CHESS_DUCK.start);
const BODY: BodyId = BodyId::new(ENTITY, BodyRole::PRIMARY);
const COLLIDER: ColliderId = ColliderId::new(ENTITY, ColliderRole::PRIMARY, 0);

/// The radius of the duck's body, as a share of the board's side: as large
/// as lets the duck sit within its room and lean as far as the board's
/// frame.
pub const RADIUS: f32 = 0.077;
/// How far the duck's silhouette reaches from the middle of its body, in
/// radii: behind it to its tail, before it to the tip of its bill and to its
/// chest, and above it to the top of its head. And how much further its bill
/// reaches when it leans all the way. The renderer's tests hold these to the
/// silhouette it draws.
pub const BACK: f32 = 1.98;
pub const FRONT: f32 = 2.31;
pub const CHEST: f32 = 1.32;
pub const TOP: f32 = 1.97;
pub const LEAN: f32 = 0.66;
/// A duck's density, as in its own visit.
const DENSITY: f32 = 0.45;

/// The gap between the duck's chest and the case it holds, how far off the
/// ground it holds it, and how high the case swings over the duck as it
/// turns, as shares of the board's side.
const HOLD_GAP: f32 = 0.01;
const HOLD_LIFT: f32 = 0.03;
const SWING: f32 = 0.22;
/// How far beyond the edge of the view the duck and its case are when they
/// are out of it, as a share of the board's side.
const OUT: f32 = 0.05;
/// The door's height in the duck's radii, as a visiting duck's door has.
const DOOR_HEIGHT: f32 = 4.25;
/// How long the door takes to open and to close, in ticks, as a visiting
/// duck's does.
const DOOR_OPENING: u64 = 36;
const DOOR_CLOSING: u64 = 24;
/// How far open the door is when it is wide open, in steps of which it
/// gains as many a tick as lets it open in its time, and loses as many as
/// lets it close in its own.
const DOOR_OPEN: u64 = DOOR_OPENING * DOOR_CLOSING;
/// How long the duck takes to set the case down or pick it up, and to turn
/// with it, in ticks.
pub const LOWER_TICKS: u64 = 24;
pub const TURN_TICKS: u64 = 36;
/// How much of its remaining way the duck would cover in a second as it
/// comes to a stop, so that it slows into its place.
const APPROACH: f32 = 10.0;
/// How near its place the duck is to have arrived, in radii.
const ARRIVED: f32 = 0.1;
/// How long a walk may take before the duck is late: this many times what it
/// takes with nothing in the way, and this many ticks more.
const LATE: f32 = 1.5;
const LATE_TICKS: u64 = 2 * FIXED_HZ as u64;
/// How fast the duck's feet go, in radii a second, for it to be walking.
const WALKING: f32 = 0.1;
/// How far below the ground the middle of the duck's body is, in radii, for
/// it to have fallen out of the scene.
const FALLEN: f32 = 2.0;
/// How much of a lean the duck gains or loses in a tick.
const LEAN_RATE: f32 = 0.1;
/// How long each nod over the pieces lasts, down and up, in ticks.
const NOD_TICKS: u64 = 30;
/// How long a hop lasts in ticks, how many the winner takes, and how high
/// each goes in the pixels of the duck's art.
const HOP_TICKS: u64 = 18;
const HOPS: u64 = 3;
const HOP_HEIGHT: f32 = 1.5;
/// Ticks between a walking duck's steps.
const STEP_TICKS: u64 = 6;

/// What the duck is doing with the board's case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    /// Walks in with the case, to where the board is to stand.
    Bringing,
    /// Sets the case down there.
    Lowering,
    /// Steps back to its seat beside the board.
    Retiring,
    /// Sits at the board.
    Seated,
    /// Steps up to the case.
    Fetching,
    /// Picks the case up.
    Lifting,
    /// Turns with the case to the edge it came from.
    Turning,
    /// Walks out with the case.
    Taking,
    /// Out of the view and of the arena.
    Gone,
}

/// The duck of a visit to the board. See the [module documentation](self).
pub struct Duck {
    side: Side,
    /// Which way the board lies from the duck along x, as a sign.
    toward: f32,
    radius: f32,
    ground: f32,
    /// The view's edges, between which the floor is.
    view: [f32; 2],
    /// The frame's width, which is the door's.
    frame: f32,
    /// Where the middle of the duck's body is when it is out of the view
    /// with its case before it, where it sets the case down, and at its
    /// seat. From its seat the duck's bill reaches as far as the inner edge
    /// of the board's frame when it leans all the way, so it covers no
    /// square.
    outside: f32,
    place: f32,
    seat: f32,
    /// The case standing where the board is to stand.
    rest: Area,
    /// How far the middle of the case is before the middle of the duck that
    /// holds it.
    reach: f32,
    /// How far off the ground the duck holds the case, and how high the
    /// case swings as the duck turns.
    lift: f32,
    swing: f32,
    movement: Movement,
    task: Task,
    /// Ticks into the task.
    ticks: u64,
    /// The ticks the task has to be done in, if it is a walk.
    allowed: Option<u64>,
    /// Where the duck stands while it turns.
    stand: f32,
    /// The middle of the body and how it moves, as of the last step.
    position: Vec2,
    velocity: Vec2,
    grounded: bool,
    /// The body is in the arena.
    present: bool,
    /// The body has left the scene some other way than the duck walks out.
    lost: bool,
    /// How far the door is open, from 0 to `DOOR_OPEN`.
    door: u64,
    /// What the visit has the duck do at the board, and for how many ticks.
    pose: Pose,
    pose_ticks: u64,
    /// How far the duck leans toward the board, from 0 to 1.
    lean: f32,
    /// Ticks since the duck came.
    age: u64,
}

impl Duck {
    /// The duck of a visit on `stage`, out of the view on its side with the
    /// case, and its body in `world`.
    pub(crate) fn new(stage: &Stage, layout: Layout, world: &mut PhysicsWorld) -> Self {
        let side = stage.duck;
        let toward = match side {
            Side::Left => 1.0,
            Side::Right => -1.0,
        };
        let board = stage.side();
        let radius = board * RADIUS;
        let rest = stage.case();
        let reach = radius * CHEST + board * HOLD_GAP + rest.width() * 0.5;
        let view = [layout.bounds_min.x, layout.bounds_max.x];
        let edge = view[side as usize];
        let outside = edge - toward * (reach + rest.width() * 0.5 + board * OUT);
        let place = rest.center().x - toward * reach;
        let near = match side {
            Side::Left => stage.board.min.x,
            Side::Right => stage.board.max.x,
        };
        let frame = board * engine_chess_board::FRAME_SHARE;
        // As far back as it may stop short of its seat.
        let seat = near - toward * (radius * (FRONT + LEAN + ARRIVED) - frame);
        let ground = stage.ground();
        let position = Vec2::new(outside, ground + radius);
        let mut hull = ColliderSpec::ball(COLLIDER, radius);
        hull.density = DENSITY;
        hull.friction = 0.0;
        hull.restitution = 0.0;
        hull.collision_groups = crate::arena::BIRD;
        assert!(
            world.insert_body(
                BODY,
                BodySpec {
                    position,
                    lock_rotation: true,
                    gravity_scale: 0.0,
                    can_sleep: false,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[hull],
            ),
            "the duck enters the arena once"
        );
        let movement = Movement::new(view[1] - view[0], radius);
        let mut duck = Self {
            side,
            toward,
            radius,
            ground,
            view,
            frame: layout.frame_width,
            outside,
            place,
            seat,
            rest,
            reach,
            lift: board * HOLD_LIFT,
            swing: board * SWING,
            movement,
            task: Task::Bringing,
            ticks: 0,
            allowed: None,
            stand: outside,
            position,
            velocity: Vec2::ZERO,
            grounded: false,
            present: true,
            lost: false,
            door: 0,
            pose: Pose::Arriving,
            pose_ticks: 0,
            lean: 0.0,
            age: 0,
        };
        duck.begin(Task::Bringing);
        duck
    }

    pub fn task(&self) -> Task {
        self.task
    }

    /// Ticks into the task.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The radius of the duck's body.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// The middle of the duck's body.
    pub fn position(&self) -> Vec2 {
        self.position
    }

    /// Whether the duck's body is in the arena, so that the duck is there to
    /// be drawn.
    pub fn present(&self) -> bool {
        self.present
    }

    /// Whether the duck's body left the scene some other way than the duck
    /// walks out: it fell from it, or is no longer in the arena.
    pub fn lost(&self) -> bool {
        self.lost
    }

    /// Whether the duck has had longer for its walk than the walk is
    /// allowed.
    pub fn late(&self) -> bool {
        self.allowed.is_some_and(|allowed| self.ticks > allowed)
    }

    /// Whether the duck is where its task takes it: at the board's place
    /// with the case or for it, at its seat, or out of the view.
    pub fn arrived(&self) -> bool {
        let near = |place: f32| (self.position.x - place).abs() <= self.radius * ARRIVED;
        match self.task {
            Task::Bringing | Task::Fetching => near(self.place),
            Task::Retiring | Task::Seated => near(self.seat),
            Task::Taking => self.toward * (self.edge() - self.position.x) >= self.radius * BACK,
            Task::Lowering | Task::Lifting | Task::Turning | Task::Gone => false,
        }
    }

    /// Whether the duck's body is clear of `board`, beside it.
    pub fn beside(&self, board: Area) -> bool {
        let edge = if self.toward > 0.0 {
            board.min.x
        } else {
            board.max.x
        };
        self.toward * (edge - self.position.x) >= self.radius * (1.0 + ARRIVED)
    }

    /// Whether the duck has done what its task takes a set time for: the
    /// case is down, or up, or the duck has turned with it.
    pub fn finished(&self) -> bool {
        match self.task {
            Task::Lowering | Task::Lifting => self.ticks >= LOWER_TICKS,
            Task::Turning => self.ticks >= TURN_TICKS,
            _ => false,
        }
    }

    /// Sets the duck to `task`, from its start.
    pub fn begin(&mut self, task: Task) {
        self.task = task;
        self.ticks = 0;
        self.stand = self.position.x;
        let walk = |from: f32, to: f32| {
            let ticks = (to - from).abs() / self.movement.walk_speed * FIXED_HZ as f32;
            Some((ticks * LATE).ceil() as u64 + LATE_TICKS)
        };
        self.allowed = match task {
            // The door opens first.
            Task::Bringing => walk(self.outside, self.place).map(|ticks| ticks + DOOR_OPENING),
            Task::Retiring => walk(self.place, self.seat),
            Task::Fetching => walk(self.position.x, self.place),
            Task::Taking => walk(self.position.x, self.outside).map(|ticks| ticks + DOOR_OPENING),
            _ => None,
        };
    }

    /// Has the duck pick the case back up from as far down as it has set it,
    /// instead of setting it down.
    pub fn lift_again(&mut self) {
        assert_eq!(self.task, Task::Lowering, "a duck setting its case down");
        let down = self.ticks.min(LOWER_TICKS);
        self.begin(Task::Lifting);
        self.ticks = LOWER_TICKS - down;
    }

    /// What the duck does at the board from now on.
    pub fn set_pose(&mut self, pose: Pose) {
        if self.pose != pose {
            self.pose = pose;
            self.pose_ticks = 0;
        }
    }

    pub fn pose(&self) -> Pose {
        self.pose
    }

    /// Takes the duck out of `world`, wherever it is.
    pub fn remove(&mut self, world: &mut PhysicsWorld) {
        if self.present {
            world.remove_entity(ENTITY);
            self.present = false;
        }
        self.begin(Task::Gone);
    }

    fn edge(&self) -> f32 {
        self.view[self.side as usize]
    }

    /// How far into the view the door swings as it opens: a duck within this
    /// of the edge is in its way.
    fn doorway(&self) -> f32 {
        self.radius * DOOR_HEIGHT + self.frame
    }

    /// How far inside the view the duck is from the edge it came by, at the
    /// nearest of its tail and the case it holds before it. Negative while
    /// any of it is beyond the edge.
    fn inside(&self) -> f32 {
        let tail = self.position.x - self.facing() * self.radius * BACK;
        let nose = self.position.x + self.facing() * (self.reach + self.rest.width() * 0.5);
        (self.toward * (tail - self.edge())).min(self.toward * (nose - self.edge()))
    }

    /// Where the duck steers to in the step to come. It keeps out of the
    /// doorway until the door has opened.
    fn heading(&self) -> f32 {
        let open = self.door >= DOOR_OPEN;
        match self.task {
            // Once it is in and clear of the door, the door closes behind it.
            Task::Bringing if open || self.inside() >= self.doorway() => self.place,
            Task::Bringing => self.outside,
            Task::Lowering | Task::Lifting | Task::Fetching => self.place,
            Task::Retiring | Task::Seated => self.seat,
            Task::Turning | Task::Gone => self.stand,
            Task::Taking if open => self.outside,
            Task::Taking => {
                // No nearer the edge than leaves its case clear of the door.
                let clear = self.edge()
                    + self.toward * (self.doorway() + self.reach + self.rest.width() * 0.5);
                if self.toward * (self.position.x - clear) > 0.0 {
                    clear
                } else {
                    self.position.x
                }
            }
        }
    }

    /// Steers the duck's body toward where it is going, for the step to
    /// come. The host calls this before the arena's step.
    pub fn drive(&mut self, world: &mut PhysicsWorld) {
        if !self.present {
            return;
        }
        let away = self.heading() - self.position.x;
        let speed = (away.abs() * APPROACH).min(self.movement.walk_speed);
        let command = Command {
            gait: Gait::Pace(speed / self.movement.run_speed),
            direction: if away == 0.0 { 0.0 } else { away.signum() },
            jump: false,
        };
        let observed = Observation {
            position: self.position,
            velocity: self.velocity,
            support_velocity: Vec2::ZERO,
            debris: None,
            grounded: self.grounded,
            blocked: false,
            support: None,
        };
        let mut delta = self.movement.velocity_delta(observed, &command);
        // The floor ends at the edges of the view. Over it the duck brings
        // its own pull, and beyond it the duck walks level.
        delta.y = if (self.view[0]..=self.view[1]).contains(&self.position.x) {
            -self.movement.gravity * DT
        } else {
            -self.velocity.y
        };
        // A body no longer in the arena is lost, as the next reading says.
        if !world.apply_velocity_delta(BODY, delta, true) {
            self.lost = true;
        }
    }

    /// Reads the duck's body back from `world` after the arena's step, and
    /// takes a tick of what it is doing.
    pub fn observe(&mut self, world: &PhysicsWorld) {
        self.age += 1;
        self.ticks += 1;
        self.pose_ticks += 1;
        if self.present {
            match world.motion(BODY) {
                Some(motion)
                    if motion.position.x.is_finite()
                        && motion.position.y.is_finite()
                        && motion.position.y >= self.ground - self.radius * FALLEN =>
                {
                    self.position = motion.position;
                    self.velocity = motion.linear_velocity;
                    self.grounded = world.surface_contacts(COLLIDER).any(|contact| {
                        contact.normal.y > 0.7 && contact.separation <= self.radius * 0.05
                    });
                }
                _ => self.lost = true,
            }
        }
        // The door is open for a duck in the doorway or on its way out.
        let wanted = self.present
            && (matches!(self.task, Task::Turning | Task::Taking)
                || self.inside() < self.doorway());
        self.door = if wanted {
            (self.door + DOOR_CLOSING).min(DOOR_OPEN)
        } else {
            self.door.saturating_sub(DOOR_OPENING)
        };
        let wanted = self.lean_wanted();
        self.lean += (wanted - self.lean).clamp(-LEAN_RATE, LEAN_RATE);
    }

    /// How far the duck means to lean now.
    fn lean_wanted(&self) -> f32 {
        match self.task {
            Task::Lowering | Task::Lifting => 1.0,
            Task::Seated => match self.pose {
                Pose::Moving | Pose::Lost => 1.0,
                // A nod over each few pieces as they come out or go back.
                Pose::Setting | Pose::Packing => {
                    f32::from(self.pose_ticks % NOD_TICKS < NOD_TICKS / 2)
                }
                // One slow nod to a drawn game.
                Pose::Drawn => f32::from(self.pose_ticks < NOD_TICKS),
                _ => 0.0,
            },
            _ => 0.0,
        }
    }

    /// Which way the duck faces along x, as a sign: toward the board, until
    /// it has turned half way to the edge it came from.
    pub fn facing(&self) -> f32 {
        if self.turned() < 0.5 {
            self.toward
        } else {
            -self.toward
        }
    }

    /// How far the duck has turned with its case from the board to the edge
    /// it came from, from 0 to 1.
    fn turned(&self) -> f32 {
        match self.task {
            Task::Turning => ease(self.ticks as f32 / TURN_TICKS as f32),
            Task::Taking | Task::Gone => 1.0,
            _ => 0.0,
        }
    }

    /// Whether the duck's feet are moving.
    fn walking(&self) -> bool {
        self.present && self.velocity.x.abs() > self.radius * WALKING
    }

    /// How far each of the duck's feet is from where it rests, in the pixels
    /// of its art: none standing, and none or one by turns walking.
    pub fn stride(&self) -> f32 {
        if self.walking() {
            ((self.age / STEP_TICKS) % 2) as f32
        } else {
            0.0
        }
    }

    /// How far the duck leans the way it faces, from 0 to 1.
    pub fn lean(&self) -> f32 {
        self.lean
    }

    /// Whether the duck sits at the board leaning all the way over it, its
    /// bill at the board's frame.
    pub fn reached(&self) -> bool {
        self.task == Task::Seated && self.lean >= 1.0
    }

    /// How far off the ground the duck is drawn, in the pixels of its art:
    /// the winner's few hops. Its body stays where it is.
    pub fn hop(&self) -> f32 {
        if self.task != Task::Seated
            || self.pose != Pose::Won
            || self.pose_ticks >= HOPS * HOP_TICKS
        {
            return 0.0;
        }
        let through = (self.pose_ticks % HOP_TICKS) as f32 / HOP_TICKS as f32;
        (through * PI).sin() * HOP_HEIGHT
    }

    /// The case as the duck has it: held before it, on its way down to
    /// where the board is to stand or up from there, or swinging over the
    /// duck as it turns. None while the case stands on the ground.
    pub fn case(&self) -> Option<Area> {
        let held = {
            let around = self.turned() * PI;
            let half = self.rest.height() * 0.5;
            // The case bobs with the duck's steps.
            let bob = self.stride() * self.radius * 0.1;
            Vec2::new(
                self.position.x + self.toward * around.cos() * self.reach,
                self.ground + self.lift + half + around.sin() * self.swing + bob,
            )
        };
        let down = match self.task {
            Task::Retiring | Task::Seated | Task::Fetching => return None,
            Task::Lowering => ease(self.ticks as f32 / LOWER_TICKS as f32),
            Task::Lifting => 1.0 - ease(self.ticks as f32 / LOWER_TICKS as f32),
            Task::Bringing | Task::Turning | Task::Taking | Task::Gone => 0.0,
        };
        let middle = held + (self.rest.center() - held) * down;
        let half = (self.rest.max - self.rest.min) * 0.5;
        Some(Area::new(middle - half, middle + half))
    }

    /// The door the duck comes and goes through, while it is at all open.
    pub(crate) fn door(&self) -> Option<DoorPanel> {
        if self.door == 0 {
            return None;
        }
        let top = self.ground + self.radius * DOOR_HEIGHT;
        let open = self.door as f32 / DOOR_OPEN as f32;
        Some(DoorPanel {
            bottom: self.ground,
            top,
            hinge: Vec2::new(self.edge(), top),
            angle: self.toward * ease(open) * FRAC_PI_2,
            width: self.frame,
            inward: self.toward,
        })
    }

    /// Where the door is drawn while it is at all open: the frame on the
    /// duck's side, and as far into the view as the door swings.
    pub fn door_area(&self) -> Option<Area> {
        if self.door == 0 {
            return None;
        }
        let edge = self.edge();
        let reach = edge + self.toward * self.doorway();
        Some(Area::new(
            Vec2::new(edge.min(reach), self.ground),
            Vec2::new(
                edge.max(reach),
                self.ground + self.radius * DOOR_HEIGHT + self.frame,
            ),
        ))
    }

    /// Whether the door has closed.
    pub fn door_closed(&self) -> bool {
        self.door == 0
    }
}

/// Slow at either end, as a thing lifted and laid down.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
