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
        Command::Screenshot { output } => request_screenshot(&socket, output)?,
    }

    Ok(())
}

#[cfg(unix)]
fn request_screenshot(socket: &Path, output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixStream;

    let output = output
        .to_str()
        .ok_or("screenshot output path must be valid UTF-8")?;
    if output.contains('\n') {
        return Err("screenshot output path must not contain newlines".into());
    }

    let mut stream = UnixStream::connect(socket)?;
    writeln!(stream, "screenshot")?;
    writeln!(stream, "{output}")?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let response = response.trim_end();
    if let Some(message) = response.strip_prefix("ok ") {
        println!("{message}");
        Ok(())
    } else if let Some(message) = response.strip_prefix("error ") {
        Err(message.to_string().into())
    } else {
        Err(format!("unexpected response from engine-client: {response:?}").into())
    }
}

#[cfg(not(unix))]
fn request_screenshot(_socket: &Path, _output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli screenshot requires Unix domain sockets".into())
}
