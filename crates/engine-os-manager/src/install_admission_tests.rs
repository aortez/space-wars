use super::*;
use jiff::civil::date;
use std::fs;

const LOS_ANGELES: &str = "America/Los_Angeles";
const IMAGE: Option<&str> = Some("rootfs-aaaa");

fn at(utc: &str, zone: &str) -> Moment {
    Moment::at(
        engine_ota::UsableTime(utc.parse().unwrap()),
        &TimeZone::get(zone).unwrap(),
    )
}
/// Pacific Standard Time is eight hours behind UTC throughout these dates.
fn pst(utc: &str) -> Moment {
    at(utc, LOS_ANGELES)
}
fn policy(mode: Mode, start_minute: u16, end_minute: u16) -> Policy {
    Policy {
        generation: 1,
        mode,
        window: Window {
            start_minute,
            end_minute,
        },
    }
}
fn automatic(start_minute: u16, end_minute: u16) -> Policy {
    policy(Mode::Automatic, start_minute, end_minute)
}
fn night() -> Policy {
    automatic(3 * 60, 5 * 60)
}
fn history(temp: &tempfile::TempDir) -> History {
    History::open(&temp.path().join("install-admission"))
}
fn gate(history: &History, policy: &Policy, now: &Moment) -> Gate {
    history.status(Some(policy), Some(now), IMAGE).gate
}
fn begin(history: &History, policy: &Policy, now: &Moment) -> Gate {
    history.begin(Some(policy), Some(now), IMAGE).unwrap()
}

#[test]
fn local_time_follows_the_zone() {
    assert_eq!(
        pst("2026-11-10T11:30:00Z"),
        Moment {
            utc_s: 1_794_310_200,
            date: date(2026, 11, 10),
            minute: 3 * 60 + 30,
        }
    );
    let tokyo = at("2026-11-10T11:30:00Z", "Asia/Tokyo");
    assert_eq!(
        (tokyo.date, tokyo.minute),
        (date(2026, 11, 10), 20 * 60 + 30)
    );
    let summer = at("2026-07-01T06:59:59Z", LOS_ANGELES);
    assert_eq!(
        (summer.date, summer.minute),
        (date(2026, 6, 30), 23 * 60 + 59)
    );
}

#[test]
fn only_automatic_mode_with_a_ready_image_and_known_local_time_opens() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    let inside = pst("2026-11-10T11:30:00Z");
    let status = |policy: Option<&Policy>, now: Option<&Moment>, image| {
        history.status(policy, now, image).gate
    };
    assert_eq!(status(None, Some(&inside), IMAGE), Gate::NotAutomatic);
    assert_eq!(
        status(Some(&policy(Mode::Ask, 180, 300)), Some(&inside), IMAGE),
        Gate::NotAutomatic
    );
    assert_eq!(status(Some(&night()), Some(&inside), None), Gate::NoImage);
    assert_eq!(status(Some(&night()), None, IMAGE), Gate::TimeUnknown);
    assert_eq!(status(Some(&night()), Some(&inside), IMAGE), Gate::Open);
    // Asking never spends the window or creates state, and neither does a
    // refused beginning.
    assert_eq!(
        history
            .begin(Some(&policy(Mode::Ask, 180, 300)), Some(&inside), IMAGE)
            .unwrap(),
        Gate::NotAutomatic
    );
    assert_eq!(
        history.begin(Some(&night()), None, IMAGE).unwrap(),
        Gate::TimeUnknown
    );
    assert_eq!(status(Some(&night()), Some(&inside), IMAGE), Gate::Open);
    assert!(!temp.path().join("install-admission").exists());
}

#[test]
fn the_window_is_half_open_in_local_time() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    for (utc, expected) in [
        ("2026-11-10T10:59:59Z", Gate::OutsideWindow),
        ("2026-11-10T11:00:00Z", Gate::Open),
        ("2026-11-10T12:59:59Z", Gate::Open),
        ("2026-11-10T13:00:00Z", Gate::OutsideWindow),
        ("2026-11-10T23:30:00Z", Gate::OutsideWindow),
    ] {
        assert_eq!(gate(&history, &night(), &pst(utc)), expected, "{utc}");
    }
}

#[test]
fn a_window_gets_one_attempt_and_the_next_night_gets_another() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-admission");
    let history = History::open(&path);
    let first = pst("2026-11-10T11:10:00Z");
    assert_eq!(begin(&history, &night(), &first), Gate::Open);
    assert_eq!(
        history.status(Some(&night()), Some(&first), IMAGE),
        Status {
            gate: Gate::WindowUsed,
            local: Some(Local {
                date: date(2026, 11, 10),
                minute: 3 * 60 + 10,
            }),
            last_attempt: Some(Attempt {
                opened: date(2026, 11, 10),
                utc_s: first.utc_s,
            }),
            tried_build_id: None,
        }
    );
    // The on-disk shape is what an older or newer image must read after an update.
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("state.json")).unwrap()).unwrap();
    assert_eq!(
        envelope["record"],
        serde_json::json!({
            "schema": 1, "generation": 1,
            "attempt": {"opened": "2026-11-10", "utc_s": first.utc_s},
            "tried_build_id": null,
        })
    );
    let before = fs::read(path.join("state.json")).unwrap();
    let later = pst("2026-11-10T12:50:00Z");
    assert_eq!(begin(&history, &night(), &later), Gate::WindowUsed);
    assert_eq!(fs::read(path.join("state.json")).unwrap(), before);

    // A restart in the same window finds it spent; the next night is new.
    drop(history);
    let history = History::open(&path);
    assert_eq!(gate(&history, &night(), &later), Gate::WindowUsed);
    let next = pst("2026-11-11T11:00:00Z");
    assert_eq!(begin(&history, &night(), &next), Gate::Open);
    assert_eq!(gate(&history, &night(), &next), Gate::WindowUsed);
}

#[test]
fn a_window_across_midnight_is_one_window() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    let late = automatic(22 * 60 + 30, 60);
    for (utc, expected) in [
        ("2026-11-10T06:29:59Z", Gate::OutsideWindow),
        ("2026-11-10T06:30:00Z", Gate::Open),
        ("2026-11-10T08:59:59Z", Gate::Open),
        ("2026-11-10T09:00:00Z", Gate::OutsideWindow),
    ] {
        assert_eq!(gate(&history, &late, &pst(utc)), expected, "{utc}");
    }
    // Half past midnight on the 10th belongs to the window that opened on the 9th.
    let early = pst("2026-11-10T08:30:00Z");
    assert_eq!(begin(&history, &late, &early), Gate::Open);
    let attempt = history
        .status(Some(&late), Some(&early), IMAGE)
        .last_attempt;
    assert_eq!(attempt.unwrap().opened, date(2026, 11, 9));

    let temp = tempfile::tempdir().unwrap();
    let history = self::history(&temp);
    assert_eq!(
        begin(&history, &late, &pst("2026-11-10T07:00:00Z")),
        Gate::Open
    );
    assert_eq!(
        gate(&history, &late, &pst("2026-11-10T08:30:00Z")),
        Gate::WindowUsed
    );
    assert_eq!(
        gate(&history, &late, &pst("2026-11-11T06:30:00Z")),
        Gate::Open
    );
}

#[test]
fn a_changed_window_cannot_reopen_the_same_night() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    // Half past midnight in a window that opened the evening before.
    assert_eq!(
        begin(
            &history,
            &automatic(23 * 60, 6 * 60),
            &pst("2026-11-10T08:30:00Z")
        ),
        Gate::Open
    );
    // A new window that opens after midnight has a later date but no gap.
    let changed = automatic(60, 5 * 60);
    assert_eq!(
        gate(&history, &changed, &pst("2026-11-10T09:10:00Z")),
        Gate::WindowUsed
    );
    assert_eq!(
        gate(&history, &changed, &pst("2026-11-11T09:10:00Z")),
        Gate::Open
    );

    // A window that covers nearly the whole day still waits the minimum gap.
    let temp = tempfile::tempdir().unwrap();
    let history = self::history(&temp);
    let all_day = automatic(0, 23 * 60 + 59);
    assert_eq!(
        begin(&history, &all_day, &pst("2026-11-10T07:58:00Z")),
        Gate::Open
    );
    assert_eq!(
        gate(&history, &all_day, &pst("2026-11-10T08:01:00Z")),
        Gate::WindowUsed
    );
    assert_eq!(
        gate(&history, &all_day, &pst("2026-11-10T19:57:59Z")),
        Gate::WindowUsed
    );
    assert_eq!(
        gate(&history, &all_day, &pst("2026-11-10T19:58:00Z")),
        Gate::Open
    );
}

#[test]
fn a_repeated_hour_and_a_skipped_hour_add_no_attempts() {
    // Clocks in Los Angeles fall back from 02:00 to 01:00 on 2026-11-01.
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    let one_to_two = automatic(60, 120);
    let (first, repeat) = (pst("2026-11-01T08:30:00Z"), pst("2026-11-01T09:30:00Z"));
    assert_eq!((first.date, first.minute), (repeat.date, repeat.minute));
    assert_eq!(begin(&history, &one_to_two, &first), Gate::Open);
    assert_eq!(begin(&history, &one_to_two, &repeat), Gate::WindowUsed);

    // They spring forward from 02:00 to 03:00 on 2027-03-14, so that night has
    // no 02:00 to 03:00 at all.
    let two_to_three = automatic(120, 180);
    for utc in ["2027-03-14T09:59:59Z", "2027-03-14T10:00:00Z"] {
        assert_eq!(
            gate(&history, &two_to_three, &pst(utc)),
            Gate::OutsideWindow,
            "{utc}"
        );
    }
}

#[test]
fn a_corrected_clock_costs_one_window_at_most() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    // The attempt was saved while the clock ran a year ahead.
    assert_eq!(
        begin(&history, &night(), &pst("2027-11-10T11:30:00Z")),
        Gate::Open
    );
    // Outside the window the corrected clock changes nothing.
    let afternoon = pst("2026-11-09T23:00:00Z");
    assert_eq!(begin(&history, &night(), &afternoon), Gate::OutsideWindow);
    assert_eq!(
        history
            .status(Some(&night()), Some(&afternoon), IMAGE)
            .last_attempt
            .unwrap()
            .opened,
        date(2027, 11, 10)
    );
    // Inside it, the window counts as used and the attempt is restated.
    let corrected = pst("2026-11-10T11:30:00Z");
    assert_eq!(gate(&history, &night(), &corrected), Gate::WindowUsed);
    assert_eq!(begin(&history, &night(), &corrected), Gate::WindowUsed);
    assert_eq!(
        history
            .status(Some(&night()), Some(&corrected), IMAGE)
            .last_attempt,
        Some(Attempt {
            opened: date(2026, 11, 10),
            utc_s: corrected.utc_s,
        })
    );
    assert_eq!(
        begin(&history, &night(), &pst("2026-11-10T12:00:00Z")),
        Gate::WindowUsed
    );
    assert_eq!(
        begin(&history, &night(), &pst("2026-11-11T11:30:00Z")),
        Gate::Open
    );

    // A clock that jumps ahead opens at most the one window it lands in.
    assert_eq!(
        begin(&history, &night(), &pst("2028-01-05T11:30:00Z")),
        Gate::Open
    );
    assert_eq!(
        begin(&history, &night(), &pst("2028-01-05T12:30:00Z")),
        Gate::WindowUsed
    );
}

#[test]
fn a_moved_time_zone_cannot_repeat_the_night() {
    let temp = tempfile::tempdir().unwrap();
    let history = history(&temp);
    assert_eq!(
        begin(&history, &night(), &pst("2026-11-10T11:30:00Z")),
        Gate::Open
    );
    // Three in the morning arrives again five hours later in Honolulu, and
    // the next calendar day arrives early in Tokyo; neither is twelve hours on.
    let honolulu = at("2026-11-10T13:30:00Z", "Pacific/Honolulu");
    assert_eq!(
        (honolulu.date, honolulu.minute),
        (date(2026, 11, 10), 3 * 60 + 30)
    );
    assert_eq!(gate(&history, &night(), &honolulu), Gate::WindowUsed);
    let tokyo = at("2026-11-10T18:30:00Z", "Asia/Tokyo");
    assert_eq!(
        (tokyo.date, tokyo.minute),
        (date(2026, 11, 11), 3 * 60 + 30)
    );
    assert_eq!(gate(&history, &night(), &tokyo), Gate::WindowUsed);
    assert_eq!(
        gate(
            &history,
            &night(),
            &at("2026-11-11T18:30:00Z", "Asia/Tokyo")
        ),
        Gate::Open
    );
}

#[test]
fn an_image_installs_by_itself_at_most_once() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-admission");
    let history = History::open(&path);
    let inside = pst("2026-11-10T11:30:00Z");
    for bad in ["", "rootfs aaaa", "rootfs-\u{e9}", &"a".repeat(257)] {
        assert!(history.tried(bad).is_err(), "{bad:?}");
    }
    assert!(!path.exists());
    history.tried("rootfs-aaaa").unwrap();
    // The reason holds whatever the clock or the window say.
    for now in [Some(&inside), None] {
        let status = history.status(Some(&night()), now, IMAGE);
        assert_eq!(status.gate, Gate::ImageTried);
        assert_eq!(status.local.is_some(), now.is_some());
    }
    assert_eq!(
        history.begin(Some(&night()), Some(&inside), IMAGE).unwrap(),
        Gate::ImageTried
    );
    drop(history);
    let history = History::open(&path);
    assert_eq!(
        history.status(Some(&night()), Some(&inside), IMAGE),
        Status {
            gate: Gate::ImageTried,
            local: Some(Local {
                date: date(2026, 11, 10),
                minute: 3 * 60 + 30,
            }),
            last_attempt: None,
            tried_build_id: Some("rootfs-aaaa".into()),
        }
    );
    // Another image has its own attempt.
    assert_eq!(
        history
            .begin(Some(&night()), Some(&inside), Some("rootfs-bbbb"))
            .unwrap(),
        Gate::Open
    );
}

#[test]
fn unreadable_history_keeps_the_gate_shut() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("install-admission");
    let history = History::open(&path);
    let inside = pst("2026-11-10T11:30:00Z");
    assert_eq!(begin(&history, &night(), &inside), Gate::Open);
    drop(history);
    let state = path.join("state.json");
    let edited = fs::read_to_string(&state)
        .unwrap()
        .replace("2026-11-10", "2026-11-01");
    fs::write(&state, edited).unwrap();
    let history = History::open(&path);
    let next = pst("2026-11-11T11:30:00Z");
    assert_eq!(
        history.status(Some(&night()), Some(&next), IMAGE),
        Status {
            gate: Gate::HistoryUnavailable,
            local: Some(Local {
                date: date(2026, 11, 11),
                minute: 3 * 60 + 30,
            }),
            last_attempt: None,
            tried_build_id: None,
        }
    );
    assert!(history.begin(Some(&night()), Some(&next), IMAGE).is_err());
    assert!(history.tried("rootfs-aaaa").is_err());
}
