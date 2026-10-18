use super::*;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use spacewars_control::UiAction;

// Each exported image owns a fresh buffer, so paint the complete menu tree.
struct CapturePlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

#[test]
fn pizza_menu_keyboard_touch_and_disabled_action_work_at_target_sizes() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    crate::install_ui_navigation(&ui);
    crate::install_keyboard_navigation(&ui, crate::input::new_shared_input().0);
    let triggers = Rc::new(RefCell::new(0));
    let count = triggers.clone();
    ui.on_ingame_pizza_gravity(move || *count.borrow_mut() += 1);
    let pointers = Rc::new(RefCell::new(Vec::new()));
    let events = pointers.clone();
    ui.on_scenario_pointer(move |_, _, phase| events.borrow_mut().push(phase));
    ui.show().unwrap();
    let output =
        std::env::var_os("SPACEWARS_PIZZA_CONTROLS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows.borrow()[0].set_size(PhysicalSize::new(width, height));
        reset_panels(&ui);
        ui.set_launcher_visible(false);
        ui.set_launcher_scenario("pizza".into());
        ui.set_scenario_benchmark_available(true);
        ui.set_scenario_pointer_enabled(true);
        ui.set_pizza_event_available(true);
        ui.set_pizza_event_can_trigger(true);
        ui.set_pizza_event_status(
            "Starts in this world · Grabbing and flinging stay available".into(),
        );
        ui.set_ingame_menu_visible(true);
        slint::platform::update_timers_and_animations();
        let mut initial = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
        windows.borrow()[0].draw_if_needed(|renderer| {
            renderer.render(initial.make_mut_slice(), width as usize);
        });
        for key in [Key::UpArrow, Key::Return] {
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        }
        assert_eq!(ui.get_ingame_menu_focus_index(), 6);
        assert_eq!(*triggers.borrow(), 1);
        let position =
            slint::LogicalPosition::new(width as f32 / 2.0, (height as f32 - 390.0) / 2.0 + 298.0);
        for (active, expected) in [(false, 2), (true, 2)] {
            ui.set_pizza_event_can_trigger(!active);
            ui.set_pizza_event_status(
                if active {
                    "Gravity active · Resume to continue"
                } else {
                    "Starts in this world · Grabbing and flinging stay available"
                }
                .into(),
            );
            slint::platform::update_timers_and_animations();
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            let windows = windows.borrow();
            windows[0].request_redraw();
            windows[0].draw_if_needed(|renderer| {
                renderer.render(pixels.make_mut_slice(), width as usize);
            });
            if let Some(path) = &output {
                crate::thruster_visual_tests::write_png(
                    &path.join(format!(
                        "{width}x{height}-{}.png",
                        if active { "busy" } else { "ready" }
                    )),
                    &pixels,
                );
            }
            for event in [
                WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                },
                WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                },
            ] {
                ui.window().dispatch_event(event);
            }
            assert_eq!(*triggers.borrow(), expected, "touch at {width}x{height}");
            if active {
                ui.invoke_ui_action(UiAction::Confirm.code());
                assert_eq!(*triggers.borrow(), expected, "disabled keyboard activation");
            }
            assert!(
                pointers.borrow().iter().all(|phase| *phase == 3),
                "menu pointer leaked"
            );
        }
        // After Resume, the usual pointer press/drag/release still reaches Pizza.
        ui.set_ingame_menu_visible(false);
        pointers.borrow_mut().clear();
        for event in [
            WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(position.x + 30.0, position.y - 10.0),
            },
            WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            },
        ] {
            ui.window().dispatch_event(event);
        }
        for phase in [0, 1, 2] {
            assert!(
                pointers.borrow().contains(&phase),
                "missing gameplay pointer phase {phase}"
            );
        }
        *triggers.borrow_mut() = 0;
        pointers.borrow_mut().clear();
    }
}
