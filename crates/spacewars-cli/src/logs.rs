//! Kiosk journal access is separate from the application's control socket.

use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct LogOptions {
    /// Number of recent entries from the current boot (1–1000).
    #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u16).range(1..=1000))]
    lines: u16,

    /// Stream new entries after the recent history; Ctrl-C stops following.
    #[arg(long)]
    follow: bool,
}

const HELPER: &str = "/usr/sbin/spacewars-logs";

#[cfg(any(target_os = "linux", test))]
fn command(options: &LogOptions) -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/sudo");
    command.args(["-n", HELPER, "--lines", &options.lines.to_string()]);
    if options.follow {
        command.arg("--follow");
    }
    command.stdin(std::process::Stdio::null());
    command
}

pub(crate) fn run(options: &LogOptions) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;

        if !std::path::Path::new(HELPER).is_file() {
            return Err("Kiosk log access is not installed. Install a Pi rootfs update containing spacewars-logs; a fast application update cannot install it.".into());
        }
        // Inherit output, exit status and signals without buffering or polling.
        // The root-owned helper fixes the unit and independently validates args.
        Err(format!(
            "Could not start kiosk log reader: {}",
            command(options).exec()
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (options, HELPER);
        Err("Kiosk logs are available on the Pi via SSH.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Result<LogOptions, clap::Error> {
        let args = crate::Args::try_parse_from(
            ["spacewars-cli", "logs"]
                .into_iter()
                .chain(args.iter().copied()),
        )?;
        let crate::Command::Logs(options) = args.command else {
            unreachable!()
        };
        Ok(options)
    }

    #[test]
    fn defaults_to_bounded_history_without_following() {
        let options = parse(&[]).unwrap();
        assert_eq!(options.lines, 200);
        assert!(!options.follow);
    }

    #[test]
    fn only_exposes_bounded_lines_and_follow() {
        for value in ["1", "200", "1000"] {
            assert!(parse(&["--lines", value]).is_ok());
        }
        for args in [
            vec!["--lines", "0"],
            vec!["--lines", "1001"],
            vec!["--lines", "-1"],
            vec!["--lines", "all"],
            vec!["--unit", "ssh.service"],
            vec!["--file", "/tmp/private"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn uses_fixed_noninteractive_helper_with_separate_arguments() {
        let options = parse(&["--lines", "25", "--follow"]).unwrap();
        let command = command(&options);
        assert_eq!(command.get_program(), "/usr/bin/sudo");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-n", HELPER, "--lines", "25", "--follow"]
        );
    }
}
