//! The crow at the chess board.
//!
//! The crow flies in over the edge of the view on its side, comes down in
//! the room beside the board and plays from its seat there. It is the Clock's
//! crow in another role: the same body under the same silhouette, flown by
//! the same controller. Nothing of a crow's own visit comes with it. It has
//! no perch and no errand of its own, it keeps the size the board gives it,
//! and it stays for as long as the visit to the board does.
//!
//! Its body is a bird's disc in the arena, in the chess visit's claim, with
//! its bottom at the crow's feet. It feels none of the world's gravity: the
//! controller brings the pull the crow flies against. Before each step the
//! body takes the velocity the controller chose. The step moves it, and
//! whatever it meets stops it, pushes it or is pushed by it. After the step
//! the controller reads the body back and chooses again. It is one of the
//! arena's birds, which the walls at the edges of the view let through.
//!
//! The crow flies in along its way, the band of air the stage keeps for it
//! from the edge of the view to the board, and comes down to its seat from
//! there. To leave it climbs back into its way and flies out as it came.
//! Pushed from its seat, it steps back to it, and knocked off its feet, it
//! takes to the air and comes down to it again.
//!
//! Each flight has a time to be done in. The visit decides what a crow that
//! is late does next.

use std::f32::consts::PI;

use engine_core::Vec2;
use engine_rapier::world::PhysicsWorld;

use super::Pose;
use super::stage::{Side, Stage};
use crate::crow::body::{self, Bird};
use crate::crow::flight::{self, Flight};
use crate::events::FIXED_HZ;
use crate::layout::Layout;

#[cfg(test)]
pub(crate) mod tests;

/// The crow's body in the arena.
const BODY: Bird = body::CHESS_CROW;

/// The pitch the crow is drawn and flown at, as a share of the board's side.
/// A crow at the face is as large as the face's pitch makes it. This one is
/// as large as lets it come down within its room, and lean from its seat as
/// far as the board's frame.
pub const PITCH: f32 = 0.215;
/// How far the crow's silhouette reaches from its feet, in its pitches:
/// behind it to its tail, before it to the tip of its bill, above them to
/// the top of its head, and to the tip of its wing raised all the way. And
/// how much further its bill reaches when it leans all the way. The
/// renderer's tests hold these to the silhouette it draws.
pub const BACK: f32 = 0.735;
pub const FRONT: f32 = 0.735;
pub const TOP: f32 = 1.155;
pub const WING: f32 = 1.365;
pub const LEAN: f32 = 0.21;
/// How far below the height it flies at the crow's feet may sink, and how
/// far above it they may rise, as it flies along its way, in its pitches.
/// The crow's tests hold its flights to these.
pub const SINK: f32 = 0.1;
pub const RISE: f32 = 0.4;
/// How tall the crow's way is, in its pitches: room for its feet to sink and
/// rise, and for its raised wing above them.
pub const WAY: f32 = SINK + RISE + WING;

/// How far beyond the edge of the view the crow's feet are when it is out
/// of it, and how far beyond it they are for the crow to have left, in its
/// pitches.
const OUT: f32 = 2.0;
const AWAY: f32 = 1.5;
/// How near its seat the crow's feet stand once it has settled there, in
/// its pitches.
const SETTLED: f32 = 0.03;
/// How far the feet may be from the seat they come down to, across and up
/// or down, in pitches, for the crow to stand there, as for a crow at the
/// face.
const SPOT_REACH: f32 = 0.35;
const SPOT_DEPTH: f32 = 0.25;
/// How far a standing crow may be pushed across with nothing under its
/// feet, in pitches, before it has been knocked off them.
const KNOCKED_OFF: f32 = 0.5;
/// How much of its way back to its seat a standing crow would cover in a
/// second, and how fast it steps at most, in pitches a second.
const APPROACH: f32 = 10.0;
const STEP_SPEED: f32 = 2.0;
/// How long a flight in or out may take, in ticks: about twice as long as
/// either takes with nothing in the way.
pub const FLIGHT_TICKS: u64 = 6 * FIXED_HZ as u64;
/// How fast across the crow flies, in pitches a second, for it to turn to
/// face the way it goes.
const TURNING: f32 = 0.2;
/// How far below the ground the crow's feet are, in pitches, for it to have
/// fallen out of the scene.
const FALLEN: f32 = 2.0;
/// How much of a lean the crow gains or loses in a tick.
const LEAN_RATE: f32 = 0.1;
/// How long each nod over the pieces lasts, down and up, in ticks.
const NOD_TICKS: u64 = 30;
/// How long a hop lasts in ticks, how many the winner takes, and how high
/// each goes in the pixels of the crow's art.
const HOP_TICKS: u64 = 18;
const HOPS: u64 = 3;
const HOP_HEIGHT: f32 = 2.0;

/// What the crow is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    /// Flies in to its seat.
    Coming,
    /// Stands at its seat.
    Seated,
    /// Flies out the way it came.
    Going,
    /// Out of the view and of the arena.
    Gone,
}

/// The crow of a visit to the board. See the [module documentation](self).
pub struct Crow {
    /// Which way the board lies from the crow along x, as a sign.
    toward: f32,
    /// The view as the crow flies in it: the Clock's, at the crow's own
    /// pitch.
    layout: Layout,
    ground: f32,
    /// The edge of the view on the crow's side.
    edge: f32,
    /// Where the crow's feet are when it is out of the view, and at its
    /// seat. From its seat the crow's bill reaches as far as the inner edge
    /// of the board's frame when it leans all the way, so it covers no
    /// square.
    outside: Vec2,
    seat: Vec2,
    /// The height the crow's feet fly at along its way.
    cruise: f32,
    flight: Flight,
    task: Task,
    /// Ticks into the task.
    ticks: u64,
    /// The crow's feet, as of the last step.
    position: Vec2,
    facing_right: bool,
    /// The body is in the arena.
    present: bool,
    /// The body has left the scene some other way than the crow flies out.
    lost: bool,
    /// What the visit has the crow do at the board, and for how many ticks.
    pose: Pose,
    pose_ticks: u64,
    /// How far the crow leans toward the board, from 0 to 1.
    lean: f32,
}

impl Crow {
    /// The crow of a visit on `stage`, out of the view on its side, and its
    /// body in `world`.
    pub(crate) fn new(stage: &Stage, layout: Layout, world: &mut PhysicsWorld) -> Self {
        let side = stage.duck.other();
        let toward = match side {
            Side::Left => 1.0,
            Side::Right => -1.0,
        };
        let board = stage.side();
        let pitch = board * PITCH;
        let layout = Layout { pitch, ..layout };
        let ground = stage.ground();
        let edge = match side {
            Side::Left => layout.bounds_min.x,
            Side::Right => layout.bounds_max.x,
        };
        let near = match side {
            Side::Left => stage.board.min.x,
            Side::Right => stage.board.max.x,
        };
        let frame = board * engine_chess_board::FRAME_SHARE;
        // As far back as it may settle short of its seat.
        let seat = Vec2::new(
            near - toward * (pitch * (FRONT + LEAN + SETTLED) - frame),
            ground,
        );
        let cruise = stage.way(side).min.y + pitch * SINK;
        let outside = Vec2::new(edge - toward * pitch * OUT, cruise);
        let mut flight = Flight::default();
        flight.retarget(outside, false, layout);
        BODY.insert(world, outside, Vec2::ZERO, layout);
        Self {
            toward,
            layout,
            ground,
            edge,
            outside,
            seat,
            cruise,
            flight,
            task: Task::Coming,
            ticks: 0,
            position: outside,
            facing_right: toward > 0.0,
            present: true,
            lost: false,
            pose: Pose::Arriving,
            pose_ticks: 0,
            lean: 0.0,
        }
    }

    pub fn task(&self) -> Task {
        self.task
    }

    /// Ticks into the task.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The pitch the crow is drawn and flown at.
    pub fn pitch(&self) -> f32 {
        self.layout.pitch
    }

    /// The crow's feet.
    pub fn position(&self) -> Vec2 {
        self.position
    }

    /// Where the crow's feet stand at its seat.
    pub fn seat(&self) -> Vec2 {
        self.seat
    }

    /// Where the crow is going, while it is on its way to its seat.
    pub fn destination(&self) -> Option<Vec2> {
        (self.task == Task::Coming).then_some(self.seat)
    }

    /// Whether the crow's body is in the arena, so that the crow is there to
    /// be drawn.
    pub fn present(&self) -> bool {
        self.present
    }

    /// Whether the crow's body left the scene some other way than the crow
    /// flies out: it fell from it, or is no longer in the arena.
    pub fn lost(&self) -> bool {
        self.lost
    }

    /// Whether the crow has had longer for its flight than the flight is
    /// allowed.
    pub fn late(&self) -> bool {
        matches!(self.task, Task::Coming | Task::Going) && self.ticks > FLIGHT_TICKS
    }

    /// Whether the crow on its way out is out of the view.
    pub fn away(&self) -> bool {
        self.task == Task::Going
            && self.toward * (self.edge - self.position.x) >= self.layout.pitch * AWAY
    }

    /// Has the crow fly out the way it came, from wherever it is.
    pub fn leave(&mut self) {
        let grounded = match self.task {
            Task::Coming => false,
            Task::Seated => true,
            Task::Going | Task::Gone => return,
        };
        self.begin(Task::Going);
        self.flight.retarget(self.position, grounded, self.layout);
    }

    fn begin(&mut self, task: Task) {
        self.task = task;
        self.ticks = 0;
    }

    /// What the crow does at the board from now on.
    pub fn set_pose(&mut self, pose: Pose) {
        if self.pose != pose {
            self.pose = pose;
            self.pose_ticks = 0;
        }
    }

    pub fn pose(&self) -> Pose {
        self.pose
    }

    /// Takes the crow out of `world`, wherever it is.
    pub fn remove(&mut self, world: &mut PhysicsWorld) {
        if self.present {
            BODY.remove(world);
            self.present = false;
        }
        self.begin(Task::Gone);
    }

    /// Gives the crow's body the velocity the crow chose, for the step to
    /// come. The host calls this before the arena's step.
    pub fn drive(&mut self, world: &mut PhysicsWorld) {
        if !self.present {
            return;
        }
        // A body no longer in the arena is lost, as the next reading says.
        if BODY.motion(world).is_some() {
            BODY.drive(world, self.flight.velocity);
        }
    }

    /// Reads the crow's body back from `world` after the arena's step, takes
    /// a tick of what it is doing, and chooses the velocity its body takes
    /// into the next step.
    pub fn observe(&mut self, world: &PhysicsWorld) {
        self.ticks += 1;
        self.pose_ticks += 1;
        if !self.present {
            return;
        }
        let pitch = self.layout.pitch;
        match BODY.motion(world) {
            Some((position, velocity))
                if position.x.is_finite()
                    && position.y.is_finite()
                    && position.y >= self.ground - pitch * FALLEN =>
            {
                self.position = position;
                self.flight.velocity = velocity;
            }
            _ => {
                self.lost = true;
                return;
            }
        }
        let footing = BODY.footing(world, self.layout);
        match self.task {
            Task::Coming => {
                if footing && self.on_seat() && self.flight.touchdown(self.layout) {
                    // Its feet are down at its seat: it stands there, facing
                    // the board.
                    self.flight = Flight::default();
                    self.facing_right = self.toward > 0.0;
                    self.begin(Task::Seated);
                } else {
                    // Whatever else its feet came down on holds them, and
                    // the wings start again from there.
                    if footing && !self.on_seat() {
                        self.flight.retarget(self.position, true, self.layout);
                    }
                    self.fly(self.seat, false);
                }
            }
            Task::Seated => {
                if !footing && self.off_seat() {
                    // Something knocked it off its feet. It takes to the air
                    // and comes down to its seat again.
                    self.begin(Task::Coming);
                    self.flight.retarget(self.position, false, self.layout);
                    self.fly(self.seat, false);
                } else {
                    // Standing keeps its weight on its feet. With them on
                    // the ground it steps back to its seat from wherever it
                    // was pushed.
                    if footing {
                        let away = self.seat.x - self.position.x;
                        let most = pitch * STEP_SPEED;
                        self.flight.velocity.x = (away * APPROACH).clamp(-most, most);
                    }
                    self.flight.velocity.y -= flight::gravity(self.layout) * flight::DT;
                }
            }
            Task::Going => {
                if footing {
                    self.flight.retarget(self.position, true, self.layout);
                }
                self.fly(self.outside, true);
            }
            Task::Gone => {}
        }
        let wanted = self.lean_wanted();
        self.lean += (wanted - self.lean).clamp(-LEAN_RATE, LEAN_RATE);
    }

    /// Steers the crow's flight toward `to` along its way, and turns it to
    /// face the way it goes.
    fn fly(&mut self, to: Vec2, leaving: bool) {
        self.flight
            .steer(self.position, to, self.cruise, self.layout, leaving);
        if self.flight.velocity.x.abs() > self.layout.pitch * TURNING {
            self.facing_right = self.flight.velocity.x > 0.0;
        }
    }

    /// Whether the feet are at the seat the crow is coming down to.
    fn on_seat(&self) -> bool {
        let pitch = self.layout.pitch;
        (self.position.x - self.seat.x).abs() <= pitch * SPOT_REACH
            && (self.position.y - self.seat.y).abs() <= pitch * SPOT_DEPTH
    }

    /// Whether the feet have been pushed or have fallen from the seat the
    /// crow stands at.
    fn off_seat(&self) -> bool {
        let pitch = self.layout.pitch;
        (self.position.x - self.seat.x).abs() > pitch * KNOCKED_OFF
            || self.position.y < self.seat.y - pitch * SPOT_DEPTH
    }

    /// Whether the crow stands settled at its seat.
    pub fn settled(&self) -> bool {
        self.task == Task::Seated
            && (self.position.x - self.seat.x).abs() <= self.layout.pitch * SETTLED
    }

    /// How far the crow means to lean now.
    fn lean_wanted(&self) -> f32 {
        if self.task != Task::Seated {
            return 0.0;
        }
        match self.pose {
            Pose::Moving => 1.0,
            // A nod over each few pieces as they come out or go back, by
            // turns with the duck's.
            Pose::Setting | Pose::Packing => {
                f32::from(self.pose_ticks % NOD_TICKS >= NOD_TICKS / 2)
            }
            // One slow nod to a drawn game.
            Pose::Drawn => f32::from(self.pose_ticks < NOD_TICKS),
            _ => 0.0,
        }
    }

    pub fn facing_right(&self) -> bool {
        self.facing_right
    }

    /// How far the crow leans the way it faces, from 0 to 1.
    pub fn lean(&self) -> f32 {
        self.lean
    }

    /// Whether the crow hangs its head to the ground: it has lost.
    pub fn bowed(&self) -> bool {
        self.task == Task::Seated && self.pose == Pose::Lost
    }

    /// Whether the winner is still taking its hops.
    fn hopping(&self) -> bool {
        self.task == Task::Seated && self.pose == Pose::Won && self.pose_ticks < HOPS * HOP_TICKS
    }

    /// How far off the ground the crow is drawn, in the pixels of its art:
    /// the winner's few hops. Its body stays where it is.
    pub fn hop(&self) -> f32 {
        if !self.hopping() {
            return 0.0;
        }
        let through = (self.pose_ticks % HOP_TICKS) as f32 / HOP_TICKS as f32;
        (through * PI).sin() * HOP_HEIGHT
    }

    /// The crow's wing while it is spread, from raised, 1, to lowered, -1:
    /// as it beats in flight, and raised at the top of each of the winner's
    /// hops.
    pub fn wing(&self) -> Option<f32> {
        match self.task {
            Task::Coming | Task::Going => Some(self.flight.wing),
            Task::Seated if self.hopping() => {
                let through = (self.pose_ticks % HOP_TICKS) as f32 / HOP_TICKS as f32;
                Some(-(through * 2.0 * PI).cos())
            }
            Task::Seated | Task::Gone => None,
        }
    }
}
