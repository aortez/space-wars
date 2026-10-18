//! Explicit normal-path diagnostic launch. Never saved as a user preference.
use clap::Args;
use engine_common::Scenario;
use scenario_pizza::{PizzaAction, PizzaEventKind, PizzaScenario, PizzaState, profile::Sample};
use std::{io::Write, path::PathBuf, time::Duration};

#[derive(Args, Debug, Default)]
#[group(id = "pizza_lab")]
pub(crate) struct Options {
    /// Opt-in healthy normal-physics cascade; requires an isolated --config-dir.
    #[arg(long)]
    pub pizza_lab_balls: Option<usize>,
    #[arg(long, default_value_t = 500, requires = "pizza_lab_balls")]
    pub pizza_lab_limit: usize,
    /// Positive simulation updates to capture (1..=3600).
    #[arg(long, default_value_t = 1200, requires = "pizza_lab_balls")]
    pub pizza_lab_steps: usize,
    /// Request the ordinary directional-gravity event after 120 updates.
    #[arg(long, requires = "pizza_lab_balls")]
    pub pizza_lab_field: bool,
    /// Use ordinary desired=24 spawning instead of the dense fixture.
    #[arg(long, requires = "pizza_lab_balls")]
    pub pizza_lab_ordinary: bool,
    /// New JSON report; serialized once after capture, outside timed updates.
    #[arg(long, requires = "pizza_lab_balls")]
    pub pizza_lab_report: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub balls: usize,
    pub limit: usize,
    pub steps: usize,
    pub field: bool,
    pub ordinary: bool,
    pub report: PathBuf,
}

impl Options {
    pub fn config(
        &self,
        scenario: Option<&str>,
        benchmark: bool,
        isolated: bool,
    ) -> Result<Option<Config>, String> {
        let Some(balls) = self.pizza_lab_balls else {
            return Ok(None);
        };
        if scenario != Some("pizza") || benchmark || !isolated {
            return Err("Pizza lab requires --scenario pizza and an isolated --config-dir, without benchmark mode".into());
        }
        if !(64..=5_000).contains(&balls)
            || !(balls..=5_000).contains(&self.pizza_lab_limit)
            || !(1..=3600).contains(&self.pizza_lab_steps)
        {
            return Err("Pizza lab requires 64..=balls<=limit<=5000 and 1..=3600 steps".into());
        }
        let report = self
            .pizza_lab_report
            .clone()
            .ok_or("Pizza lab requires --pizza-lab-report")?;
        if report.exists() {
            return Err("Pizza lab report already exists; choose a new path".into());
        }
        Ok(Some(Config {
            balls,
            limit: self.pizza_lab_limit,
            steps: self.pizza_lab_steps,
            field: self.pizza_lab_field,
            ordinary: self.pizza_lab_ordinary,
            report,
        }))
    }
}

pub(crate) struct Recorder {
    config: Config,
    samples: Vec<Sample>,
    complete: bool,
}

impl Recorder {
    pub fn new(config: Config) -> Self {
        Self {
            samples: Vec::with_capacity(config.steps),
            config,
            complete: false,
        }
    }
    pub fn before_step(&self, state: &mut PizzaState, dt: Duration) {
        if !dt.is_zero() && self.config.field && state.tick == 120 {
            PizzaScenario::step(
                state,
                &[PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode()],
                Duration::ZERO,
            );
        }
    }
    pub fn after_step(&mut self, state: &PizzaState, dt: Duration) {
        if dt.is_zero() || self.complete {
            return;
        }
        self.samples.push(Sample::capture(state));
        if self.samples.len() == self.config.steps {
            self.complete = true;
            let samples = std::mem::take(&mut self.samples);
            let path = self.config.report.clone();
            let seed = state.seed();
            let (width, height) = (state.config.bounds.width, state.config.bounds.height);
            let ordinary = self.config.ordinary;
            // No serialization/file writes in captured updates. This one worker
            // starts only after the final sample; exclude export from CPU windows.
            std::thread::spawn(move || {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)?;
                    let mut writer = std::io::BufWriter::new(file);
                    serde_json::to_writer(
                        &mut writer,
                        &serde_json::json!({
                            "schema":1,"fixture_version":scenario_pizza::capacity_lab::FIXTURE_VERSION,
                            "seed":seed,"bounds":[width,height],"ordinary":ordinary,
                            "scope":"normal simulation; capture_us is observer overhead; use KMS trace for presentation",
                            "samples":samples,
                        }),
                    )?;
                    writer.flush()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    tracing::error!(%error, ?path, "Pizza lab export failed");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pizza_lab_requires_explicit_scope_and_a_new_bounded_report() {
        use clap::Parser;
        let args = crate::Args::try_parse_from(["engine-client"]).unwrap();
        assert!(args.pizza_lab.config(None, false, false).unwrap().is_none());
        let args = crate::Args::try_parse_from([
            "engine-client",
            "--pizza-lab-balls",
            "500",
            "--pizza-lab-report",
            "/tmp/pizza-lab-scope-test.json",
        ])
        .unwrap();
        assert!(args.pizza_lab.config(None, false, true).is_err());
        assert!(args.pizza_lab.config(Some("pizza"), true, true).is_err());
        assert!(args.pizza_lab.config(Some("pizza"), false, false).is_err());
        let config = args
            .pizza_lab
            .config(Some("pizza"), false, true)
            .unwrap()
            .unwrap();
        assert_eq!((config.limit, config.steps), (500, 1200));
    }

    #[test]
    fn pizza_lab_pause_does_not_consume_capture_or_trigger_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            balls: 64,
            limit: 500,
            steps: 1200,
            field: true,
            ordinary: false,
            report: dir.path().join("trace.json"),
        };
        let mut recorder = Recorder::new(config);
        let mut state = scenario_pizza::capacity_lab::mixed_size_fixture(
            64,
            500,
            scenario_pizza::PizzaBounds::default(),
            7,
        )
        .unwrap();
        state.tick = 120;
        recorder.before_step(&mut state, Duration::ZERO);
        recorder.after_step(&state, Duration::ZERO);
        assert!(recorder.samples.is_empty());
        assert!(matches!(
            state.event_state(),
            scenario_pizza::PizzaEventState::Idle {
                pending_manual: None,
                ..
            }
        ));
        let dt = Duration::from_nanos(16_666_667);
        recorder.before_step(&mut state, dt);
        PizzaScenario::step(&mut state, &[], dt);
        recorder.after_step(&state, dt);
        assert_eq!(recorder.samples.len(), 1);
        assert!(matches!(
            state.event_state(),
            scenario_pizza::PizzaEventState::Active { .. }
        ));
        let without_capture = Sample::capture(&state).fingerprint;
        assert_eq!(without_capture, recorder.samples[0].fingerprint);
    }
}
