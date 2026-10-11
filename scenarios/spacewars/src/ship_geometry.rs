//! Small authoring boundary for the ship renderer. Fixed artwork is identified
//! before its world transform; moving wings and other geometry remain ordinary
//! primitives. The renderer lab can observe this without changing RenderFrame's
//! serialized contract or introducing graphics/cache handles into the scenario.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    WingMount,
    Thruster,
    Hull,
    Laser,
    PodHull,
    PodThruster,
    PodLaser,
}

impl Part {
    pub const ALL: [Self; 7] = [
        Self::WingMount,
        Self::Thruster,
        Self::Hull,
        Self::Laser,
        Self::PodHull,
        Self::PodThruster,
        Self::PodLaser,
    ];

    /// Immutable local-space source points, not already transformed geometry.
    pub fn points(self) -> &'static [Vec2] {
        match self {
            Self::WingMount => &SHIP_WING_MOUNT,
            Self::Thruster => &SHIP_THRUSTER,
            Self::Hull => &SHIP_BODY,
            Self::Laser => &SHIP_LASER,
            Self::PodHull => &POD_BODY,
            Self::PodThruster => &POD_THRUSTER,
            Self::PodLaser => &POD_LASER,
        }
    }
}

pub trait Sink {
    fn push_primitive(&mut self, layer: i32, primitive: RenderPrimitive);

    fn rigid_polygon(
        &mut self,
        layer: i32,
        part: Part,
        transform: Transform2,
        fill: RenderColor,
        outline: RenderColor,
    ) {
        push_filled_polygon(self, layer, transform, part.points(), fill, outline);
    }
}

impl Sink for RenderFrame {
    fn push_primitive(&mut self, layer: i32, primitive: RenderPrimitive) {
        RenderFrame::push_primitive(self, layer, primitive);
    }
}

/// The production artwork, not a second benchmark-only ship implementation.
pub fn render(sink: &mut impl Sink, ship: &ShipState) {
    render_ship(sink, ship);
}

/// Prescribed visual state for renderer-only tests, with production weapon art.
/// Simulation and controller behavior are deliberately not benchmarked here.
pub fn specimen(index: usize, phase: f32, position: Vec2) -> ShipState {
    let mut ship = ShipState::new(
        index % 2,
        position,
        if index.is_multiple_of(2) {
            Color::RED
        } else {
            Color::GREEN
        },
        100,
        1.0 / 60.0,
    );
    ship.rotation_radians = phase * 0.6;
    ship.wing_theta = MAX_WING_THETA * (0.5 + 0.5 * phase.sin());
    if index % 4 == 3 {
        ship.form = ShipForm::EscapePod;
    } else {
        ship.enable_weapon_supply();
        // Include both loaded mounts and the moving, dim reload artwork.
        if index % 3 == 1 {
            let armament = ship.armament.as_mut().unwrap();
            armament.take_round();
            armament.advance(0.5 + 0.4 * phase.sin());
            armament.advance(0.5 + 0.4 * phase.sin());
        }
    }
    ship
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Observer {
        frame: RenderFrame,
        parts: Vec<Part>,
    }
    impl Sink for Observer {
        fn push_primitive(&mut self, z: i32, primitive: RenderPrimitive) {
            self.frame.push_primitive(z, primitive);
        }
        fn rigid_polygon(
            &mut self,
            z: i32,
            part: Part,
            t: Transform2,
            f: RenderColor,
            s: RenderColor,
        ) {
            self.parts.push(part);
            self.frame.rigid_polygon(z, part, t, f, s);
        }
    }

    #[test]
    fn observing_authored_parts_preserves_production_draw_order_and_geometry() {
        for i in 0..32 {
            let ship = specimen(i, i as f32 * 0.37, Vec2::new(21., -16.));
            let mut original = RenderFrame::default();
            render_ship(&mut original, &ship);
            let mut observer = Observer::default();
            render(&mut observer, &ship);
            assert_eq!(observer.frame, original);
            assert_eq!(
                observer.parts.len(),
                if ship.form == ShipForm::EscapePod {
                    3
                } else {
                    7
                }
            );
        }
    }
}
