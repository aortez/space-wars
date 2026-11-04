use super::*;
use crate::Sha256;
use crate::boot_linux::PhysicalIdentity;
use crate::bootstrap::trial::TrialInputs;
use crate::bootstrap::trial_store::tests::Fixture;
use crate::recovery::Slot;
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use std::collections::BTreeMap;

fn runtime() -> (Runtime, TrialInputs) {
    let fixture = Fixture::new();
    let mut inputs = fixture.inputs.clone();
    let physical = PhysicalIdentity {
        model: "Raspberry Pi 5 Model B Rev 1.1".into(),
        serial: "abcdef0123456789".into(),
        bootloader_revision: "c".repeat(40),
        medium_sectors: 200_000_000,
        medium: BTreeMap::from([("ID_SERIAL".into(), "fixture-usb-medium".into())]),
    };
    inputs.binding.device_id = physical.fingerprint();
    let geometry = [
        (8192, 307200),
        (319488, 6291456),
        (6610944, 6291456),
        (12902400, 100_000_000),
    ];
    let layout = DiskLayout {
        disk_name: "sda".into(),
        disk_device: DeviceNumber { major: 8, minor: 0 },
        disk_sectors: physical.medium_sectors,
        active: inputs.plan.target,
        layout_id: inputs.binding.layout_id,
        partitions: std::array::from_fn(|index| Partition {
            number: index as u32 + 1,
            name: format!("sda{}", index + 1),
            device: DeviceNumber {
                major: 8,
                minor: index as u32 + 1,
            },
            start_sector: geometry[index].0,
            sectors: geometry[index].1,
            partuuid: format!("076c4a2a-{:02}", index + 1),
        }),
    };
    let runtime = Runtime {
        physical,
        image: inputs.image.clone(),
        layout,
        kernel: inputs.kernel_build_id.clone(),
        profile: inputs.plan.profile.clone(),
        command_line: format!("reboot=w {}", inputs.plan.trial_cmdline),
        boot_id: fixture.boot().boot_id,
        uptime_ms: 5_000,
        tryboot: true,
    };
    (runtime, inputs)
}

#[test]
fn loaded_boot_evidence_must_match_every_selected_identity_and_root() {
    let (runtime, inputs) = runtime();
    inputs.validate().unwrap();
    let boot = observation(&runtime, &inputs, false).unwrap();
    assert_eq!(boot.default_slot, Some(Slot::B));
    assert_eq!(boot.actual_slot, Some(Slot::A));
    assert_eq!(boot.trial_id, Some(inputs.plan.trial_id));
    let mut changes = Vec::new();
    let mut different = runtime.clone();
    different.physical.serial = "fedcba9876543210".into();
    changes.push(different);
    let mut different = runtime.clone();
    different.layout.layout_id = Sha256::digest(b"other layout");
    changes.push(different);
    let mut different = runtime.clone();
    different.layout.active = Slot::B;
    changes.push(different);
    let mut different = runtime.clone();
    different.kernel = "d".repeat(40);
    changes.push(different);
    let mut different = runtime.clone();
    different.image.binaries.client.sha256 = Sha256::digest(b"other executable");
    changes.push(different);
    let mut different = runtime.clone();
    different.profile = "hdmi".into();
    changes.push(different);
    let mut different = runtime.clone();
    different.layout.partitions[1].partuuid = "abcdef12-02".into();
    changes.push(different);
    // inspect_layout derives layout_id from the complete physical layout,
    // including both root UUIDs. Here we also exercise selected-root rendering.
    for different in changes {
        assert!(
            observation(&different, &inputs, false).is_err(),
            "{different:?}"
        );
    }
    let mut wrong_board = inputs.clone();
    wrong_board.plan.board = Board::Pi4;
    assert!(observation(&runtime, &wrong_board, false).is_err());
}

#[test]
fn firmware_trial_state_and_running_command_line_must_agree() {
    let (runtime, inputs) = runtime();
    for command in [
        inputs.plan.normal_cmdline.clone(),
        runtime
            .command_line
            .replace(&inputs.plan.trial_id.to_string(), &"f".repeat(64)),
        runtime.command_line.replace("076c4a2a-02", "076c4a2a-03"),
        format!("{} init=/bin/sh", runtime.command_line.trim()),
        format!("{} root=/dev/sda2", runtime.command_line.trim()),
    ] {
        let mut changed = runtime.clone();
        changed.command_line = command;
        assert!(
            observation(&changed, &inputs, false).is_err(),
            "{}",
            changed.command_line
        );
    }
    let mut normal = runtime;
    normal.tryboot = false;
    assert!(observation(&normal, &inputs, true).is_err());
    normal.command_line = format!("reboot=w {}", inputs.plan.normal_cmdline);
    let boot = observation(&normal, &inputs, true).unwrap();
    assert!(!boot.tryboot);
    assert_eq!(boot.trial_id, None);
    assert_eq!(boot.default_slot, Some(Slot::A));
}
