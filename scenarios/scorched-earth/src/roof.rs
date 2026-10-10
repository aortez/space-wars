//! A bridge over a cave, weakened only by the ordinary tank weapon.
use super::*;

pub(super) fn terrain() -> Terrain {
    Terrain::generate(
        97,
        65,
        1.0,
        vec![Material {
            id: DIRT,
            hardness: 100,
        }],
        |c| {
            let x = c.x - 48;
            let y = c.y - 32;
            let top = 13 - ((x.abs() - 12).max(0) / 2);
            if y <= -10 || (y <= top && (x <= -8 || x >= 7 || y >= 8)) {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

impl ScorchedState {
    pub(super) fn aim_at_roof(&mut self) {
        self.tanks[1].elevation = 65.0_f32.to_radians();
        let (_, muzzle) = self.muzzle(1);
        let target = Vec2::new(-4.0, 8.0);
        let dx = (muzzle.x - target.x).abs();
        let dy = target.y - muzzle.y;
        let angle = self.tanks[1].elevation;
        let denominator = 2.0 * angle.cos().powi(2) * (dx * angle.tan() - dy);
        if denominator > 0.0 {
            self.tanks[1].speed = (GRAVITY * dx * dx / denominator).sqrt().clamp(12.0, 48.0);
        }
    }
    pub(super) fn advance_roof(&mut self) {
        if self.config.demo && self.tick == 120 {
            self.aim_at_roof();
            self.fire(1);
        }
    }
}
