use super::*;
use crate::network::Inventory;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::sync::{Arc, Mutex};

struct TestPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let mut windows = self.0.borrow_mut();
        let window = MinimalSoftwareWindow::new(if windows.is_empty() {
            RepaintBufferType::ReusedBuffer
        } else {
            RepaintBufferType::NewBuffer
        });
        windows.push(window.clone());
        Ok(window)
    }
}

fn test_view() -> View {
    View {
        inventory: Inventory {
            can_connect: true,
            can_scan: true,
            can_manage: true,
            profiles: Vec::new(),
            devices: vec!["/wifi".into()],
            networks: vec![
                Network {
                    id: "test".into(),
                    name: "Family network".into(),
                    interface: "wlan0".into(),
                    security: Security::WpaPsk,
                    strength: 75,
                    ..Network::default()
                },
                Network {
                    id: "connected".into(),
                    name: "Connected family network".into(),
                    interface: "wlan0".into(),
                    security: Security::WpaPsk,
                    strength: 92,
                    connected: true,
                    saved: Some("/saved".into()),
                    ..Network::default()
                },
                Network {
                    id: "saved".into(),
                    name: "Saved guest network".into(),
                    interface: "wlan0".into(),
                    security: Security::Sae,
                    strength: 40,
                    saved: Some("/guest".into()),
                    ..Network::default()
                },
            ],
            summary: "Choose a nearby Wi-Fi network".into(),
        },
        ..View::default()
    }
}

#[test]
fn keyboard_includes_every_printable_ascii_character() {
    for page in 0..3 {
        assert_eq!(alphabet(page).chars().count(), 40);
    }
    for c in '!'..='~' {
        assert!((0..3).any(|page| alphabet(page).contains(c)), "missing {c}");
    }
}

#[test]
fn network_ui_navigation_password_privacy_and_cabinet_layouts() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let window = MainWindow::new().unwrap();
    window.show().unwrap();
    let reference = MainWindow::new().unwrap();
    reference.show().unwrap();
    let adapter = windows.borrow()[0].clone();
    let reference_adapter = windows.borrow()[1].clone();
    let views = Rc::new(RefCell::new(Vec::<Arc<Mutex<View>>>::new()));
    let receivers = Rc::new(RefCell::new(Vec::new()));
    let session_views = views.clone();
    let session_receivers = receivers.clone();
    let state = install_with(&window, move || {
        let (session, view, receiver) = Session::simulated(test_view());
        session_views.borrow_mut().push(view);
        session_receivers.borrow_mut().push(receiver);
        session
    });
    let output = std::env::var_os("SPACEWARS_NETWORK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        adapter.set_size(PhysicalSize::new(width, height));
        reference_adapter.set_size(PhysicalSize::new(width, height));
        window.set_launcher_visible(true);
        window.set_sound_visible(true);
        window.invoke_network_open();
        slint::platform::update_timers_and_animations();
        assert!(state.borrow().is_some());
        let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
        let mut reference_pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
        for stage in [
            "networks",
            "scanning",
            "selected",
            "password",
            "revealed",
            "concealed",
            "invalid",
            "long-revealed",
            "connecting",
            "confirm",
            "recovered",
        ] {
            match stage {
                "scanning" => {
                    views.borrow().last().unwrap().lock().unwrap().phase = Phase::Discovering;
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                    let rows = window.get_network_rows();
                    assert!(rows.iter().any(|r| r.id == "network.select.test"));
                    assert!(!rows.iter().any(|r| r.id == "network.cancel"));
                    assert!(!rows.iter().any(|r| r.id == "network.refresh"));
                    assert!(rows.iter().any(|r| r.id == "network.scan" && !r.enabled));
                    assert!(rows.iter().any(|r| r.badge == "Connected"));
                    assert!(rows.iter().any(|r| r.badge == "Saved"));
                }
                "selected" => {
                    views.borrow().last().unwrap().lock().unwrap().phase = Phase::Idle;
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                    window.set_network_focus_index(
                        window
                            .get_network_rows()
                            .iter()
                            .position(|r| r.id == "network.select.test")
                            .unwrap() as i32,
                    );
                    // Signal changes can reorder rows, but must not move the
                    // selection to a different SSID under the user's finger.
                    views
                        .borrow()
                        .last()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .inventory
                        .networks
                        .reverse();
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                    assert_eq!(
                        window
                            .get_network_rows()
                            .row_data(window.get_network_focus_index() as usize)
                            .unwrap()
                            .id,
                        "network.select.test"
                    );
                    assert!(crate::ui_activation::activate(
                        &window,
                        "network.select.test"
                    ));
                }
                "password" => {
                    window.invoke_network_command("network.password".into());
                    window.invoke_network_text("abcdAB12?".into());
                    assert!(!format!("{:?}", inventory(&window)).contains("abcdAB12?"));
                    assert_eq!(state.borrow().as_ref().unwrap().password, "abcdAB12?");
                    // Keyboard capture keeps letters such as p/r/q from reaching
                    // global pause/restart/quit shortcuts.
                    adapter.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                        text: "p".into(),
                    });
                    adapter.dispatch_event(slint::platform::WindowEvent::KeyReleased {
                        text: "p".into(),
                    });
                    assert_eq!(state.borrow().as_ref().unwrap().password, "abcdAB12?p");
                    handle_action(&window, UiAction::Down);
                    assert_eq!(
                        window.get_network_focus_index(),
                        window.get_network_keyboard_columns()
                    );
                }
                "revealed" => {
                    window.invoke_network_command("network.reveal".into());
                    assert!(window.get_network_password_revealed());
                    assert_eq!(window.get_network_password_text(), "abcdAB12?p");
                    assert!(!format!("{:?}", inventory(&window)).contains("abcdAB12?"));
                    let controls = crate::ui_inventory::inventory_for_screen(
                        spacewars_control::UiScreen::PauseNetwork,
                        &crate::ui_inventory::UiInventoryContext {
                            network_controls: inventory(&window),
                            ..Default::default()
                        },
                    );
                    assert!(controls.actions.contains(&UiAction::Start));
                    assert!(controls.actions.contains(&UiAction::Controls));
                    assert!(!format!("{controls:?}").contains("abcdAB12?"));
                    let focus = window.get_network_focus_index();
                    // Scan refreshes must not erase input or reset keyboard focus.
                    views.borrow().last().unwrap().lock().unwrap().phase = Phase::Discovering;
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                    assert_eq!(state.borrow().as_ref().unwrap().password, "abcdAB12?p");
                    assert_eq!(window.get_network_focus_index(), focus);
                    views.borrow().last().unwrap().lock().unwrap().phase = Phase::Idle;
                }
                "concealed" => {
                    window.invoke_network_conceal();
                    assert!(!window.get_network_password_revealed());
                    assert!(window.get_network_password_text().is_empty());
                    assert_eq!(state.borrow().as_ref().unwrap().password, "abcdAB12?p");
                    // Hiding is also explicitly available on the same button.
                    window.invoke_network_command("network.reveal".into());
                    window.invoke_network_command("network.reveal".into());
                    assert!(!window.get_network_password_revealed());
                }
                "invalid" => {
                    for _ in 0..10 {
                        handle_action(&window, UiAction::Controls);
                    }
                    handle_action(&window, UiAction::Start);
                    assert!(window.get_network_password_visible());
                    assert!(receivers.borrow().last().unwrap().is_empty());
                }
                "connecting" => {
                    for _ in 0..64 {
                        handle_action(&window, UiAction::Controls);
                    }
                    window.invoke_network_text("testpass123".into());
                    handle_action(&window, UiAction::Start);
                    assert!(!window.get_network_password_visible());
                    assert!(state.borrow().as_ref().unwrap().password.is_empty());
                    assert!(!window.get_network_password_revealed());
                    assert!(window.get_network_password_text().is_empty());
                    let controls = crate::ui_inventory::inventory_for_screen(
                        spacewars_control::UiScreen::PauseNetwork,
                        &crate::ui_inventory::UiInventoryContext {
                            network_controls: inventory(&window),
                            ..Default::default()
                        },
                    );
                    assert!(!controls.actions.contains(&UiAction::Start));
                    assert!(
                        matches!(receivers.borrow().last().unwrap().try_recv(), Ok(Command::Connect { password: Some(p), .. }) if p == "testpass123")
                    );
                }
                "confirm" => {
                    *views.borrow().last().unwrap().lock().unwrap() = View {
                        phase: Phase::Confirm,
                        status:
                            "Connected. Keep this network? Restoring in 30 seconds unless kept."
                                .into(),
                        ..test_view()
                    };
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                    assert!(inventory(&window).iter().any(|c| c.id == "network.keep"));
                }
                "recovered" => {
                    window.invoke_network_command("network.cancel".into());
                    *views.borrow().last().unwrap().lock().unwrap() = View {
                        status: "Previous network configuration restored.".into(),
                        ..test_view()
                    };
                    state.borrow_mut().as_mut().unwrap().publish(&window);
                }
                "long-revealed" => {
                    window.invoke_network_text("W".repeat(64).into());
                    window.invoke_network_command("network.reveal".into());
                    assert_eq!(window.get_network_password_text().len(), 64);
                }
                _ => {}
            }
            reference.set_launcher_visible(true);
            reference.set_sound_visible(true);
            reference.set_network_visible(true);
            reference.set_network_title(window.get_network_title());
            reference.set_network_subtitle(window.get_network_subtitle());
            reference.set_network_detail(window.get_network_detail());
            reference.set_network_password_visible(window.get_network_password_visible());
            reference.set_network_password_revealed(window.get_network_password_revealed());
            reference.set_network_password_text(window.get_network_password_text());
            reference.set_network_password_mask(window.get_network_password_mask());
            reference.set_network_password_caption(window.get_network_password_caption());
            reference.set_network_rows(window.get_network_rows());
            reference.set_network_focus_index(window.get_network_focus_index());
            reference_adapter.request_redraw();
            // Opening/replacing a panel can schedule deferred layout work.
            // Drain those event-loop turns before capturing its settled frame.
            for _ in 0..4 {
                slint::platform::update_timers_and_animations();
                let drawn = adapter.draw_if_needed(|renderer| {
                    renderer.render(pixels.make_mut_slice(), width as usize);
                });
                let reference_drawn = reference_adapter.draw_if_needed(|renderer| {
                    renderer.render(reference_pixels.make_mut_slice(), width as usize);
                });
                if !drawn && !reference_drawn {
                    break;
                }
            }
            if let Some(output) = &output {
                crate::thruster_visual_tests::write_png(
                    &output.join(format!("network-{width}x{height}-{stage}.png")),
                    &pixels,
                );
            }
            assert!(
                pixels.as_bytes() == reference_pixels.as_bytes(),
                "{width}x{height} {stage}: partial repaint differs from full repaint"
            );
            let heading_y = (height - (height - 24).min(680)) / 2 + 12;
            let heading_ink = pixels
                .as_slice()
                .chunks_exact(width as usize)
                .skip(heading_y as usize)
                .take(40)
                .flatten()
                .filter(|p| p.r > 220 && p.g > 220 && p.b > 220)
                .count();
            assert!(
                heading_ink > 200,
                "{width}x{height} {stage}: missing heading"
            );
        }
        window.invoke_network_command("network.select.test".into());
        window.invoke_network_command("network.password".into());
        window.invoke_network_text("secret123".into());
        window.invoke_network_command("network.reveal".into());
        handle_action(&window, UiAction::Back);
        assert!(!window.get_network_password_visible());
        assert!(!window.get_network_password_revealed());
        assert!(window.get_network_password_text().is_empty());
        window.invoke_network_command("network.password".into());
        assert!(state.borrow().as_ref().unwrap().password.is_empty());
        assert!(!window.get_network_password_revealed());
        window.invoke_network_text("\"Quotes\\backslash? <&> space!".into());
        window.invoke_network_command("network.reveal".into());
        assert_eq!(
            window.get_network_password_text(),
            "\"Quotes\\backslash? <&> space!"
        );
        window.invoke_network_text("W".repeat(64).into());
        assert_eq!(state.borrow().as_ref().unwrap().password.len(), 64);
        assert_eq!(window.get_network_password_text().len(), 64);
        handle_action(&window, UiAction::Back);
        handle_action(&window, UiAction::Back);
        handle_action(&window, UiAction::Back);
        slint::platform::update_timers_and_animations();
        assert!(!window.get_network_visible());
        assert!(window.get_sound_visible());
        assert!(state.borrow().is_none());
        assert!(receivers.borrow().last().unwrap().is_closed());
    }
    window.set_launcher_visible(false);
    window.set_ingame_menu_visible(true);
    window.invoke_network_open();
    slint::platform::update_timers_and_animations();
    handle_action(&window, UiAction::Start);
    assert!(window.get_ingame_menu_visible());
    assert!(window.get_network_visible());
    window.invoke_network_command("network.select.test".into());
    window.invoke_network_command("network.password".into());
    window.invoke_network_text("secret123".into());
    window.invoke_network_command("network.reveal".into());
    window.set_sound_visible(false);
    slint::platform::update_timers_and_animations();
    assert!(!window.get_network_visible());
    assert!(state.borrow().is_none());
    assert!(!window.get_network_password_revealed());
    assert!(window.get_network_password_text().is_empty());
}

#[test]
fn keyboard_vertical_navigation_covers_all_action_columns() {
    for columns in [8, 10] {
        for index in 0..47 {
            for down in [false, true] {
                assert!(keyboard_vertical(index, columns, down) < 47);
            }
        }
        for action in 44..47 {
            assert!((0..columns).any(|i| keyboard_vertical(i, columns, false) == action));
            assert!((40..44).any(|i| keyboard_vertical(i, columns, true) == action));
        }
    }
}

fn saved_view() -> View {
    let mut view = test_view();
    view.status.clear();
    view.inventory.profiles = (0..3)
        .map(|index| {
            let id = format!("0000000{index}-0000-0000-0000-00000000000{index}");
            SavedNetwork {
                id,
                path: format!("/saved/{index}"),
                name: "Home connection".into(),
                ssid_name: "Family network".into(),
                autoconnect: index != 1,
                priority: if index == 0 { 50 } else { 0 },
                connected: index == 0,
                network: (index != 2).then(|| Network {
                    saved: Some(format!("/saved/{index}")),
                    connected: index == 0,
                    security: Security::WpaPsk,
                    ..Network::default()
                }),
            }
        })
        .collect();
    view
}

#[test]
fn saved_network_controller_touch_confirmation_and_stale_profile_flows() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let window = MainWindow::new().unwrap();
    window.show().unwrap();
    let adapter = windows.borrow()[0].clone();
    let (session, latest, receiver) = Session::simulated(saved_view());
    let once = RefCell::new(Some(session));
    let state = install_with(&window, move || once.borrow_mut().take().unwrap());
    let ids: Vec<_> = latest
        .lock()
        .unwrap()
        .inventory
        .profiles
        .iter()
        .map(|p| p.id.clone())
        .collect();
    let output = std::env::var_os("SPACEWARS_NETWORK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    window.set_launcher_visible(true);
    window.set_sound_visible(true);
    window.invoke_network_open();
    slint::platform::update_timers_and_animations();
    let focus = |id: &str| {
        let index = window
            .get_network_rows()
            .iter()
            .position(|r| r.id == id && r.enabled)
            .unwrap();
        window.set_network_focus_index(index as i32);
    };
    let click = |x, y| {
        slint::platform::update_timers_and_animations();
        let position = slint::LogicalPosition::new(x, y);
        for event in [
            slint::platform::WindowEvent::PointerPressed {
                position,
                button: slint::platform::PointerEventButton::Left,
            },
            slint::platform::WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            },
        ] {
            window.window().dispatch_event(event);
        }
    };
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        adapter.set_size(PhysicalSize::new(width, height));
        *latest.lock().unwrap() = saved_view();
        state.borrow_mut().as_mut().unwrap().page = Page::List;
        state.borrow_mut().as_mut().unwrap().publish(&window);
        window.set_network_focus_index(0);
        let top = (height - (height - 24).min(680)) as f32 / 2.;
        // Actual pointer hit testing uses the shared MenuButton, not its callback directly.
        click(width as f32 / 2., top + 154. + 60. + 26.);
        assert!(state.borrow().as_ref().unwrap().page == Page::Saved);
        assert_eq!(
            window
                .get_network_rows()
                .iter()
                .filter(|r| r.id.starts_with("network.profile."))
                .count(),
            3
        );
        assert!(
            window
                .get_network_rows()
                .iter()
                .any(|r| r.badge == "Preferred")
        );
        for (stage, profile_index) in [
            ("saved-profiles", None),
            ("saved-offline", Some(2)),
            ("forget-active", Some(0)),
        ] {
            if let Some(index) = profile_index {
                state.borrow_mut().as_mut().unwrap().page = Page::Saved;
                state.borrow_mut().as_mut().unwrap().publish(&window);
                focus(&format!("network.profile.{}", ids[index]));
                handle_action(&window, UiAction::Confirm);
                assert!(state.borrow().as_ref().unwrap().page == Page::Profile(ids[index].clone()));
                assert!(window.get_network_detail().contains(&ids[index]));
                if index == 2 {
                    assert!(window.get_network_subtitle().contains("Out of range"));
                    assert!(
                        window
                            .get_network_rows()
                            .iter()
                            .any(|r| r.id == "network.profile-connect" && !r.enabled)
                    );
                    assert!(
                        window
                            .get_network_rows()
                            .iter()
                            .any(|r| r.id == "network.forget" && r.enabled)
                    );
                } else {
                    focus("network.forget");
                    handle_action(&window, UiAction::Confirm);
                    assert!(window.get_network_detail().contains("remote access"));
                    assert!(window.get_network_detail().contains(&ids[0]));
                    assert_eq!(
                        window.get_network_focus_index(),
                        0,
                        "Cancel is the safe initial choice"
                    );
                    assert!(receiver.is_empty());
                }
            }
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            adapter.request_redraw();
            for _ in 0..4 {
                slint::platform::update_timers_and_animations();
                adapter.draw_if_needed(|r| {
                    r.render(pixels.make_mut_slice(), width as usize);
                });
            }
            if let Some(output) = &output {
                crate::thruster_visual_tests::write_png(
                    &output.join(format!("network-{width}x{height}-{stage}.png")),
                    &pixels,
                );
            }
        }
        click(width as f32 / 2., top + 90. + 144. + 26.);
        assert!(state.borrow().as_ref().unwrap().page == Page::Profile(ids[0].clone()));
        assert!(
            receiver.is_empty(),
            "touch Cancel must never enqueue deletion"
        );
    }
    // A connection becoming active while its confirmation is displayed must
    // show the stronger warning and move focus back to Cancel.
    state.borrow_mut().as_mut().unwrap().page = Page::Profile(ids[2].clone());
    state.borrow_mut().as_mut().unwrap().publish(&window);
    window.invoke_network_command("network.forget".into());
    focus("network.forget-confirm");
    latest.lock().unwrap().inventory.profiles[2].connected = true;
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert_eq!(window.get_network_focus_index(), 0);
    assert!(window.get_network_detail().contains("remote access"));
    handle_action(&window, UiAction::Down);
    handle_action(&window, UiAction::Confirm);
    assert!(
        matches!(receiver.try_recv().unwrap(), Command::Manage { id, change: ProfileChange::Forget { allow_active: true } } if id == ids[2])
    );
    assert_eq!(latest.lock().unwrap().phase, Phase::Managing);
    window.invoke_network_command("network.autoconnect".into());
    assert!(receiver.is_empty());
    // A completed deletion must not label another existing profile "forgotten".
    *latest.lock().unwrap() = saved_view();
    latest.lock().unwrap().status = "Saved network forgotten.".into();
    state.borrow_mut().as_mut().unwrap().page = Page::Profile(ids[0].clone());
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(!window.get_network_detail().contains("forgotten"));
    assert!(
        window
            .get_network_detail()
            .contains("future automatic connections")
    );
    window.invoke_network_command("network.autoconnect".into());
    assert!(matches!(receiver.try_recv().unwrap(), Command::Manage { id, .. } if id == ids[0]));
    latest.lock().unwrap().phase = Phase::Idle;
    latest.lock().unwrap().status = "Could not save preference.".into();
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(
        window
            .get_network_detail()
            .contains("Could not save preference")
    );
    // Manual connection is still available for an autoconnect-disabled profile.
    *latest.lock().unwrap() = saved_view();
    state.borrow_mut().as_mut().unwrap().page = Page::Profile(ids[1].clone());
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(
        window
            .get_network_rows()
            .iter()
            .any(|r| r.id == "network.prefer" && !r.enabled)
    );
    focus("network.profile-connect");
    handle_action(&window, UiAction::Confirm);
    assert!(matches!(receiver.try_recv().unwrap(), Command::ConnectProfile { id } if id == ids[1]));
    latest.lock().unwrap().phase = Phase::Confirm;
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(
        !window
            .get_network_rows()
            .iter()
            .any(|r| r.id == "network.saved" || r.id == "network.forget")
    );
    window.invoke_network_command("network.forget-confirm".into());
    assert!(receiver.is_empty());
    // Lost profiles and inventory errors cannot retarget management to a duplicate.
    *latest.lock().unwrap() = saved_view();
    state.borrow_mut().as_mut().unwrap().page = Page::Profile(ids[1].clone());
    latest.lock().unwrap().inventory.profiles.remove(1);
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(state.borrow().as_ref().unwrap().page == Page::Saved);
    window.invoke_network_command("network.forget".into());
    assert!(receiver.is_empty());
    *latest.lock().unwrap() = saved_view();
    latest.lock().unwrap().inventory.can_manage = false;
    latest.lock().unwrap().inventory.can_connect = false;
    latest.lock().unwrap().inventory_error = Some("Read failed".into());
    state.borrow_mut().as_mut().unwrap().page = Page::Profile(ids[1].clone());
    state.borrow_mut().as_mut().unwrap().publish(&window);
    assert!(
        window
            .get_network_rows()
            .iter()
            .filter(|r| r.id != "network.back")
            .all(|r| !r.enabled)
    );
    assert!(window.get_network_detail().contains("Read failed"));
}
