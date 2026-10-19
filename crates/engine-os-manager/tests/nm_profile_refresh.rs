//! Opt-in qualification against a real, isolated NetworkManager daemon.
//! All profiles and credentials are synthetic; never use the host system bus.
#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Value};

type Settings = HashMap<String, HashMap<String, OwnedValue>>;
const NAME: &str = "org.freedesktop.NetworkManager";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const PROFILE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const SECURITY: &str = "802-11-wireless-security";

fn text(value: &str) -> OwnedValue {
    Str::from(value).into()
}

fn profile(agent_owned: bool) -> Settings {
    HashMap::from([
        (
            "connection".into(),
            HashMap::from([
                ("id".into(), text("spacewars-isolated-qualification")),
                ("uuid".into(), text("a47999bd-80bc-4e21-bd3b-127a18c74190")),
                ("type".into(), text("802-11-wireless")),
                ("autoconnect".into(), false.into()),
            ]),
        ),
        (
            "802-11-wireless".into(),
            HashMap::from([
                (
                    "ssid".into(),
                    Value::from(b"spacewars-isolated".as_slice())
                        .try_into()
                        .unwrap(),
                ),
                ("mode".into(), text("infrastructure")),
            ]),
        ),
        (
            SECURITY.into(),
            if agent_owned {
                HashMap::from([
                    ("key-mgmt".into(), text("wpa-psk")),
                    ("psk-flags".into(), 1u32.into()),
                ])
            } else {
                HashMap::from([
                    ("key-mgmt".into(), text("wpa-psk")),
                    ("psk".into(), text("synthetic-password-one")),
                ])
            },
        ),
    ])
}

#[derive(Default)]
struct AgentState {
    second: bool,
    calls: usize,
}

struct Agent(Arc<Mutex<AgentState>>);

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl Agent {
    fn get_secrets(
        &self,
        _connection: Settings,
        _path: OwnedObjectPath,
        setting: &str,
        _hints: Vec<String>,
        _flags: u32,
    ) -> Settings {
        assert_eq!(setting, SECURITY);
        let mut state = self.0.lock().unwrap();
        state.calls += 1;
        HashMap::from([(
            SECURITY.into(),
            HashMap::from([(
                "psk".into(),
                text(if state.second {
                    "synthetic-agent-two"
                } else {
                    "synthetic-agent-one"
                }),
            )]),
        )])
    }

    fn cancel_get_secrets(&self, _path: &str, _setting: &str) {}
    fn save_secrets(&self, _connection: Settings, _path: OwnedObjectPath) {}
    fn delete_secrets(&self, _connection: Settings, _path: OwnedObjectPath) {}
}

fn same_directory(left: &Path, right: &Path) {
    let left = fs::metadata(left).unwrap();
    let right = fs::metadata(right).unwrap();
    assert!(left.is_dir() && right.is_dir());
    assert_eq!((left.dev(), left.ino()), (right.dev(), right.ino()));
    assert_eq!(left.uid(), 0);
    assert_eq!(left.mode() & 0o777, 0o700);
}

async fn version(proxy: &zbus::Proxy<'_>) -> u64 {
    proxy.get_property("VersionId").await.unwrap()
}

async fn native_proxy<'a>(
    connection: &'a zbus::Connection,
    path: &str,
    interface: &str,
) -> zbus::Proxy<'a> {
    // Assertions must see the method's completed mutation, not a cached value
    // waiting for its PropertiesChanged signal to be dispatched.
    zbus::proxy::Builder::new(connection)
        .destination(NAME)
        .unwrap()
        .path(path.to_owned())
        .unwrap()
        .interface(interface.to_owned())
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn settings(proxy: &zbus::Proxy<'_>) -> Settings {
    proxy.call("GetSettings", &()).await.unwrap()
}

async fn secrets(proxy: &zbus::Proxy<'_>, expected: &str) {
    let values: Settings = proxy.call("GetSecrets", &(SECURITY,)).await.unwrap();
    assert_eq!(
        <&str>::try_from(&values[SECURITY]["psk"]).unwrap(),
        expected
    );
}

async fn update(proxy: &zbus::Proxy<'_>, method: &str, value: Settings) {
    let before = version(proxy).await;
    let _: () = proxy.call(method, &(value,)).await.unwrap();
    assert!(
        version(proxy).await > before,
        "{method} must change revision"
    );
}

#[test]
#[ignore = "owner-only: requires an isolated NetworkManager, bus, network and mount namespaces"]
fn real_daemon_preserves_only_identical_internal_secret_refreshes() {
    assert_eq!(unsafe { libc::geteuid() }, 0);
    let directory = std::env::var("SPACEWARS_NM_QUALIFICATION_ROOT").unwrap();
    let directory = Path::new(&directory);
    let root = directory.parent().unwrap();
    assert_eq!(root.parent(), Some(Path::new("/data")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("spacewars-nm-qualification-")
    );
    assert_eq!(directory.canonicalize().unwrap(), directory);
    for namespace in ["net", "mnt"] {
        assert_ne!(
            fs::read_link(format!("/proc/self/ns/{namespace}")).unwrap(),
            fs::read_link(format!("/proc/1/ns/{namespace}")).unwrap(),
        );
    }
    for (local, bound) in [
        ("etc", "/etc/NetworkManager"),
        ("run", "/run/NetworkManager"),
        ("state", "/var/lib/NetworkManager"),
    ] {
        same_directory(&directory.join(local), Path::new(bound));
    }
    assert_eq!(fs::read_dir("/sys/class/net").unwrap().count(), 1);
    assert!(Path::new("/sys/class/net/lo").exists());
    let address = format!("unix:path={}/bus", directory.display());
    assert_eq!(std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(), address);

    futures_lite::future::block_on(async {
        let connection = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(5))
            .build()
            .await
            .unwrap();
        let inventory = native_proxy(
            &connection,
            "/org/freedesktop/NetworkManager/Settings",
            SETTINGS,
        )
        .await;
        let original: Vec<OwnedObjectPath> = inventory.get_property("Connections").await.unwrap();
        assert!(original.is_empty(), "fixture must begin without profiles");
        let path: OwnedObjectPath = inventory
            .call("AddConnection", &(profile(false),))
            .await
            .unwrap();
        let proxy = native_proxy(&connection, path.as_str(), PROFILE).await;
        let visible = settings(&proxy).await;
        assert!(!visible[SECURITY].contains_key("psk"));
        let before = version(&proxy).await;
        for _ in 0..3 {
            secrets(&proxy, "synthetic-password-one").await;
            assert_eq!(settings(&proxy).await, visible);
            assert_eq!(
                version(&proxy).await,
                before,
                "unchanged stored secret refresh changed VersionId"
            );
        }
        println!("Passed repeated identical stored-secret refreshes");

        // Explicit no-op updates retain their optimistic-concurrency semantics.
        update(&proxy, "Update", profile(false)).await;
        let stale: zbus::Result<HashMap<String, OwnedValue>> = proxy
            .call(
                "Update2",
                &(
                    Settings::new(),
                    1u32,
                    HashMap::from([("version-id", OwnedValue::from(before))]),
                ),
            )
            .await;
        let zbus::Error::MethodError(name, _, _) = stale.unwrap_err() else {
            panic!("expected a native version-conflict error");
        };
        assert_eq!(
            name.as_str(),
            "org.freedesktop.NetworkManager.Settings.VersionIdMismatch"
        );
        println!("Passed explicit no-op update and stale-version rejection");

        let visible = settings(&proxy).await;
        let mut changed = profile(false);
        changed
            .get_mut(SECURITY)
            .unwrap()
            .insert("psk".into(), text("synthetic-password-two"));
        update(&proxy, "Update", changed).await;
        assert_eq!(
            settings(&proxy).await,
            visible,
            "password-only control must have equal visible settings"
        );
        let before = version(&proxy).await;
        secrets(&proxy, "synthetic-password-two").await;
        assert_eq!(version(&proxy).await, before);
        let _: () = proxy.call("ClearSecrets", &()).await.unwrap();
        assert!(version(&proxy).await > before);
        println!("Passed password-only edit and ClearSecrets");

        update(&proxy, "Update", profile(false)).await;
        update(&proxy, "UpdateUnsaved", profile(false)).await;
        assert!(proxy.get_property::<bool>("Unsaved").await.unwrap());
        let before = version(&proxy).await;
        let _: () = proxy.call("Save", &()).await.unwrap();
        assert!(version(&proxy).await > before);
        assert!(!proxy.get_property::<bool>("Unsaved").await.unwrap());
        println!("Passed unsaved update and storage/flags transition");

        let state = Arc::new(Mutex::new(AgentState::default()));
        let agent_connection = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(5))
            .serve_at(
                "/org/freedesktop/NetworkManager/SecretAgent",
                Agent(state.clone()),
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let manager = native_proxy(
            &agent_connection,
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await;
        let _: () = manager
            .call("Register", &("spacewars.qualification",))
            .await
            .unwrap();
        update(&proxy, "Update", profile(true)).await;
        let before = version(&proxy).await;
        secrets(&proxy, "synthetic-agent-one").await;
        assert!(
            version(&proxy).await > before,
            "new agent credential must advance revision"
        );
        let before = version(&proxy).await;
        secrets(&proxy, "synthetic-agent-one").await;
        assert_eq!(version(&proxy).await, before);
        let calls = state.lock().unwrap().calls;
        state.lock().unwrap().second = true;
        secrets(&proxy, "synthetic-agent-two").await;
        assert!(
            state.lock().unwrap().calls > calls,
            "changed credential must come from the real agent callback"
        );
        assert!(
            version(&proxy).await > before,
            "changed agent credential must advance revision"
        );
        let before = version(&proxy).await;
        secrets(&proxy, "synthetic-agent-two").await;
        assert_eq!(version(&proxy).await, before);
        println!("Passed changed and identical agent-secret refreshes");

        let _: () = proxy.call("Delete", &()).await.unwrap();
        let remaining: Vec<OwnedObjectPath> = inventory.get_property("Connections").await.unwrap();
        assert!(remaining.is_empty());
        println!("Passed profile deletion; all native refresh controls passed");
    });
}
