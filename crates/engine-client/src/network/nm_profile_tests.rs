use super::*;
use std::collections::BTreeMap;

const FIRST: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const SECOND: &str = "bbbbbbbb-0000-0000-0000-000000000002";
const OFFLINE: &str = "cccccccc-0000-0000-0000-000000000003";

struct Entry {
    settings: Settings,
    version: u64,
    secret: String,
}
struct Store {
    entries: BTreeMap<String, Entry>,
    active: Option<String>,
    radio: bool,
    denied: Option<String>,
    race: Option<String>,
    calls: Vec<String>,
}
type Shared = Arc<Mutex<Store>>;

fn clone_settings(settings: &Settings) -> Settings {
    settings
        .iter()
        .map(|(section, values)| {
            (
                section.clone(),
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.try_clone().unwrap()))
                    .collect(),
            )
        })
        .collect()
}
fn profile_path(id: &str) -> String {
    format!("/profiles/{}", id.replace('-', "_"))
}
fn entry(id: &str, ssid: &str, auto: bool, priority: i32) -> Entry {
    let mut network = super::super::super::tests::network();
    network.ssid = ssid.as_bytes().to_vec();
    let mut settings: Settings = connection_settings(&network, None)
        .into_iter()
        .map(|(section, values)| {
            (
                section.into(),
                values
                    .into_iter()
                    .map(|(key, value)| (key.into(), OwnedValue::try_from(value).unwrap()))
                    .collect(),
            )
        })
        .collect();
    settings
        .get_mut("802-11-wireless-security")
        .unwrap()
        .remove("psk");
    settings.get_mut("connection").unwrap().extend([
        ("uuid".into(), Value::from(id).try_into().unwrap()),
        (
            "id".into(),
            Value::from("Duplicate profile name").try_into().unwrap(),
        ),
        ("autoconnect".into(), OwnedValue::from(auto)),
        ("autoconnect-priority".into(), OwnedValue::from(priority)),
        ("metered".into(), OwnedValue::from(2_u32)),
        ("zone".into(), Value::from("trusted").try_into().unwrap()),
    ]);
    settings
        .get_mut("ipv4")
        .unwrap()
        .insert("route-metric".into(), OwnedValue::from(321_i64));
    Entry {
        settings,
        version: 1,
        secret: "private-not-returned".into(),
    }
}

struct ProfileManager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl ProfileManager {
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.0.lock().unwrap().radio
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().active.is_some() {
            vec![path(ACTIVE).unwrap()]
        } else {
            vec![]
        }
    }
    fn get_permissions(&self) -> HashMap<String, String> {
        [
            "network-control",
            "settings.modify.system",
            "checkpoint-rollback",
            "wifi.scan",
        ]
        .into_iter()
        .map(|p| (format!("{SERVICE}.{p}"), "yes".into()))
        .collect()
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI).unwrap()]
    }
    fn activate_connection(
        &self,
        profile: OwnedObjectPath,
        device: OwnedObjectPath,
        ap: OwnedObjectPath,
    ) -> OwnedObjectPath {
        assert_eq!(device.as_str(), WIFI);
        assert_eq!(ap.as_str(), "/ap");
        self.0
            .lock()
            .unwrap()
            .calls
            .push(format!("activate:{profile}"));
        path(ACTIVE).unwrap()
    }
}
struct SavedProfiles(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl SavedProfiles {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        self.0
            .lock()
            .unwrap()
            .entries
            .keys()
            .map(|id| path(&profile_path(id)).unwrap())
            .collect()
    }
    fn get_connection_by_uuid(&self, id: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        if !self.0.lock().unwrap().entries.contains_key(id) {
            return Err(zbus::fdo::Error::InvalidArgs("missing profile".into()));
        }
        Ok(path(&profile_path(id)).unwrap())
    }
}
struct SavedProfile {
    store: Shared,
    id: String,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl SavedProfile {
    #[zbus(property)]
    fn version_id(&self) -> u64 {
        self.store
            .lock()
            .unwrap()
            .entries
            .get(&self.id)
            .map_or(1, |e| e.version)
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        0
    }
    fn get_settings(&self) -> zbus::fdo::Result<Settings> {
        let mut store = self.store.lock().unwrap();
        let race = store.race.as_deref() == Some(&self.id);
        let entry = store
            .entries
            .get_mut(&self.id)
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("removed".into()))?;
        if race {
            entry.version += 1;
        }
        Ok(clone_settings(&entry.settings))
    }
    fn update2(
        &self,
        settings: Settings,
        flags: u32,
        args: Properties,
    ) -> zbus::fdo::Result<Properties> {
        let mut store = self.store.lock().unwrap();
        store.calls.push(format!("update:{}", self.id));
        if store.denied.as_deref() == Some(&self.id) {
            return Err(zbus::fdo::Error::AccessDenied(
                "private-not-returned".into(),
            ));
        }
        let entry = store.entries.get_mut(&self.id).unwrap();
        let version = u64::try_from(&args["version-id"]).unwrap();
        if version != entry.version {
            return Err(zbus::fdo::Error::InvalidArgs("version mismatch".into()));
        }
        assert_eq!(
            flags, 0x41,
            "persistent, without reapplying active settings"
        );
        assert_eq!(text(&settings["connection"], "uuid"), self.id);
        assert!(!settings["802-11-wireless-security"].contains_key("psk"));
        assert_eq!(text(&settings["connection"], "zone"), "trusted");
        assert_eq!(number(&settings["connection"], "metered"), 2);
        assert_eq!(
            i64::try_from(&settings["ipv4"]["route-metric"]).unwrap(),
            321
        );
        entry.settings = settings;
        entry.version += 1;
        Ok(Properties::new())
    }
    fn delete(&self) -> zbus::fdo::Result<()> {
        let mut store = self.store.lock().unwrap();
        store.calls.push(format!("delete:{}", self.id));
        if store.denied.as_deref() == Some(&self.id) {
            return Err(zbus::fdo::Error::AccessDenied(
                "private-not-returned".into(),
            ));
        }
        store.entries.remove(&self.id);
        if store.active.as_deref() == Some(&self.id) {
            store.active = None;
        }
        Ok(())
    }
}
struct ActiveProfile(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveProfile {
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        path(
            &self
                .0
                .lock()
                .unwrap()
                .active
                .as_deref()
                .map(profile_path)
                .unwrap_or("/".into()),
        )
        .unwrap()
    }
}
struct VisibleWifi;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl VisibleWifi {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path("/ap").unwrap()
    }
    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        vec![path("/ap").unwrap()]
    }
}

fn on_bus(test: impl AsyncFnOnce(NetworkManager, Shared)) {
    let mut bus = Bus(Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap());
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    async_io::block_on(async {
        bounded(Duration::from_secs(15), async {
            let store = Arc::new(Mutex::new(Store {
                entries: BTreeMap::from([
                    (FIRST.into(), entry(FIRST, "Test network", true, 0)),
                    (SECOND.into(), entry(SECOND, "Test network", false, 0)),
                    (OFFLINE.into(), entry(OFFLINE, "Distant network", true, 20)),
                ]),
                active: Some(FIRST.into()),
                radio: true,
                denied: None,
                race: None,
                calls: vec![],
            }));
            let mut builder = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .name(SERVICE)
                .unwrap()
                .serve_at(ROOT, ProfileManager(store.clone()))
                .unwrap()
                .serve_at(format!("{ROOT}/Settings"), SavedProfiles(store.clone()))
                .unwrap()
                .serve_at(ACTIVE, ActiveProfile(store.clone()))
                .unwrap()
                .serve_at(WIFI, Device)
                .unwrap()
                .serve_at(WIFI, VisibleWifi)
                .unwrap()
                .serve_at(
                    "/ap",
                    AccessPoint {
                        ssid: b"Test network".to_vec(),
                        flags: 1,
                        rsn: 0x100,
                        strength: 80,
                    },
                )
                .unwrap();
            for id in [FIRST, SECOND, OFFLINE] {
                builder = builder
                    .serve_at(
                        profile_path(id),
                        SavedProfile {
                            store: store.clone(),
                            id: id.into(),
                        },
                    )
                    .unwrap();
            }
            let _server = builder.build().await.unwrap();
            let connection = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap();
            test(NetworkManager { connection }, store).await;
            Ok(())
        })
        .await
        .unwrap();
    });
}

#[test]
fn saved_profiles_include_offline_and_duplicate_names_even_with_radio_disabled() {
    on_bus(async |backend, store| {
        let inventory = backend.inventory().await.unwrap();
        assert_eq!(inventory.profiles.len(), 3);
        let first = inventory.profiles.iter().find(|p| p.id == FIRST).unwrap();
        let second = inventory.profiles.iter().find(|p| p.id == SECOND).unwrap();
        let offline = inventory.profiles.iter().find(|p| p.id == OFFLINE).unwrap();
        assert!(first.connected);
        assert_eq!(first.name, second.name);
        assert_eq!(first.ssid_name, second.ssid_name);
        assert!(!second.connected && !second.autoconnect);
        assert_eq!(
            second.network.as_ref().unwrap().saved.as_deref(),
            Some(profile_path(SECOND).as_str())
        );
        assert!(offline.network.is_none());
        assert!(inventory.preferred(offline));
        assert!(!format!("{inventory:?}").contains("private-not-returned"));
        // A manual connection keeps the chosen duplicate, even with autoconnect off.
        backend
            .activate(
                second.network.as_ref().unwrap(),
                None,
                &mut Trial::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            store.lock().unwrap().calls,
            [format!("activate:{}", profile_path(SECOND))]
        );
        store.lock().unwrap().radio = false;
        let inventory = backend.inventory().await.unwrap();
        assert!(inventory.can_manage && !inventory.can_connect && !inventory.can_scan);
        assert_eq!(inventory.profiles.len(), 3);
        assert!(inventory.profiles.iter().all(|p| p.network.is_none()));
    });
}

#[test]
fn saved_preferences_preserve_credentials_and_other_settings_without_activation() {
    on_bus(async |backend, store| {
        assert!(backend.manage(SECOND, ProfileChange::Prefer).await.is_err());
        backend
            .manage(SECOND, ProfileChange::Autoconnect(true))
            .await
            .unwrap();
        backend.manage(SECOND, ProfileChange::Prefer).await.unwrap();
        let inventory = backend.inventory().await.unwrap();
        assert!(inventory.preferred(inventory.profiles.iter().find(|p| p.id == SECOND).unwrap()));
        assert_eq!(
            inventory
                .profiles
                .iter()
                .find(|p| p.id == OFFLINE)
                .unwrap()
                .priority,
            20
        );
        assert!(
            inventory
                .profiles
                .iter()
                .find(|p| p.id == FIRST)
                .unwrap()
                .connected
        );
        backend
            .manage(SECOND, ProfileChange::Autoconnect(false))
            .await
            .unwrap();
        let inventory = backend.inventory().await.unwrap();
        assert!(inventory.preferred(inventory.profiles.iter().find(|p| p.id == OFFLINE).unwrap()));
        assert!(
            !inventory
                .profiles
                .iter()
                .find(|p| p.id == SECOND)
                .unwrap()
                .autoconnect
        );
        let store = store.lock().unwrap();
        assert_eq!(store.active.as_deref(), Some(FIRST));
        assert!(store.calls.iter().all(|c| c == &format!("update:{SECOND}")));
        assert!(
            store
                .entries
                .values()
                .all(|p| p.secret == "private-not-returned")
        );
    });
}

#[test]
fn inaccessible_profiles_disable_edits_without_hiding_other_saved_networks() {
    on_bus(async |backend, store| {
        // Listed by NM, but its D-Bus object has disappeared before GetSettings.
        let missing = "dddddddd-0000-0000-0000-000000000004";
        store
            .lock()
            .unwrap()
            .entries
            .insert(missing.into(), entry(missing, "Gone", true, 999));
        let inventory = backend.inventory().await.unwrap();
        assert_eq!(inventory.profiles.len(), 3);
        assert!(!inventory.can_manage);
        assert!(inventory.summary.contains("could not be read"));
        assert!(backend.manage(FIRST, ProfileChange::Prefer).await.is_err());
        assert!(store.lock().unwrap().calls.is_empty());
    });
}

#[test]
fn forget_is_exact_and_requires_active_warning_and_handles_removed_profiles() {
    on_bus(async |backend, store| {
        assert!(
            backend
                .manage(
                    FIRST,
                    ProfileChange::Forget {
                        allow_active: false
                    }
                )
                .await
                .is_err()
        );
        assert!(store.lock().unwrap().calls.is_empty());
        backend
            .manage(
                SECOND,
                ProfileChange::Forget {
                    allow_active: false,
                },
            )
            .await
            .unwrap();
        assert!(store.lock().unwrap().entries.contains_key(FIRST));
        assert!(store.lock().unwrap().entries.contains_key(OFFLINE));
        assert!(!store.lock().unwrap().entries.contains_key(SECOND));
        assert!(
            backend
                .manage(SECOND, ProfileChange::Autoconnect(true))
                .await
                .is_err()
        );
        backend
            .manage(FIRST, ProfileChange::Forget { allow_active: true })
            .await
            .unwrap();
        assert_eq!(store.lock().unwrap().entries.len(), 1);
    });
}

#[test]
fn profile_updates_reject_concurrent_changes_and_report_permission_errors_without_secrets() {
    on_bus(async |backend, store| {
        store.lock().unwrap().race = Some(SECOND.into());
        assert!(
            backend
                .manage(SECOND, ProfileChange::Autoconnect(true))
                .await
                .is_err()
        );
        assert!(!boolean(
            &store.lock().unwrap().entries[SECOND].settings["connection"],
            "autoconnect"
        ));
        store.lock().unwrap().race = None;
        store.lock().unwrap().denied = Some(SECOND.into());
        let error = backend
            .manage(SECOND, ProfileChange::Autoconnect(true))
            .await
            .unwrap_err();
        assert!(error.contains("not permitted"));
        assert!(!error.contains("private-not-returned"));
        assert!(
            backend
                .manage(
                    SECOND,
                    ProfileChange::Forget {
                        allow_active: false
                    }
                )
                .await
                .is_err()
        );
        assert!(store.lock().unwrap().entries.contains_key(SECOND));
    });
}

#[test]
fn preference_ceiling_preserves_fallbacks_and_partial_failure_is_visible() {
    on_bus(async |backend, store| {
        for id in [FIRST, OFFLINE] {
            store
                .lock()
                .unwrap()
                .entries
                .get_mut(id)
                .unwrap()
                .settings
                .get_mut("connection")
                .unwrap()
                .insert("autoconnect-priority".into(), OwnedValue::from(999_i32));
        }
        backend
            .manage(SECOND, ProfileChange::Autoconnect(true))
            .await
            .unwrap();
        store.lock().unwrap().denied = Some(OFFLINE.into());
        assert!(backend.manage(SECOND, ProfileChange::Prefer).await.is_err());
        let inventory = backend.inventory().await.unwrap();
        assert!(!inventory.preferred(inventory.profiles.iter().find(|p| p.id == SECOND).unwrap()));
        store.lock().unwrap().denied = None;
        backend.manage(SECOND, ProfileChange::Prefer).await.unwrap();
        let inventory = backend.inventory().await.unwrap();
        assert!(inventory.preferred(inventory.profiles.iter().find(|p| p.id == SECOND).unwrap()));
        assert!(inventory.profiles.iter().all(|p| p.autoconnect));
        assert_eq!(store.lock().unwrap().active.as_deref(), Some(FIRST));
    });
}
