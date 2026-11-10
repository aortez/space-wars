use super::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    generation: u64,
    text: String,
}
impl StateRecord for Note {
    fn initial() -> Self {
        Self {
            generation: 0,
            text: String::new(),
        }
    }
    fn validate(&self) -> Result<()> {
        if self.text.len() > 8 {
            return Err(Error::Invalid("note too long".into()));
        }
        Ok(())
    }
    fn generation(&self) -> u64 {
        self.generation
    }
}
impl Revised for Note {
    fn at(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }
}

fn open(path: &Path) -> Saved<Note> {
    Saved::open("note", path)
}
fn write(saved: &Saved<Note>, text: &str) -> Result<Note> {
    saved.change(|note| {
        Ok(Note {
            text: text.into(),
            ..note.clone()
        })
    })
}
fn note(generation: u64, text: &str) -> Note {
    Note {
        generation,
        text: text.into(),
    }
}

#[test]
fn a_record_that_never_changes_has_no_directory() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    let saved = open(&path);
    assert_eq!(saved.read(), Some(note(0, "")));
    // Writing what is already there is not a change, and a refused revision
    // is not one either.
    assert_eq!(write(&saved, "").unwrap(), note(0, ""));
    assert!(
        saved
            .change(|_| Err(Error::Invalid("refused".into())))
            .is_err()
    );
    assert_eq!(saved.read(), Some(note(0, "")));
    assert!(!path.exists());
}

#[test]
fn each_change_is_durable_and_advances_one_generation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    let saved = open(&path);
    assert_eq!(write(&saved, "a").unwrap(), note(1, "a"));
    for entry in [path.clone(), path.join("state.json")] {
        assert_eq!(fs::metadata(entry).unwrap().permissions().mode() & 0o077, 0);
    }
    let before = fs::read(path.join("state.json")).unwrap();
    assert_eq!(write(&saved, "a").unwrap(), note(1, "a"));
    assert_eq!(fs::read(path.join("state.json")).unwrap(), before);
    // A revision cannot choose its own generation.
    assert_eq!(saved.change(|_| Ok(note(40, "b"))).unwrap(), note(2, "b"));
    assert_eq!(saved.read(), Some(note(2, "b")));
    drop(saved);
    let reopened = open(&path);
    assert_eq!(reopened.read(), Some(note(2, "b")));
    assert_eq!(write(&reopened, "c").unwrap(), note(3, "c"));
}

#[test]
fn an_invalid_revision_is_refused_and_the_saved_record_stays() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    let saved = open(&path);
    assert!(write(&saved, "far too long").is_err());
    assert_eq!(saved.read(), Some(note(0, "")));
    assert_eq!(write(&saved, "a").unwrap(), note(1, "a"));
    assert!(write(&saved, "far too long").is_err());
    assert_eq!(saved.read(), Some(note(1, "a")));
    assert_eq!(write(&saved, "b").unwrap(), note(2, "b"));
}

#[test]
fn unreadable_state_reads_as_nothing_and_refuses_changes_until_it_reads_again() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    let saved = open(&path);
    write(&saved, "a").unwrap();
    drop(saved);
    let state = path.join("state.json");
    let good = fs::read(&state).unwrap();

    // A record edited outside its owner no longer matches its checksum.
    let edited = String::from_utf8(good.clone())
        .unwrap()
        .replace("\"a\"", "\"z\"");
    assert_ne!(edited.as_bytes(), good);
    fs::write(&state, edited).unwrap();
    let saved = open(&path);
    assert_eq!(saved.read(), None);
    assert!(write(&saved, "b").is_err());
    assert_eq!(saved.read(), None);

    // Once the state reads again, the next change proceeds from it.
    fs::write(&state, &good).unwrap();
    assert_eq!(write(&saved, "b").unwrap(), note(2, "b"));
    drop(saved);

    // A directory other accounts can enter is not trusted either.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let saved = open(&path);
    assert_eq!(saved.read(), None);
    assert!(write(&saved, "c").is_err());
}

#[test]
fn a_failed_save_leaves_whichever_record_is_durable() {
    for (point, kept) in [
        ("written", false),
        ("file-synced", false),
        ("renamed", true),
        ("directory-synced", true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let saved = open(&temp.path().join("note"));
        write(&saved, "a").unwrap();
        match &mut *saved.state.lock().unwrap() {
            State::Stored(store, _) => store.fail_at = Some(point),
            _ => panic!("a changed record has no store"),
        }
        assert!(write(&saved, "b").is_err(), "{point}");
        let durable = if kept { note(2, "b") } else { note(1, "a") };
        assert_eq!(saved.read(), Some(durable.clone()), "{point}");
        // The reload replaced the failed store, so later changes work again.
        assert_eq!(
            write(&saved, "c").unwrap(),
            note(durable.generation + 1, "c"),
            "{point}"
        );
    }
}

#[test]
fn a_reader_does_not_wait_for_a_save_in_progress() {
    let temp = tempfile::tempdir().unwrap();
    let saved = open(&temp.path().join("note"));
    // A change holds this lock while it writes and flushes.
    let saving = saved.state.lock().unwrap();
    let reader = &saved;
    let seen = std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::channel();
        scope.spawn(move || send.send(reader.read()));
        let seen = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(saving);
        seen
    });
    assert_eq!(seen, Ok(Some(note(0, ""))));
}

#[test]
fn a_second_owner_reads_nothing_and_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    let owner = open(&path);
    write(&owner, "a").unwrap();
    let other = open(&path);
    assert_eq!(other.read(), None);
    assert!(write(&other, "b").is_err());
    assert_eq!(owner.read(), Some(note(1, "a")));
}
