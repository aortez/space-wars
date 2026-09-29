use super::*;

fn device() -> Device {
    Device {
        id: "/org/bluez/hci0/dev_00_11_22_33_44_55".into(),
        adapter: "/org/bluez/hci0".into(),
        name: "Test gamepad".into(),
        address: "00:11:22:33:44:55".into(),
        paired: true,
        connected: true,
    }
}

fn panel() -> (Panel, async_channel::Receiver<Command>) {
    let (commands, receiver) = async_channel::bounded(4);
    let view = View {
        inventory: Inventory {
            adapter: Some("/org/bluez/hci0".into()),
            devices: vec![device()],
        },
        ..View::default()
    };
    (
        Panel {
            session: Session {
                commands,
                latest: Arc::new(Mutex::new(view.clone())),
            },
            view,
            page: Page::Devices,
        },
        receiver,
    )
}

#[test]
fn forget_requires_device_selection_and_confirmation() {
    let (mut panel, receiver) = panel();
    panel.command("controllers.bluetooth.confirm-forget");
    assert!(receiver.try_recv().is_err());
    panel.command(&format!("controllers.bluetooth.device.{}", device().id));
    panel.command("controllers.bluetooth.forget");
    assert!(receiver.try_recv().is_err());
    assert!(panel.detail().contains("must be paired again"));
    assert!(!panel.back());
    panel.command("controllers.bluetooth.confirm-forget");
    assert!(receiver.try_recv().is_err());
    panel.command("controllers.bluetooth.forget");
    panel.command("controllers.bluetooth.confirm-forget");
    assert!(
        matches!(receiver.try_recv(), Ok(Command::Act(Action::Forget, id)) if id == device().id)
    );
}

#[test]
fn pending_action_gates_duplicate_clicks_and_close_cancels_session() {
    let (mut panel, receiver) = panel();
    panel.command("controllers.bluetooth.scan");
    panel.command("controllers.bluetooth.scan");
    assert!(matches!(receiver.try_recv(), Ok(Command::Scan)));
    assert!(receiver.try_recv().is_err());
    panel.poll();
    assert!(panel.view.busy);
    assert_eq!(panel.rows().len(), 1);
    assert!(panel.back());
    drop(panel);
    assert!(receiver.is_closed());
}

#[test]
fn disappearance_during_forget_confirmation_cannot_target_another_device() {
    let (mut panel, receiver) = panel();
    panel.command(&format!("controllers.bluetooth.device.{}", device().id));
    panel.command("controllers.bluetooth.forget");
    panel
        .session
        .latest
        .lock()
        .unwrap()
        .inventory
        .devices
        .clear();
    panel.poll();
    panel.command("controllers.bluetooth.confirm-forget");
    assert!(receiver.try_recv().is_err());
    assert!(matches!(panel.page, Page::Devices));
}

#[test]
fn nearby_pads_offer_pair_and_remembered_pads_offer_connection_management() {
    let (mut panel, _) = panel();
    panel.page = Page::Device(device().id);
    assert!(
        panel
            .rows()
            .iter()
            .any(|(id, _)| id.ends_with(".disconnect"))
    );
    panel.view.inventory.devices[0].connected = false;
    assert!(panel.rows().iter().any(|(id, _)| id.ends_with(".connect")));
    panel.view.inventory.devices[0].paired = false;
    let rows = panel.rows();
    assert!(rows.iter().any(|(id, _)| id.ends_with(".pair")));
    assert!(!rows.iter().any(|(id, _)| id.ends_with(".forget")));
}

#[test]
fn device_detail_tracks_live_connection_after_pairing_and_reconnection() {
    let (mut panel, _) = panel();
    panel.command(&format!("controllers.bluetooth.device.{}", device().id));
    panel.session.latest.lock().unwrap().status = Action::Pair.completed_message().into();
    panel.poll();
    assert!(panel.detail().contains("Bluetooth: Connected"));

    panel.session.latest.lock().unwrap().inventory.devices[0].connected = false;
    panel.poll();
    assert!(panel.detail().contains("Bluetooth: Paired, disconnected"));
    assert!(!panel.detail().contains("Connected"));
    assert!(panel.rows().iter().any(|(id, _)| id.ends_with(".connect")));

    // No successful operation notice should keep claiming a live connection.
    for action in [Action::Pair, Action::Connect, Action::Disconnect] {
        panel.session.latest.lock().unwrap().status = action.completed_message().into();
        panel.poll();
        assert!(panel.detail().contains("Bluetooth: Paired, disconnected"));
        assert!(!panel.detail().contains("Connected"));
    }

    panel.session.latest.lock().unwrap().inventory.devices[0].connected = true;
    panel.poll();
    assert!(panel.detail().contains("Bluetooth: Connected"));
    assert!(
        panel
            .rows()
            .iter()
            .any(|(id, _)| id.ends_with(".disconnect"))
    );

    panel.session.latest.lock().unwrap().status = "Input connection failed. Try again.".into();
    panel.poll();
    assert!(panel.detail().contains("Input connection failed"));
}
