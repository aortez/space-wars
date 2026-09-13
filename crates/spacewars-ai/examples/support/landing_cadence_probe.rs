//! Optional paired sensor measurements along the unchanged 60 Hz policy run.
use scenario_spacewars::surface_sortie::{
    SurfaceSortieState,
    mission::{
        LandingSurveyCadence, LandingSurveyStamp, MissionObservationV1, MissionSensorRequest,
    },
    pilot::LandingSiteQuery,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct LandingCadenceProbe {
    output: BufWriter<File>,
    last: [Option<LandingSurveyStamp>; 2],
}

impl LandingCadenceProbe {
    pub fn new(path: &Path) -> Self {
        let mut output = BufWriter::new(File::create(path).unwrap());
        writeln!(
            output,
            "tick,seat,reference_first,reference_ms,scheduled_ms,scheduled_query"
        )
        .unwrap();
        Self {
            output,
            last: [None; 2],
        }
    }

    pub fn observe(
        &mut self,
        state: &SurfaceSortieState,
        seat: usize,
        mut request: MissionSensorRequest,
    ) -> (MissionObservationV1, f64) {
        request.last_survey = self.last[seat];
        let clock = Instant::now();
        // Match the reference's profiling wrapper so both clocks include the
        // same instrumentation; nested scopes are otherwise disabled.
        #[cfg(feature = "sensor-profile")]
        let (o, _profile) = scenario_spacewars::surface_sortie::sensor_profile::measure(|| {
            state.mission_observation_with_cadence(seat, request, LandingSurveyCadence::FourHz)
        });
        #[cfg(not(feature = "sensor-profile"))]
        let o = state.mission_observation_with_cadence(seat, request, LandingSurveyCadence::FourHz);
        let ms = clock.elapsed().as_secs_f64() * 1000.0;
        let p = &o.local.combat.recovery.flight.pilot;
        if p.queries_ready && p.site_query == LandingSiteQuery::Survey {
            self.last[seat] = Some(LandingSurveyStamp {
                tick: p.tick,
                planet: p.planet.index,
                form: p.ship_form,
            });
        }
        (o, ms)
    }

    pub fn record(
        &mut self,
        seat: usize,
        reference: &MissionObservationV1,
        reference_ms: f64,
        scheduled: (MissionObservationV1, f64),
        reference_first: bool,
    ) {
        let (o, scheduled_ms) = scheduled;
        let p = &o.local.combat.recovery.flight.pilot;
        let mut expected = reference.clone();
        if p.site_query.is_deferred() {
            assert_eq!(
                expected.local.combat.recovery.flight.pilot.site_query,
                LandingSiteQuery::Survey
            );
            expected.local.combat.recovery.flight.pilot.site_query = p.site_query;
            expected.local.combat.recovery.flight.pilot.sites.clear();
            expected.local.combat.recovery.sites.clear();
            expected.local.cover.clear();
        }
        assert_eq!(
            o, expected,
            "cadence changed live observations at tick {}, seat {seat}",
            p.tick
        );
        let query = match p.site_query {
            LandingSiteQuery::Survey => "survey",
            LandingSiteQuery::Selected(_) => "selected",
            LandingSiteQuery::Deferred { .. } => "deferred",
            LandingSiteQuery::NotRequested => "not_requested",
        };
        writeln!(
            self.output,
            "{},{seat},{reference_first},{reference_ms},{scheduled_ms},{query}",
            p.tick
        )
        .unwrap();
    }
}
