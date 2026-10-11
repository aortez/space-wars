//! Saved-profile inventory and narrowly scoped persistent edits.
use super::*;
use crate::network::SavedNetwork;

pub(super) fn describe(path: &str, settings: &Settings, connected: bool) -> Option<SavedNetwork> {
    let connection = settings.get("connection")?;
    let wifi = settings.get("802-11-wireless")?;
    let id = text(connection, "uuid");
    if text(connection, "type") != "802-11-wireless" || id.is_empty() {
        return None;
    }
    Some(SavedNetwork {
        id,
        path: path.into(),
        name: ssid_name(text(connection, "id").as_bytes()),
        ssid_name: ssid_name(&bytes(wifi, "ssid")),
        autoconnect: connection
            .get("autoconnect")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(true),
        priority: connection
            .get("autoconnect-priority")
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(0),
        connected,
        network: None,
    })
}

impl NetworkManager {
    pub(super) async fn active_profiles(&self) -> Result<Vec<String>, String> {
        let props = self.properties(ROOT, SERVICE).await?;
        let active = props
            .get("ActiveConnections")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| Vec::<OwnedObjectPath>::try_from(v).ok())
            .unwrap_or_default();
        if active.len() > 64 {
            return Err("Too many active connections to verify saved network state.".into());
        }
        let mut profiles = Vec::new();
        for path in active {
            let props = self
                .properties(path.as_str(), &format!("{SERVICE}.Connection.Active"))
                .await?;
            if number(&props, "State") != 4 {
                profiles.push(object(&props, "Connection"));
            }
        }
        Ok(profiles)
    }

    async fn profile_settings(&self, id: &str) -> Result<(String, u64, Settings), String> {
        let path: OwnedObjectPath = self
            .proxy(&format!("{ROOT}/Settings"), &format!("{SERVICE}.Settings"))
            .await?
            .call("GetConnectionByUuid", &(id,))
            .await
            .map_err(explain)?;
        // Read the version before settings. An edit during either read is
        // rejected by Update2 instead of overwriting another client's fields.
        let props = self.properties(path.as_str(), PROFILE).await?;
        let version = props
            .get("VersionId")
            .and_then(|v| u64::try_from(v).ok())
            .filter(|v| *v != 0)
            .ok_or("This NetworkManager cannot safely update saved profiles.")?;
        let settings: Settings = self
            .proxy(path.as_str(), PROFILE)
            .await?
            .call("GetSettings", &())
            .await
            .map_err(explain)?;
        let profile = describe(path.as_str(), &settings, false)
            .filter(|p| p.id == id)
            .ok_or("Saved network changed or was removed. Refresh and select it again.")?;
        if number(&props, "Flags") & 4 != 0 {
            return Err(
                "A temporary connection cannot be edited. Finish its connection trial first."
                    .into(),
            );
        }
        Ok((profile.path, version, settings))
    }

    async fn write_profile(
        &self,
        path: &str,
        version: u64,
        settings: Settings,
    ) -> Result<(), String> {
        // GetSettings omits all secrets. NM 1.46 merges existing secrets into a
        // secret-free update (update_auth_cb), so no password read is needed.
        // TO_DISK | NO_REAPPLY persists preferences without reapplying the
        // active device's unrelated zone/metered settings.
        let args: Properties = HashMap::from([("version-id".into(), OwnedValue::from(version))]);
        let _: Properties = self
            .proxy(path, PROFILE)
            .await?
            .call("Update2", &(settings, 0x41_u32, args))
            .await
            .map_err(explain)?;
        Ok(())
    }

    pub(super) async fn manage_profile(
        &self,
        id: &str,
        change: ProfileChange,
    ) -> Result<(), String> {
        let (path, version, mut settings) = self.profile_settings(id).await?;
        match change {
            ProfileChange::Forget { allow_active } => {
                if !allow_active && self.active_profiles().await?.contains(&path) {
                    return Err("This network became active. Review the connection warning and confirm Forget again.".into());
                }
                self.proxy(&path, PROFILE)
                    .await?
                    .call::<_, _, ()>("Delete", &())
                    .await
                    .map_err(explain)
            }
            ProfileChange::Autoconnect(enabled) => {
                settings
                    .get_mut("connection")
                    .unwrap()
                    .insert("autoconnect".into(), OwnedValue::from(enabled));
                self.write_profile(&path, version, settings).await
            }
            ProfileChange::Prefer => {
                let selected = describe(&path, &settings, false).unwrap();
                if !selected.autoconnect {
                    return Err(
                        "Enable Connect automatically before preferring this network.".into(),
                    );
                }
                let (saved, complete) = self.saved().await?;
                if !complete {
                    return Err("Some saved profiles could not be read. Refresh before choosing a preference.".into());
                }
                let alternatives: Vec<_> = saved
                    .iter()
                    .filter_map(|(path, settings)| describe(path, settings, false))
                    .filter(|p| p.id != id && p.autoconnect)
                    .collect();
                let priority = alternatives
                    .iter()
                    .map(|p| p.priority)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
                    .clamp(1, 999);
                settings
                    .get_mut("connection")
                    .unwrap()
                    .insert("autoconnect-priority".into(), OwnedValue::from(priority));
                self.write_profile(&path, version, settings).await?;
                // Usually one write suffices and all fallback priorities remain
                // untouched. Only ties at NM's maximum need to move down one.
                // There is no multi-profile transaction: on failure the worker
                // reports uncertainty and refreshes actual daemon state.
                for other in alternatives.iter().filter(|p| p.priority >= priority) {
                    let (path, version, mut settings) = self.profile_settings(&other.id).await?;
                    let current = describe(&path, &settings, false).unwrap();
                    if current.autoconnect && current.priority >= priority {
                        settings.get_mut("connection").unwrap().insert(
                            "autoconnect-priority".into(),
                            OwnedValue::from(priority - 1),
                        );
                        self.write_profile(&path, version, settings).await?;
                    }
                }
                Ok(())
            }
        }
    }
}
