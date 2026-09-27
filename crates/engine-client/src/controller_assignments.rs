//! Player slots are independent of button profiles. Live IDs route inputs;
//! model keys reserve/recover distinct devices without persisting backend IDs.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Slot {
    key: String,
    connected_id: Option<usize>,
}

#[derive(Default, Debug)]
pub(crate) struct Assignments {
    slots: [Option<Slot>; 2],
    connected: BTreeMap<usize, String>,
    // Once two identical models have been seen, reconnect order is not identity.
    // Keep live assignments, but require explicit reassignment after disconnect.
    ambiguous: BTreeSet<String>,
}

impl Assignments {
    pub fn new(preferences: [Option<String>; 2]) -> Self {
        let mut result = Self::default();
        for (seat, key) in preferences.into_iter().enumerate() {
            if let Some(key) = key.filter(|key| key.starts_with("gilrs-v1:") && key.len() <= 512) {
                result.slots[seat] = Some(Slot {
                    key,
                    connected_id: None,
                });
            }
        }
        // Hand-edited duplicate preferences cannot identify two distinct pads.
        if matches!(&result.slots, [Some(a), Some(b)] if a.key == b.key) {
            result.slots = [None, None];
        }
        result
    }

    /// Register a startup batch before matching preferences, so the first of
    /// two identical pads is not mistaken for an unambiguous saved device.
    pub fn connect_many(&mut self, devices: impl IntoIterator<Item = (usize, String)>) {
        let devices = devices.into_iter().collect::<Vec<_>>();
        for (id, key) in &devices {
            self.register(*id, key.clone());
        }
        for (id, _) in devices {
            self.place(id);
        }
    }

    pub fn connect(&mut self, id: usize, key: String) {
        self.register(id, key);
        self.place(id);
    }

    fn register(&mut self, id: usize, key: String) {
        // Backends may recycle a connection ID for a different device.
        if self.connected.get(&id).is_some_and(|old| old != &key) {
            self.disconnect(id);
        }
        self.connected.insert(id, key.clone());
        if self
            .connected
            .values()
            .filter(|value| *value == &key)
            .count()
            > 1
        {
            self.ambiguous.insert(key);
        }
    }

    fn place(&mut self, id: usize) {
        if self.seat(id).is_some() {
            return;
        }
        let key = &self.connected[&id];
        if let Some(slot) = self
            .slots
            .iter_mut()
            .flatten()
            .find(|slot| &slot.key == key && slot.connected_id.is_none())
        {
            if !self.ambiguous.contains(key) {
                slot.connected_id = Some(id);
            }
            return;
        }
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(Slot {
                key: key.clone(),
                connected_id: Some(id),
            });
        }
    }

    pub fn disconnect(&mut self, id: usize) -> Option<usize> {
        let seat = self.seat(id);
        self.connected.remove(&id);
        if let Some(seat) = seat {
            self.slots[seat].as_mut().unwrap().connected_id = None;
        }
        seat
    }

    pub fn seat(&self, id: usize) -> Option<usize> {
        self.connected
            .contains_key(&id)
            .then(|| {
                self.slots.iter().position(|slot| {
                    slot.as_ref()
                        .is_some_and(|slot| slot.connected_id == Some(id))
                })
            })
            .flatten()
    }

    pub fn connected_id(&self, seat: usize) -> Option<usize> {
        self.slots.get(seat)?.as_ref()?.connected_id
    }

    pub fn reserved(&self, seat: usize) -> bool {
        self.slots.get(seat).is_some_and(Option::is_some)
    }

    pub fn has_disconnected_seat(&self) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|slot| slot.connected_id.is_none())
    }

    /// Explicit choices may replace a reservation. Moving an already seated
    /// controller swaps slots; an unassigned replacement leaves the old pad
    /// unassigned rather than quietly moving it into someone else's player.
    pub fn assign(&mut self, id: usize, seat: usize) -> bool {
        let Some(key) = self.connected.get(&id) else {
            return false;
        };
        if seat >= self.slots.len() {
            return false;
        }
        let previous = self.seat(id);
        if previous != Some(seat) {
            let displaced = self.slots[seat].replace(Slot {
                key: key.clone(),
                connected_id: Some(id),
            });
            if let Some(previous) = previous {
                self.slots[previous] = displaced;
            }
        }
        true
    }

    /// Saved preferences are deliberately model-level. Do not pretend that
    /// duplicate UUIDs or connection order identify individual controllers.
    pub fn preferences(&self) -> [Option<String>; 2] {
        std::array::from_fn(|seat| {
            self.slots[seat]
                .as_ref()
                .filter(|slot| !self.ambiguous.contains(&slot.key))
                .map(|slot| slot.key.clone())
        })
    }

    pub fn model_is_ambiguous(&self, id: usize) -> bool {
        self.connected
            .get(&id)
            .is_some_and(|key| self.ambiguous.contains(key))
    }

    pub fn reset(&mut self) {
        self.slots = [None, None];
        let devices = self.connected.keys().copied().collect::<Vec<_>>();
        for id in devices {
            self.place(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> String {
        format!("gilrs-v1:test:{name}")
    }

    #[test]
    fn connection_order_swaps_and_unassigned_replacements_remain_independent() {
        let mut seats = Assignments::default();
        seats.connect(41, key("cabinet"));
        seats.connect(12, key("usb"));
        seats.connect(99, key("extra"));
        assert_eq!(
            [seats.seat(41), seats.seat(12), seats.seat(99)],
            [Some(0), Some(1), None]
        );
        assert!(seats.assign(12, 0));
        assert_eq!([seats.seat(41), seats.seat(12)], [Some(1), Some(0)]);
        assert!(seats.assign(99, 0));
        assert_eq!(
            [seats.seat(41), seats.seat(12), seats.seat(99)],
            [Some(1), None, Some(0)]
        );
        assert!(!seats.assign(800, 0));
        assert!(!seats.assign(99, 2));
    }

    #[test]
    fn saved_preferences_ignore_connection_order_and_reserve_missing_devices() {
        let prefs = [Some(key("usb")), Some(key("cabinet"))];
        let mut seats = Assignments::new(prefs.clone());
        seats.connect(1, key("cabinet"));
        seats.connect(2, key("extra"));
        assert_eq!(seats.seat(1), Some(1));
        assert_eq!(seats.seat(2), None);
        assert!(seats.has_disconnected_seat());
        seats.connect(3, key("usb"));
        assert_eq!(seats.seat(3), Some(0));
        assert_eq!(seats.preferences(), prefs);
        assert!(!seats.has_disconnected_seat());
    }

    #[test]
    fn reconnect_with_a_new_id_reclaims_its_slot_but_id_reuse_cannot_steal_it() {
        let mut seats = Assignments::default();
        seats.connect(41, key("cabinet"));
        seats.connect(12, key("usb"));
        seats.assign(12, 0);
        assert_eq!(seats.disconnect(12), Some(0));
        seats.connect(12, key("different"));
        assert_eq!(seats.seat(12), None);
        seats.connect(99, key("usb"));
        assert_eq!(seats.seat(99), Some(0));
        assert_eq!(seats.seat(41), Some(1));
        // Even an ID-reuse event without a preceding disconnect is safe.
        seats.connect(99, key("another"));
        assert_eq!(seats.seat(99), None);
        assert!(seats.reserved(0));
    }

    #[test]
    fn startup_duplicates_do_not_guess_a_saved_assignment() {
        let mut seats = Assignments::new([Some(key("same")), None]);
        seats.connect_many([(1, key("same")), (2, key("same"))]);
        assert_eq!([seats.seat(1), seats.seat(2)], [None, None]);
        assert!(seats.assign(2, 0));
        assert!(seats.assign(1, 1));
        assert_eq!(seats.preferences(), [None, None]);
        assert_eq!([seats.seat(1), seats.seat(2)], [Some(1), Some(0)]);
    }

    #[test]
    fn duplicate_models_keep_live_slots_but_reconnect_requires_an_explicit_choice() {
        let mut seats = Assignments::default();
        seats.connect_many([(1, key("same")), (2, key("same"))]);
        seats.assign(2, 0);
        seats.disconnect(1);
        seats.disconnect(2);
        seats.connect(2, key("same"));
        seats.connect(1, key("same"));
        assert_eq!([seats.seat(1), seats.seat(2)], [None, None]);
        assert!(seats.assign(2, 0));
        assert_eq!(seats.seat(2), Some(0));
    }

    #[test]
    fn explicit_replacement_releases_missing_reservation_and_reset_is_recoverable() {
        let mut seats = Assignments::new([Some(key("missing")), None]);
        seats.connect(1, key("cabinet"));
        assert_eq!(seats.seat(1), Some(1));
        seats.assign(1, 0);
        assert_eq!(seats.seat(1), Some(0));
        assert!(seats.has_disconnected_seat());
        seats.reset();
        assert!(!seats.has_disconnected_seat());
        assert_eq!(seats.seat(1), Some(0));
        assert!(!seats.reserved(1));
    }

    #[test]
    fn invalid_or_duplicate_saved_keys_fall_back_to_automatic_slots() {
        for preferences in [
            [Some("invalid".into()), None],
            [Some(key("same")), Some(key("same"))],
        ] {
            let mut seats = Assignments::new(preferences);
            seats.connect(9, key("usb"));
            assert_eq!(seats.seat(9), Some(0));
        }
    }
}
