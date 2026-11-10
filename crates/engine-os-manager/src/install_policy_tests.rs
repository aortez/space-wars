use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn mode(mode: Mode) -> Change {
    Change {
        mode: Some(mode),
        window: None,
    }
}
fn window(start_minute: u16, end_minute: u16) -> Change {
    Change {
        mode: None,
        window: Some(Window {
            start_minute,
            end_minute,
        }),
    }
}
fn saved(generation: u64, mode: Mode, start_minute: u16, end_minute: u16) -> Status {
    Status {
        generation,
        mode,
        window: Window {
            start_minute,
            end_minute,
        },
    }
}

#[test]
fn a_unit_that_keeps_the_defaults_has_no_saved_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-policy");
    let preference = Preference::open(&path);
    let defaults = saved(0, Mode::Ask, 180, 300);
    assert_eq!(preference.status(), Some(defaults.clone()));
    // Choosing what is already in effect is not a change.
    assert_eq!(preference.change(mode(Mode::Ask)).unwrap(), defaults);
    assert_eq!(preference.change(window(180, 300)).unwrap(), defaults);
    assert!(!path.exists());
}

#[test]
fn each_setting_changes_alone_and_survives_reopening() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-policy");
    let preference = Preference::open(&path);
    assert_eq!(
        preference.change(mode(Mode::Automatic)).unwrap(),
        saved(1, Mode::Automatic, 180, 300)
    );
    // The on-disk shape is what an older or newer image must read after an update.
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("state.json")).unwrap()).unwrap();
    assert_eq!(
        envelope["record"],
        serde_json::json!({
            "schema": 1, "generation": 1, "mode": "automatic",
            "window": {"start_minute": 180, "end_minute": 300},
        })
    );
    for entry in [path.clone(), path.join("state.json")] {
        assert_eq!(fs::metadata(entry).unwrap().permissions().mode() & 0o077, 0);
    }
    let overnight = saved(2, Mode::Automatic, 22 * 60 + 30, 60);
    assert_eq!(
        preference.change(window(22 * 60 + 30, 60)).unwrap(),
        overnight
    );
    let before = fs::read(path.join("state.json")).unwrap();
    assert_eq!(preference.change(mode(Mode::Automatic)).unwrap(), overnight);
    assert_eq!(fs::read(path.join("state.json")).unwrap(), before);
    assert_eq!(preference.status(), Some(overnight.clone()));
    drop(preference);
    let reopened = Preference::open(&path);
    assert_eq!(reopened.status(), Some(overnight));
    assert_eq!(
        reopened.change(mode(Mode::Ask)).unwrap(),
        saved(3, Mode::Ask, 22 * 60 + 30, 60)
    );
}

#[test]
fn invalid_changes_write_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-policy");
    let preference = Preference::open(&path);
    for change in [
        Change::default(),
        window(180, 180),
        window(24 * 60, 60),
        window(60, 24 * 60),
        Change {
            mode: Some(Mode::Automatic),
            window: Some(Window {
                start_minute: 0,
                end_minute: 0,
            }),
        },
    ] {
        assert!(preference.change(change).is_err(), "{change:?}");
    }
    assert!(!path.exists());
    assert_eq!(preference.status(), Some(saved(0, Mode::Ask, 180, 300)));
}

#[test]
fn window_text_is_a_strict_24_hour_interval() {
    for (text, start_minute, end_minute) in [
        ("03:00-05:00", 180, 300),
        ("22:30-01:00", 22 * 60 + 30, 60),
        ("00:00-23:59", 0, 23 * 60 + 59),
        ("23:59-00:00", 23 * 60 + 59, 0),
    ] {
        assert_eq!(
            text.parse::<Window>(),
            Ok(Window {
                start_minute,
                end_minute
            }),
            "{text}"
        );
    }
    for text in [
        "",
        "03:00",
        "3:00-5:00",
        "03:00-03:00",
        "24:00-01:00",
        "03:60-04:00",
        "03:00-05:00 ",
        " 03:00-05:00",
        "03:00\u{2013}05:00",
        "+3:00-05:00",
        "03:00-05:00-07:00",
        "03.00-05.00",
        "0300-0500",
    ] {
        assert!(text.parse::<Window>().is_err(), "{text:?}");
    }
}
