use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, RenderingRotation, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};

thread_local! {
    static TEST_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct TestPlatform;

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        TEST_WINDOW.with(|window| {
            window
                .borrow()
                .as_ref()
                .cloned()
                .map(|window| window as Rc<dyn WindowAdapter>)
                .ok_or_else(|| "test window is not initialized".into())
        })
    }
}

slint::slint! {
    export component SnapshotWindow inherits Window {
        background: #123456;
    }
}

#[test]
fn snapshot_ignores_software_output_rotation() {
    let software_window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    TEST_WINDOW.with(|window| *window.borrow_mut() = Some(Rc::clone(&software_window)));
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();

    let ui = SnapshotWindow::new().unwrap();
    software_window.set_size(PhysicalSize::new(120, 80));
    ui.show().unwrap();

    assert!(software_window.draw_if_needed(|renderer| {
        renderer.set_rendering_rotation(RenderingRotation::Rotate90);
    }));

    let snapshot = ui.window().take_snapshot().unwrap();
    assert_eq!((snapshot.width(), snapshot.height()), (120, 80));
    let center = snapshot.as_slice()[40 * 120 + 60];
    assert_eq!((center.r, center.g, center.b), (0x12, 0x34, 0x56));

    ui.window().request_redraw();
    assert!(software_window.draw_if_needed(|renderer| {
        assert_eq!(renderer.rendering_rotation(), RenderingRotation::Rotate90);
    }));
}
