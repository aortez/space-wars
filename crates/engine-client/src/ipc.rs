#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

use engine_common::DEFAULT_CONTROL_SOCKET;
use slint::Timer;
#[cfg(unix)]
use slint::{ComponentHandle, Rgba8Pixel, SharedPixelBuffer, TimerMode};

use crate::MainWindow;

#[cfg(unix)]
#[derive(Debug)]
struct ControlRequest {
    command: ControlCommand,
    response: ResponseWriter,
}

#[cfg(unix)]
#[derive(Debug)]
enum ControlCommand {
    Screenshot { output: PathBuf },
    Status,
}

#[cfg(unix)]
#[derive(Debug)]
struct ResponseWriter {
    stream: std::os::unix::net::UnixStream,
}

#[cfg(unix)]
impl ResponseWriter {
    fn ok(mut self, message: impl AsRef<str>) {
        let _ = writeln!(self.stream, "ok {}", message.as_ref());
    }

    fn error(mut self, message: impl AsRef<str>) {
        let _ = writeln!(self.stream, "error {}", message.as_ref());
    }
}

pub fn control_socket_path() -> PathBuf {
    std::env::var_os("SPACEWARS_CONTROL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_SOCKET))
}

#[cfg(unix)]
pub fn start_control_server(window: &MainWindow, socket_path: PathBuf) -> Option<Timer> {
    let (tx, rx) = mpsc::channel();
    if let Err(err) = spawn_listener(socket_path.clone(), tx) {
        tracing::warn!(
            path = %socket_path.display(),
            error = %err,
            "failed to start control socket."
        );
        return None;
    }

    let timer = Timer::default();
    let weak_window = window.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        while let Ok(request) = rx.try_recv() {
            handle_request(&window, request);
        }
    });

    Some(timer)
}

#[cfg(not(unix))]
pub fn start_control_server(_window: &MainWindow, _socket_path: PathBuf) -> Option<Timer> {
    tracing::info!("control socket is unavailable on this platform.");
    None
}

#[cfg(unix)]
fn spawn_listener(
    socket_path: PathBuf,
    tx: mpsc::Sender<ControlRequest>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    tracing::info!(path = %socket_path.display(), "control socket listening.");

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_stream(stream, &tx),
                Err(err) => tracing::warn!(error = %err, "control socket accept failed."),
            }
        }
    });

    Ok(())
}

#[cfg(unix)]
fn handle_stream(mut stream: std::os::unix::net::UnixStream, tx: &mpsc::Sender<ControlRequest>) {
    let mut body = String::new();
    if let Err(err) = stream.read_to_string(&mut body) {
        let _ = writeln!(stream, "error failed to read command: {err}");
        return;
    }

    let response = ResponseWriter { stream };
    let command = match parse_command(&body) {
        Ok(command) => command,
        Err(message) => {
            response.error(message);
            return;
        }
    };

    if let Err(err) = tx.send(ControlRequest { command, response }) {
        err.0
            .response
            .error("control socket request dropped because UI loop is unavailable");
        tracing::warn!("control socket request dropped because UI loop is unavailable.");
    }
}

#[cfg(unix)]
fn parse_command(body: &str) -> Result<ControlCommand, String> {
    let mut lines = body.lines();
    match lines.next() {
        Some("screenshot") => {
            let Some(output) = lines.next() else {
                return Err("missing screenshot output path".into());
            };
            if output.is_empty() {
                return Err("screenshot output path must not be empty".into());
            }
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::Screenshot {
                output: PathBuf::from(output),
            })
        }
        Some("status") => {
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::Status)
        }
        Some(command) => Err(format!("unknown command {command:?}")),
        None => Err("empty command".into()),
    }
}

#[cfg(unix)]
fn handle_request(window: &MainWindow, request: ControlRequest) {
    match request.command {
        ControlCommand::Screenshot { output } => match write_window_screenshot(window, &output) {
            Ok(()) => request
                .response
                .ok(format!("screenshot saved to {}", output.display())),
            Err(err) => request.response.error(err.to_string()),
        },
        ControlCommand::Status => request.response.ok(window.get_runtime_diagnostics()),
    }
}

#[cfg(unix)]
fn write_window_screenshot(
    window: &MainWindow,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let snapshot = window.window().take_snapshot()?;
    write_rgba_png(output, &snapshot)?;
    Ok(())
}

#[cfg(unix)]
fn write_rgba_png(
    output: &Path,
    pixels: &SharedPixelBuffer<Rgba8Pixel>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::create(output)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, pixels.width(), pixels.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels.as_bytes())?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parse_screenshot_command() {
        let command = parse_command("screenshot\n/tmp/shot.png\n").unwrap();
        match command {
            ControlCommand::Screenshot { output } => {
                assert_eq!(output, PathBuf::from("/tmp/shot.png"));
            }
            ControlCommand::Status => panic!("expected screenshot command"),
        }
    }

    #[test]
    fn reject_extra_lines() {
        assert!(parse_command("screenshot\n/tmp/shot.png\nextra\n").is_err());
    }

    #[test]
    fn parse_status_command() {
        assert!(matches!(
            parse_command("status\n"),
            Ok(ControlCommand::Status)
        ));
        assert!(parse_command("status\nextra\n").is_err());
    }
}
