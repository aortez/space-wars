//! A separate offline stream; samples are not passed to the bot or its sensors.
use scenario_spacewars::surface_sortie::SurfaceSortieState;
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

pub struct ProjectileTrace(BufWriter<File>);

impl ProjectileTrace {
    pub fn from_args(out: &Path) -> Option<Self> {
        crate::arg("--trace-projectiles", "false")
            .parse::<bool>()
            .unwrap()
            .then(|| {
                Self(BufWriter::new(
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(out.join("projectiles.jsonl"))
                        .unwrap(),
                ))
            })
    }

    pub fn observe(&mut self, state: &SurfaceSortieState) {
        for seat in 0..state.player_count() {
            serde_json::to_writer(
                &mut self.0,
                &json!({"schema":1, "tick":state.tick(), "seat":seat,
                    "diagnostic":state.projectile_diagnostics(seat)}),
            )
            .unwrap();
            writeln!(self.0).unwrap();
        }
    }

    pub fn finish(mut self) {
        self.0.flush().unwrap();
    }
}
