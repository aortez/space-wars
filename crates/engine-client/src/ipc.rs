use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use engine_common::DEFAULT_CONTROL_SOCKET;
use slint::{ComponentHandle, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode};

use crate::MainWindow;

#[derive(Debug)]
struct ControlRequest {
    command: ControlCommand,
    response: ResponseWriter,
}

#[derive(Debug)]
enum ControlCommand {
    Screenshot { output: PathBuf },
}

#[derive(Debug)]
struct ResponseWriter {
    stream: std::os::unix::net::UnixStream,
}

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
        Some(command) => Err(format!("unknown command {command:?}")),
        None => Err("empty command".into()),
    }
}

fn handle_request(window: &MainWindow, request: ControlRequest) {
    match request.command {
        ControlCommand::Screenshot { output } => match write_window_screenshot(window, &output) {
            Ok(()) => request
                .response
                .ok(format!("screenshot saved to {}", output.display())),
            Err(err) => request.response.error(err.to_string()),
        },
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_screenshot_command() {
        let command = parse_command("screenshot\n/tmp/shot.png\n").unwrap();
        match command {
            ControlCommand::Screenshot { output } => {
                assert_eq!(output, PathBuf::from("/tmp/shot.png"));
            }
        }
    }

    #[test]
    fn reject_extra_lines() {
        assert!(parse_command("screenshot\n/tmp/shot.png\nextra\n").is_err());
    }
}
