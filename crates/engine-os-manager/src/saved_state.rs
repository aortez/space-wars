//! A record that reaches storage only once it first changes. Readers get a copy
//! from memory; only a change touches storage.
use crate::schedule_store::{StateRecord, Store};
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) trait Revised: StateRecord {
    /// The same record at another generation.
    fn at(self, generation: u64) -> Self;
}

enum State<R: StateRecord> {
    /// Nothing was ever saved: the initial record applies and no directory
    /// exists.
    Initial,
    Stored(Store<R>, R),
    /// Existing state could not be read. The initial record is not assumed in
    /// its place, and a change is refused until it reads again.
    Unavailable,
}

/// The single owner of one lazily created record.
pub(crate) struct Saved<R: Revised> {
    what: &'static str,
    path: PathBuf,
    /// Held across a save, so changes apply one at a time.
    state: Mutex<State<R>>,
    /// What readers get. A save in progress never delays one.
    published: Mutex<Option<R>>,
}

impl<R: Revised> Saved<R> {
    /// Creates nothing: a unit that never changes the record has no directory.
    pub fn open(what: &'static str, path: &Path) -> Self {
        let state = load(what, path);
        Self {
            what,
            path: path.into(),
            published: Mutex::new(snapshot(&state)),
            state: Mutex::new(state),
        }
    }

    /// `None` while saved state is unreadable.
    pub fn read(&self) -> Option<R> {
        self.published.lock().ok()?.clone()
    }

    /// Revises the current record and returns the result once it is durable.
    /// A revision equal to the current record writes nothing. The generation
    /// is assigned here, not by `revise`.
    pub fn change(&self, revise: impl FnOnce(&R) -> Result<R>) -> Result<R> {
        let poisoned = || Error::Invalid(format!("{} lock poisoned", self.what));
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        let result = self.apply(&mut state, revise);
        *self.published.lock().map_err(|_| poisoned())? = snapshot(&state);
        result
    }

    fn apply(&self, state: &mut State<R>, revise: impl FnOnce(&R) -> Result<R>) -> Result<R> {
        if matches!(*state, State::Unavailable) {
            *state = load(self.what, &self.path);
        }
        let current = match &*state {
            State::Initial => R::initial(),
            State::Stored(_, record) => record.clone(),
            State::Unavailable => {
                return Err(Error::Invalid(format!("saved {} is unreadable", self.what)));
            }
        };
        let next = revise(&current)?;
        if next == current {
            return Ok(current);
        }
        let generation = current
            .generation()
            .checked_add(1)
            .ok_or_else(|| Error::Invalid(format!("{} generation exhausted", self.what)))?;
        let next = next.at(generation);
        let saved = match std::mem::replace(state, State::Unavailable) {
            State::Stored(store, _) => Ok(store),
            _ => Store::<R>::open(&self.path).and_then(|(store, found)| {
                if found == current {
                    Ok(store)
                } else {
                    Err(Error::Invalid(format!(
                        "{} appeared outside its owner",
                        self.what
                    )))
                }
            }),
        }
        .and_then(|mut store| store.save(&next).map(|()| store));
        match saved {
            Ok(store) => {
                *state = State::Stored(store, next.clone());
                Ok(next)
            }
            Err(error) => {
                // The failed store is already dropped and its lock released, so
                // this reads whichever record is durable.
                *state = load(self.what, &self.path);
                Err(error)
            }
        }
    }
}

fn load<R: StateRecord>(what: &str, path: &Path) -> State<R> {
    match Store::<R>::existing(path) {
        Ok(None) => State::Initial,
        Ok(Some((store, record))) => State::Stored(store, record),
        Err(error) => {
            tracing::warn!(%error, "saved {what} is unreadable");
            State::Unavailable
        }
    }
}

fn snapshot<R: StateRecord>(state: &State<R>) -> Option<R> {
    match state {
        State::Initial => Some(R::initial()),
        State::Stored(_, record) => Some(record.clone()),
        State::Unavailable => None,
    }
}

#[cfg(test)]
#[path = "saved_state_tests.rs"]
mod tests;
