use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use engine_common::DEFAULT_CONTROL_SOCKET;

#[derive(Debug, Parser)]
#[command(name = "spacewars-cli", about = "Space-Wars runtime control helper")]
struct Args {
    /// Engine-client Unix control socket.
    #[arg(long)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print diagnostics from the running UI.
    Status,

    /// Ask the running UI to write a PNG screenshot.
    Screenshot {
        /// Output path on the machine running engine-client.
        output: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let socket = args
        .socket
        .or_else(|| std::env::var_os("SPACEWARS_CONTROL_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_SOCKET));

    match args.command {
        Command::Status => request_status(&socket)?,
        Command::Screenshot { output } => request_screenshot(&socket, output)?,
    }

    Ok(())
}

#[cfg(unix)]
fn request_screenshot(socket: &Path, output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let output = output
        .to_str()
        .ok_or("screenshot output path must be valid UTF-8")?;
    if output.contains('\n') {
        return Err("screenshot output path must not contain newlines".into());
    }

    let message = send_request(socket, &format!("screenshot\n{output}\n"))?;
    println!("{message}");
    Ok(())
}

#[cfg(unix)]
fn request_status(socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let message = send_request(socket, "status\n")?;
    println!("{message}");
    Ok(())
}

#[cfg(unix)]
fn send_request(socket: &Path, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket)?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    match parse_response(&response) {
        Ok(message) => Ok(message.to_string()),
        Err(message) => Err(message.into()),
    }
}

fn parse_response(response: &str) -> Result<&str, String> {
    let response = response.trim_end();
    if let Some(message) = response.strip_prefix("ok ") {
        Ok(message)
    } else if let Some(message) = response.strip_prefix("error ") {
        Err(message.to_string())
    } else {
        Err(format!(
            "unexpected response from engine-client: {response:?}"
        ))
    }
}

#[cfg(not(unix))]
fn request_screenshot(_socket: &Path, _output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli screenshot requires Unix domain sockets".into())
}

#[cfg(not(unix))]
fn request_status(_socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli status requires Unix domain sockets".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_subcommand() {
        let args = Args::try_parse_from(["spacewars-cli", "status"]).unwrap();

        assert!(matches!(args.command, Command::Status));
    }

    #[test]
    fn accepts_multiline_success_response() {
        assert_eq!(
            parse_response("ok scenario=pizza\nfps=59.8\nframes_total=123\n").unwrap(),
            "scenario=pizza\nfps=59.8\nframes_total=123"
        );
    }

    #[test]
    fn reports_engine_and_protocol_errors() {
        assert_eq!(
            parse_response("error screenshot failed\n").unwrap_err(),
            "screenshot failed"
        );
        assert_eq!(
            parse_response("").unwrap_err(),
            "unexpected response from engine-client: \"\""
        );
    }
}
