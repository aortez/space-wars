use super::*;
use engine_rapier::terrain::{PreparedRelease, RadialImpulse, TerrainBodyMut};
use engine_terrain::{Brush, EditMode, TerrainEdit};

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
            if y <= -10
                || (x <= -6 && y <= 7)
                || ((-5..=6).contains(&x) && (4..=7).contains(&y))
                || ((4..=6).contains(&x) && y <= 3)
            {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .expect("bounded bank fixture")
}

impl ScorchedState {
    pub(super) fn cut_bank_support(&mut self) {
        let body = &mut self.terrain[0];
        let edits: Vec<_> = (-7..=3)
            .flat_map(|y| (4..=6).map(move |x| (x, y)))
            .map(|(x, y)| TerrainEdit {
                brush: Brush::Circle {
                    center: body
                        .terrain
                        .local_to_cell(Vec2::new(x as f32, y as f32))
                        .unwrap(),
                    radius: 0,
                },
                mode: EditMode::Remove,
            })
            .collect();
        let plan = PreparedRelease::new(&body.terrain, &edits).expect("bounded support cut");
        let capacity = self.loose.release_capacity(plan.grain_count());
        if !capacity.fits() {
            self.last_capacity_rejection = Some(capacity);
            self.rejected_blasts += 1;
            return;
        }
        let previous = self.loose.len();
        let release = self
            .loose
            .commit(
                &mut self.physics,
                TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                },
                plan,
                &mut self.next_id,
            )
            .expect("admitted support cut");
        body.hash = body.terrain.hash();
        body.edited_chunks = release.dirty_chunks;
        self.terrain.extend(release.fragments);
        // Eject the cut samples into the clearing. The remaining bank receives
        // no impulse: its subsequent failure is caused only by self-weight.
        RadialImpulse {
            center: Vec2::new(0.0, -2.0),
            radius: 12.0,
            speed: 12.0,
        }
        .apply(
            &mut self.physics,
            self.loose.iter().skip(previous).map(|g| g.body()),
        );
        self.impacts += 1;
    }
}
