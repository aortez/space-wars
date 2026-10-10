//! A playable undermining encounter. The demo only aims and fires the ordinary
//! tank weapon; every cut, falling clump and chip comes from the shared engine.
use super::*;

pub(super) fn terrain(seed: u64) -> Terrain {
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
            let phase = (seed % 1000) as f32 * 0.006;
            let roughness = (x as f32 * 0.31 + phase).sin().round() as i32;
            let hill =
                7 + roughness - ((-18 - x).max(0) / 5) - ((x + 8).max(0) / 5) - (x - 4).max(0) * 3;
            // A filled hillside: the gun must excavate its own undercut. The
            // uneven top and circular blast cuts leave sloping, tapered roofs.
            if y <= hill.max(-10) {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

impl ScorchedConfig {
    /// Apply the same combined preset offered by Spacewars' dirt settings.
    pub fn with_destruction(mut self, strength: BankStrength) -> Self {
        let preset = strength.apply(LooseTerrainConfig::default());
        self.cohesion = strength;
        self.slumping = preset.slumping.is_some();
        self.breakup = preset.impact_fracture.is_some();
        self
    }
}

impl ScorchedState {
    pub(super) fn aim_at_hillside(&mut self) {
        // Solve only the initial aim. Once fired, the ordinary swept shell can
        // hit any terrain, grain or actor along its actual trajectory.
        self.tanks[1].elevation = 34.0_f32.to_radians();
        let (_, muzzle) = self.muzzle(1);
        let target = Vec2::new(4.0, 0.25);
        let dx = (muzzle.x - target.x).abs();
        let dy = target.y - muzzle.y;
        let angle = self.tanks[1].elevation;
        let denominator = 2.0 * angle.cos().powi(2) * (dx * angle.tan() - dy);
        if denominator > 0.0 {
            self.tanks[1].speed = (GRAVITY * dx * dx / denominator).sqrt().clamp(12.0, 48.0);
        }
    }
    pub(super) fn advance_hillside(&mut self) {
        if self.config.demo && [120, 600, 1080].contains(&self.tick) {
            self.aim_at_hillside();
            self.fire(1);
        }
    }
}
