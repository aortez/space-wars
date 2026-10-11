//! Non-square physical output with logical content rotated like LinuxKMS.
//! Check the real framebuffer separately from the snapshot API, so a cropped
//! readback cannot accidentally validate the same cropped rendering.
use super::{Error, Options, save};
use crate::presentation_probe::gpu_text::{Context, ProbeWindow};
use i_slint_renderer_femtovg::FemtoVGRendererExt;
use slint::{ComponentHandle, PhysicalSize, RenderingState};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

slint::slint! {
    export component RotationWindow inherits Window {
        background: #102030;
        in property <bool> menu: false;
        Rectangle { x: 8px; y: 8px; width: 32px; height: 32px; background: #ff0000; }
        Rectangle { x: root.width - 40px; y: 8px; width: 32px; height: 32px; background: #00ff00; }
        Rectangle { x: 8px; y: root.height - 40px; width: 32px; height: 32px; background: #0000ff; }
        Rectangle { x: root.width - 40px; y: root.height - 40px; width: 32px; height: 32px; background: #ffff00; }
        Rectangle {
            x: root.width / 2 - 48px; y: root.height / 2 - 32px;
            width: 96px; height: 64px;
            background: root.menu ? #ff00ff : #00ffff;
        }
    }
}

// Expected physical pixel location of a logical pixel. Deliberately independent
// of the renderer's matrix/translation implementation.
fn panel_position(x: u32, y: u32, panel: PhysicalSize, angle: u32) -> (u32, u32) {
    match angle {
        0 => (x, y),
        90 => (panel.width - 1 - y, x),
        180 => (panel.width - 1 - x, panel.height - 1 - y),
        270 => (y, panel.height - 1 - x),
        _ => unreachable!(),
    }
}

fn expected(x: u32, y: u32, size: PhysicalSize, menu: bool) -> [u8; 4] {
    let left = (8..40).contains(&x);
    let right = (size.width - 40..size.width - 8).contains(&x);
    let top = (8..40).contains(&y);
    let bottom = (size.height - 40..size.height - 8).contains(&y);
    match (left, right, top, bottom) {
        (true, _, true, _) => [255, 0, 0, 255],
        (_, true, true, _) => [0, 255, 0, 255],
        (true, _, _, true) => [0, 0, 255, 255],
        (_, true, _, true) => [255, 255, 0, 255],
        _ if (size.width / 2 - 48..size.width / 2 + 48).contains(&x)
            && (size.height / 2 - 32..size.height / 2 + 32).contains(&y) =>
        {
            if menu {
                [255, 0, 255, 255]
            } else {
                [0, 255, 255, 255]
            }
        }
        _ => [16, 32, 48, 255],
    }
}

pub(super) fn run(
    options: &Options,
    context: &Rc<Context>,
    last: &Rc<RefCell<Weak<ProbeWindow>>>,
) -> Result<usize, Error> {
    let panel = context.size;
    let mut failures = 0;
    for angle in [0, 90, 180, 270] {
        let logical = if angle % 180 == 0 {
            panel
        } else {
            PhysicalSize::new(panel.height, panel.width)
        };
        let translation = match angle {
            0 => (0., 0.),
            90 => (0., -(panel.width as f32)),
            180 => (-(panel.width as f32), -(panel.height as f32)),
            270 => (-(panel.height as f32), 0.),
            _ => unreachable!(),
        };
        let ui = RotationWindow::new()?;
        ui.window().set_size(logical);
        ui.show()?;
        let window = last
            .borrow()
            .upgrade()
            .ok_or("missing rotation probe window")?;
        let calls = Rc::new(Cell::new(0));
        for (index, (label, menu)) in [
            ("no-notifier-scene", false),
            ("notifier-scene", false),
            ("notifier-menu", true),
            ("notifier-scene-again", false),
            ("notifier-menu-again", true),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 1 {
                let calls = calls.clone();
                // Even an idle/no-op callback used to break 90/270-degree
                // rendering. Keep it installed through scene/menu transitions.
                ui.window()
                    .set_rendering_notifier(move |state, _| {
                        if matches!(state, RenderingState::BeforeRendering) {
                            calls.set(calls.get() + 1);
                        }
                    })
                    .map_err(|e| format!("rotation notifier: {e:?}"))?;
            }
            ui.set_menu(menu);
            ui.window().request_redraw();
            window.renderer.render_transformed_with_post_callback(
                angle as f32,
                translation,
                panel,
                None,
            )?;
            let pixels = context.pixels()?;
            let snapshot = ui.window().take_snapshot()?;
            context.check()?;
            let dimensions_ok =
                snapshot.width() == logical.width && snapshot.height() == logical.height;
            let mut panel_mismatches = 0;
            let mut snapshot_mismatches = 0;
            // Sample a dense grid away from antialiased rectangle boundaries,
            // including each corner and the asymmetric center scene/menu marker.
            for y in (4..logical.height).step_by(8) {
                for x in (4..logical.width).step_by(8) {
                    let color = expected(x, y, logical, menu);
                    let (px, py) = panel_position(x, y, panel, angle);
                    let offset = ((py * panel.width + px) * 4) as usize;
                    panel_mismatches += usize::from(pixels[offset..offset + 4] != color);
                    if dimensions_ok {
                        let offset = ((y * logical.width + x) * 4) as usize;
                        snapshot_mismatches +=
                            usize::from(snapshot.as_bytes()[offset..offset + 4] != color);
                    }
                }
            }
            let name = format!("rotation-{angle}-{label}");
            save(options, &name, &pixels)?;
            println!(
                "{name} panel_mismatches={panel_mismatches} snapshot={}x{} expected={}x{} snapshot_mismatches={snapshot_mismatches} notifier_calls={}",
                snapshot.width(),
                snapshot.height(),
                logical.width,
                logical.height,
                calls.get()
            );
            failures += usize::from(
                panel_mismatches != 0
                    || !dimensions_ok
                    || snapshot_mismatches != 0
                    || calls.get() != index,
            );
        }
    }
    Ok(failures)
}
