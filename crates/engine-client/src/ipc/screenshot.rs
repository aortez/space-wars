//! Read KMS/FemtoVG output after drawing, before swap. Never poll buffer history.
use super::*;

pub(super) fn request(window: &MainWindow, output: PathBuf, response: ResponseWriter) {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    if i_slint_backend_linuxkms::frame_capture::available() {
        use i_slint_backend_linuxkms::frame_capture;
        use std::{cell::RefCell, rc::Rc};
        let weak = window.as_weak();
        // The rejected-request path must still own its response writer.
        let reply = Rc::new(RefCell::new(Some(response)));
        let completion = reply.clone();
        let timeout = Duration::from_secs(5);
        let queued = frame_capture::request(timeout, move |ready| {
            let response = completion
                .borrow_mut()
                .take()
                .expect("screenshot completes once");
            let pixels = ready.map_err(str::to_owned).and_then(|()| {
                let window = weak
                    .upgrade()
                    .ok_or("window closed before screenshot capture")?;
                window
                    .window()
                    .take_snapshot()
                    .map_err(|error| error.to_string())
            });
            // Only read pixels in the rendering callback. Encode/write and
            // reply on the event loop after presentation, with no GL borrows.
            Timer::single_shot(Duration::ZERO, move || {
                let result = pixels
                    .and_then(|pixels| save(&output, &pixels).map_err(|error| error.to_string()));
                finish(response, &output, result);
            });
        });
        match queued {
            Ok(ticket) => {
                window.window().request_redraw();
                Timer::single_shot(timeout, move || ticket.cancel());
            }
            Err(error) => reply.borrow_mut().take().unwrap().error(error),
        }
        return;
    }
    // Software snapshots repaint into their own buffer. Other desktop backends
    // retain the existing redraw/wait behavior; the pre-swap hook is KMS-only.
    window.window().request_redraw();
    let weak = window.as_weak();
    Timer::single_shot(Duration::from_millis(50), move || {
        let result = weak
            .upgrade()
            .ok_or_else(|| "window closed before screenshot capture".to_owned())
            .and_then(|window| {
                window
                    .window()
                    .take_snapshot()
                    .map_err(|error| error.to_string())
            })
            .and_then(|pixels| save(&output, &pixels).map_err(|error| error.to_string()));
        finish(response, &output, result);
    });
}

fn save(
    output: &Path,
    pixels: &SharedPixelBuffer<Rgba8Pixel>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    write_rgba_png(output, pixels)
}

fn finish(response: ResponseWriter, output: &Path, result: Result<(), String>) {
    match result {
        Ok(()) => response.ok(format!("screenshot saved to {}", output.display())),
        Err(error) => response.error(error),
    }
}
