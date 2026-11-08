//! A census of natural debris: the live asteroid population, the spawned
//! radii and the step time of the ordinary scenario at the client's defaults,
//! then the material stream's radii at each strength every three seconds.
//! Usage: asteroid_census [SECONDS] [ARRIVALS_PER_SECOND].
use engine_common::{MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario};
use engine_core::config::SpacewarsConfig;
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use scenario_spacewars::{DebrisKind, SpacewarsScenario};
use std::time::{Duration, Instant};

fn histogram(radii: &[f32]) -> String {
    let edges = [
        0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 12.0, 20.0, 40.0, 80.0,
    ];
    let mut counts = vec![0usize; edges.len() + 1];
    for &r in radii {
        let bucket = edges.iter().position(|&e| r < e).unwrap_or(edges.len());
        counts[bucket] += 1;
    }
    let mut out = String::new();
    let mut low = 0.0;
    for (i, &count) in counts.iter().enumerate() {
        let high = edges.get(i).copied();
        let label = match high {
            Some(h) => format!("{low:>4.1}-{h:<4.1}"),
            None => format!("{low:>4.1}+    "),
        };
        out += &format!(
            "  {label} {count:6} {:5.1}%\n",
            100.0 * count as f32 / radii.len().max(1) as f32
        );
        low = high.unwrap_or(low);
    }
    out
}

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let probability: f32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let dt = Duration::from_nanos(16_666_667);
    let mut config = SpacewarsConfig {
        universe_radius: 1200,
        asteroid_probability_per_sec: probability,
        ..SpacewarsConfig::default()
    };
    for player in &mut config.players {
        player.health_percent = 500;
    }
    let mut seed = 7;
    let mut state = SpacewarsScenario::init(config.clone(), seed);
    let mut radii = Vec::new();
    let mut live = Vec::new();
    let mut games = 1;
    let started = Instant::now();
    for tick in 0..seconds * 60 {
        if state.winner.is_some() {
            // A dead seat freezes the world; carry on in a fresh game.
            seed += 1;
            games += 1;
            state = SpacewarsScenario::init(config.clone(), seed);
        }
        SpacewarsScenario::step(&mut state, &[], dt);
        for d in &state.debris {
            if d.kind == DebrisKind::Asteroid && d.spawn_tick + 1 == state.tick {
                radii.push(d.radius);
            }
        }
        if tick % 600 == 599 {
            live.push(
                state
                    .debris
                    .iter()
                    .filter(|d| d.kind == DebrisKind::Asteroid && !d.dead)
                    .count(),
            );
        }
    }
    let elapsed = started.elapsed();
    println!("  {games} games (a fresh game after each win)");
    let mut sorted = radii.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let q = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
    println!(
        "ordinary spacewars, radius 1200, {probability}/s, {seconds} s: {} spawned, {:.1} per second, step time {:.1} us",
        radii.len(),
        radii.len() as f32 / seconds as f32,
        elapsed.as_secs_f64() * 1e6 / (seconds * 60) as f64
    );
    println!("  live asteroids every 10 s: {:?}", live);
    println!(
        "  radius median {:.2}, q90 {:.2}, q99 {:.2}, max {:.2}; over 4: {:.1}%, over 6: {:.1}%",
        q(0.5),
        q(0.9),
        q(0.99),
        sorted[sorted.len() - 1],
        100.0 * sorted.iter().filter(|&&r| r > 4.0).count() as f32 / sorted.len() as f32,
        100.0 * sorted.iter().filter(|&&r| r > 6.0).count() as f32 / sorted.len() as f32
    );
    print!("{}", histogram(&radii));
    for (severity, name) in [
        (MaterialAsteroidSeverity::Light, "light"),
        (MaterialAsteroidSeverity::Mixed, "mixed"),
        (MaterialAsteroidSeverity::Heavy, "heavy"),
    ] {
        let mut state = SurfaceSortieScenario::init_material_match(7);
        state.set_asteroid_pressure(MaterialAsteroidSettings {
            interval_seconds: 3,
            severity,
        });
        let mut radii = Vec::new();
        let mut live = Vec::new();
        let started = Instant::now();
        for tick in 0..seconds * 60 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
            radii.extend(state.asteroid_pressure().arrivals.iter().map(|a| a.radius));
            if tick % 600 == 599 {
                live.push(state.asteroid_pressure().live);
            }
        }
        let elapsed = started.elapsed();
        let mut sorted = radii.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        println!(
            "material match, every 3 s, {name}: {} arrivals, radius min {:.2} median {:.2} max {:.2}, step time {:.1} us",
            radii.len(),
            sorted[0],
            sorted[sorted.len() / 2],
            sorted[sorted.len() - 1],
            elapsed.as_secs_f64() * 1e6 / (seconds * 60) as f64
        );
        println!("  live every 10 s: {:?}", live);
        print!("{}", histogram(&radii));
    }
}
