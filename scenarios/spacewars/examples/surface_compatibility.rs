//! Run with --help for the deterministic generated-surface probe matrix.
use scenario_spacewars::surface_sortie::GeneratedSurfaceProfile;
use scenario_spacewars::surface_sortie::compatibility::{GeneratedSurfaceCase, ProbeOutcome};

#[derive(Debug, PartialEq)]
enum Profiles {
    Raw,
    SurfaceV1,
    Both,
}

impl Profiles {
    fn values(&self) -> &'static [GeneratedSurfaceProfile] {
        match self {
            Self::Raw => &[GeneratedSurfaceProfile::Raw],
            Self::SurfaceV1 => &[GeneratedSurfaceProfile::SurfaceV1],
            Self::Both => &[
                GeneratedSurfaceProfile::Raw,
                GeneratedSurfaceProfile::SurfaceV1,
            ],
        }
    }
}

#[derive(Debug, PartialEq)]
struct Options {
    seed: u64,
    seeds: u64,
    planet: Option<usize>,
    bearing: Option<u8>,
    json: bool,
    profiles: Profiles,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        seed: 0,
        seeds: 1,
        planet: None,
        bearing: None,
        json: false,
        profiles: Profiles::Raw,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag == "--json" {
            options.json = true;
            continue;
        }
        if !matches!(
            flag.as_str(),
            "--seed" | "--seeds" | "--planet" | "--bearing" | "--profile"
        ) {
            return Err(format!("unknown option {flag:?}; use --help"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        if flag == "--profile" {
            options.profiles = match value.as_str() {
                "raw" => Profiles::Raw,
                "surface-v1" => Profiles::SurfaceV1,
                "both" => Profiles::Both,
                _ => return Err("--profile must be raw, surface-v1, or both".into()),
            };
            continue;
        }
        let number = value
            .parse::<u64>()
            .map_err(|_| format!("invalid value for {flag}: {value:?}"))?;
        match flag.as_str() {
            "--seed" => options.seed = number,
            "--seeds" => options.seeds = number,
            "--planet" => {
                options.planet =
                    Some(usize::try_from(number).map_err(|_| "planet index too large")?)
            }
            "--bearing" => {
                options.bearing = Some(u8::try_from(number).map_err(|_| "bearing must be 0..=3")?)
            }
            _ => unreachable!(),
        }
    }
    if !(1..=32).contains(&options.seeds) {
        return Err("--seeds must be 1..=32".into());
    }
    if options.bearing.is_some_and(|bearing| bearing > 3) {
        return Err("--bearing must be 0..=3".into());
    }
    options
        .seed
        .checked_add(options.seeds - 1)
        .ok_or("seed range overflows")?;
    Ok(options)
}

fn run(options: Options) -> Result<(), String> {
    let mut cases = Vec::new();
    for offset in 0..options.seeds {
        let seed = options.seed + offset;
        let count = GeneratedSurfaceCase::planet_count(seed);
        if options.planet.is_some_and(|planet| planet >= count) {
            return Err(format!(
                "seed {seed} has {count} planets; --planet is zero-based"
            ));
        }
        for planet in 0..count {
            if options.planet.is_some_and(|selected| selected != planet) {
                continue;
            }
            for bearing in 0..4 {
                if options.bearing.is_some_and(|selected| selected != bearing) {
                    continue;
                }
                for &profile in options.profiles.values() {
                    cases.push(
                        GeneratedSurfaceCase::new(seed, planet, bearing).with_profile(profile),
                    );
                }
            }
        }
    }
    let mut totals = options
        .profiles
        .values()
        .iter()
        .map(|&profile| (profile, 0_usize, [0_usize; 3]))
        .collect::<Vec<_>>();
    if !options.json {
        println!("Generated surfaces / unchanged lab controls / 60 Hz / no asteroids");
        println!("All generated gravity sources retained; P=pass F=fail B=blocked prerequisite");
    }
    for case in &cases {
        let report = case.run()?;
        let (_, count, outcomes) = totals
            .iter_mut()
            .find(|(profile, _, _)| *profile == case.profile)
            .unwrap();
        *count += 1;
        for probe in &report.probes {
            outcomes[match probe.outcome {
                ProbeOutcome::Passed => 0,
                ProbeOutcome::Failed => 1,
                ProbeOutcome::Blocked => 2,
            }] += 1;
        }
        if options.json {
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|error| error.to_string())?
            );
        } else {
            let e = &report.environment;
            let probes = report
                .probes
                .iter()
                .map(|probe| {
                    let outcome = match probe.outcome {
                        ProbeOutcome::Passed => "P",
                        ProbeOutcome::Failed => "F",
                        ProbeOutcome::Blocked => "B",
                    };
                    format!("{:?}={outcome}", probe.kind)
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "profile={} seed={} planet={} bearing={} r={:.1} g={:.1} spin={:+.3} frame_mismatch={:.1} inward={:.1} {}",
                case.profile.id(),
                case.seed,
                case.planet,
                case.bearing,
                e.radius,
                e.own_surface_gravity,
                e.spin,
                e.frame_acceleration_mismatch,
                e.effective_inward_acceleration,
                probes
            );
        }
    }
    // JSON mode is JSONL only on stdout; summaries stay on stderr.
    for (profile, count, outcomes) in totals {
        eprintln!(
            "profile={} cases={} probes={} passed={} failed={} blocked={} (diagnostic outcomes, not a CI exit gate)",
            profile.id(),
            count,
            outcomes.iter().sum::<usize>(),
            outcomes[0],
            outcomes[1],
            outcomes[2]
        );
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "surface_compatibility [--seed N] [--seeds 1..32] [--planet INDEX] [--bearing 0..3] [--profile raw|surface-v1|both] [--json]\n\
            Defaults: raw profile, seed 0, one world, all planets, four sun-relative bearings.\n\
            --profile both compares raw and surface-v1 on identical case identifiers and controls.\n\
            Bearings: 0 away, 1 counterclockwise, 2 toward sun, 3 clockwise.\n\
            --json emits one complete report per line. Each probe starts independently.\n\
            Failed/blocked physical probes are data, not command errors. Invalid arguments exit 2."
        );
        return std::process::ExitCode::SUCCESS;
    }
    match parse(args).and_then(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_rejects_invalid_or_unbounded_matrix_options() {
        for args in [
            vec!["--seeds", "0"],
            vec!["--seeds", "33"],
            vec!["--bearing", "4"],
            vec!["--seed"],
            vec!["--planet", "-1"],
            vec!["--wat"],
            vec!["--profile", "unknown"],
            vec!["--profile"],
            vec!["--seed", "18446744073709551615", "--seeds", "2"],
        ] {
            assert!(parse(args.into_iter().map(str::to_owned)).is_err());
        }
        assert_eq!(
            parse(["--seed", "4", "--planet", "2", "--bearing", "3", "--json"].map(str::to_owned))
                .unwrap(),
            Options {
                seed: 4,
                seeds: 1,
                planet: Some(2),
                bearing: Some(3),
                json: true,
                profiles: Profiles::Raw,
            }
        );
    }

    #[test]
    fn named_profiles_preserve_the_case_selection_and_pair_in_a_stable_order() {
        for (name, expected) in [
            ("raw", Profiles::Raw),
            ("surface-v1", Profiles::SurfaceV1),
            ("both", Profiles::Both),
        ] {
            let options = parse(
                [
                    "--profile",
                    name,
                    "--seed",
                    "13",
                    "--planet",
                    "4",
                    "--bearing",
                    "0",
                ]
                .map(str::to_owned),
            )
            .unwrap();
            assert_eq!(options.profiles, expected);
            assert_eq!(
                (options.seed, options.seeds, options.planet, options.bearing),
                (13, 1, Some(4), Some(0))
            );
        }
        assert_eq!(
            Profiles::Both.values(),
            &[
                GeneratedSurfaceProfile::Raw,
                GeneratedSurfaceProfile::SurfaceV1
            ]
        );
    }
}
