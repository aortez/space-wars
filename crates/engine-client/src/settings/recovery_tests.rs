use super::*;
use engine_common::{ControllerBinding, ControllerControl, ControllerProfile, ControllerSource};

fn profile(key: &str) -> ControllerProfile {
    ControllerProfile {
        device_key: format!("gilrs-v1:linux:{key}"),
        name: key.into(),
        bindings: ControllerControl::ALL
            .into_iter()
            .enumerate()
            .map(|(code, control)| ControllerBinding {
                control,
                source: ControllerSource::Button { code: code as u32 },
            })
            .collect(),
    }
}

fn read_table(path: &Path) -> toml::Table {
    toml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn missing_fields_and_sections_use_declared_defaults_without_resetting_siblings() {
    fn paths(value: &toml::Value, prefix: Vec<String>, out: &mut Vec<Vec<String>>) {
        if let toml::Value::Table(table) = value {
            for (key, value) in table {
                let mut child = prefix.clone();
                child.push(key.clone());
                out.push(child.clone());
                paths(value, child, out);
            }
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let defaults = Settings::default();
    let mut choices = Settings::default();
    choices.audio.master_volume = 0.05;
    choices.video.height = 900;
    choices.clock.show_date = true;
    choices.spacewars.player_health_percent = 200;
    choices.controls.player_1_device = Some("gilrs-v1:linux:usb".into());
    choices.controls.player_2_device = Some("gilrs-v1:linux:picade".into());
    let original = toml::Value::try_from(&choices).unwrap();
    let default_json = serde_json::to_value(defaults).unwrap();
    let mut all_paths = Vec::new();
    paths(&original, vec![], &mut all_paths);
    for parts in all_paths {
        let mut input = original.clone();
        let (key, parents) = parts.split_last().unwrap();
        let mut table = &mut input;
        for parent in parents {
            table = &mut table[parent];
        }
        table.as_table_mut().unwrap().remove(key);
        let loaded =
            load_settings_from_bytes(&path, toml::to_string(&input).unwrap().as_bytes()).unwrap();
        let mut expected = serde_json::to_value(&choices).unwrap();
        let mut field = &mut expected;
        let mut default = &default_json;
        for part in &parts {
            field = &mut field[part];
            default = &default[part];
        }
        *field = default.clone();
        assert_eq!(
            serde_json::to_value(loaded.settings).unwrap(),
            expected,
            "{}",
            parts.join(".")
        );
    }
}

#[test]
fn player_preferences_recover_individually_and_clear_without_erasing_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let mut settings = Settings::default();
    settings
        .controls
        .controller_profiles
        .push(profile("picade"));
    settings.controls.player_1_device = Some("gilrs-v1:linux:usb".into());
    settings.controls.player_2_device = Some("gilrs-v1:linux:picade".into());
    let mut table = toml::Value::try_from(settings).unwrap();
    table["controls"]["player_1_device"] = toml::Value::Integer(7);
    table["controls"]
        .as_table_mut()
        .unwrap()
        .insert("future_option".into(), toml::Value::Boolean(true));
    fs::write(&path, toml::to_string(&table).unwrap()).unwrap();
    let mut loaded = load_settings(&path).unwrap();
    let LoadStatus::RecoveredFields { fields, .. } = &loaded.status else {
        panic!("{:?}", loaded.status)
    };
    assert_eq!(fields, &["controls.player_1_device"]);
    assert_eq!(loaded.settings.controls.player_1_device, None);
    assert_eq!(
        loaded.settings.controls.player_2_device.as_deref(),
        Some("gilrs-v1:linux:picade")
    );
    loaded.settings.controls.player_2_device = None;
    save_settings(&loaded.settings, &path).unwrap();
    let reloaded = load_settings(&path).unwrap();
    assert_eq!(reloaded.settings.controls.player_1_device, None);
    assert_eq!(reloaded.settings.controls.player_2_device, None);
    assert_eq!(
        reloaded.settings.controls.controller_profiles,
        vec![profile("picade")]
    );
    assert_eq!(
        read_table(&path)["controls"]["future_option"].as_bool(),
        Some(true)
    );
}

#[test]
fn multiple_bad_types_and_enums_recover_at_the_smallest_settings_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let input = r#"
audio = false
last_scenario = []
[video]
width = "wide"
height = 900
[clock]
time_format = "future-format"
show_date = true
events = "all"
[spacewars]
use_planets = "yes"
universe_radius = 2400
[nes]
selected_rom_id = 12
future_palette = "warm"
"#;
    fs::write(&path, input).unwrap();
    let loaded = load_settings(&path).unwrap();
    let LoadStatus::RecoveredFields {
        fields,
        backup_path,
    } = &loaded.status
    else {
        panic!("{:?}", loaded.status);
    };
    let defaults = Settings::default();
    assert_eq!(fields.len(), 7);
    for field in [
        "audio",
        "last_scenario",
        "video.width",
        "clock.time_format",
        "clock.events",
        "spacewars.use_planets",
        "nes.selected_rom_id",
    ] {
        assert!(fields.iter().any(|f| f == field), "{field}: {fields:?}");
    }
    assert_eq!(loaded.settings.audio, defaults.audio);
    assert_eq!(loaded.settings.last_scenario, None);
    assert_eq!(loaded.settings.video.width, defaults.video.width);
    assert_eq!(loaded.settings.video.height, 900);
    assert_eq!(
        loaded.settings.clock.time_format,
        defaults.clock.time_format
    );
    assert_eq!(loaded.settings.clock.events, defaults.clock.events);
    assert!(loaded.settings.clock.show_date);
    assert_eq!(loaded.settings.spacewars.universe_radius, 2400);
    assert_eq!(
        loaded.settings.spacewars.use_planets,
        defaults.spacewars.use_planets
    );
    assert_eq!(fs::read_to_string(backup_path).unwrap(), input);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        input,
        "load does not rewrite the original"
    );
    save_settings(&loaded.settings, &path).unwrap();
    assert_eq!(
        read_table(&path)["nes"]["future_palette"].as_str(),
        Some("warm")
    );
    assert_eq!(load_settings(&path).unwrap().status, LoadStatus::Existing);
    assert!(
        !path.with_file_name("settings.toml.bad.1").exists(),
        "writeback reuses the original backup"
    );
}

#[test]
fn malformed_profile_is_skipped_without_losing_valid_profiles_or_other_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let mut settings = Settings::default();
    settings.audio.master_volume = 0.05;
    let good = profile("good");
    settings.controls.controller_profiles = vec![profile("bad"), good.clone()];
    let mut stored = toml::Value::try_from(&settings).unwrap();
    stored["controls"]["controller_profiles"][0]["bindings"][4]["source"]["kind"] =
        "future-input".into();
    fs::write(&path, toml::to_string(&stored).unwrap()).unwrap();
    let loaded = load_settings(&path).unwrap();
    assert_eq!(loaded.settings.controls.controller_profiles, [good]);
    assert_eq!(loaded.settings.audio.master_volume, 0.05);
    let LoadStatus::RecoveredFields { fields, .. } = loaded.status else {
        panic!()
    };
    assert_eq!(fields, ["controls.controller_profiles[0]"]);
    save_settings(&loaded.settings, &path).unwrap();
    let reloaded = load_settings(&path).unwrap();
    assert_eq!(reloaded.status, LoadStatus::Existing);
    assert_eq!(
        reloaded.settings.controls.controller_profiles,
        loaded.settings.controls.controller_profiles
    );
}

#[test]
fn unknown_record_fields_follow_identity_through_edits_reordering_and_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let mut settings = Settings::default();
    settings.controls.controller_profiles = vec![profile("first"), profile("second")];
    let mut stored = toml::Value::try_from(&settings).unwrap();
    stored["controls"]["controller_profiles"][0]
        .as_table_mut()
        .unwrap()
        .insert("future".into(), "first-extra".into());
    let second = &mut stored["controls"]["controller_profiles"][1];
    second
        .as_table_mut()
        .unwrap()
        .insert("future".into(), "second-extra".into());
    second["bindings"][0]
        .as_table_mut()
        .unwrap()
        .insert("future_binding".into(), "up-extra".into());
    second["bindings"][0]["source"]
        .as_table_mut()
        .unwrap()
        .insert("future_source".into(), true.into());
    fs::write(&path, toml::to_string(&stored).unwrap()).unwrap();
    let mut loaded = load_settings(&path).unwrap().settings;
    loaded.controls.controller_profiles.swap(0, 1);
    let second = &mut loaded.controls.controller_profiles[0];
    second.name = "Renamed second controller".into();
    second.bindings.swap(0, 1);
    second.bindings[1].source = ControllerSource::Button { code: 123 };
    save_settings(&loaded, &path).unwrap();
    let stored = read_table(&path);
    let profiles = &stored["controls"]["controller_profiles"];
    assert_eq!(profiles[0]["future"].as_str(), Some("second-extra"));
    assert_eq!(profiles[1]["future"].as_str(), Some("first-extra"));
    assert_eq!(
        profiles[0]["bindings"][1]["future_binding"].as_str(),
        Some("up-extra")
    );
    assert_eq!(
        profiles[0]["bindings"][1]["source"]["future_source"].as_bool(),
        Some(true)
    );
    loaded.controls.controller_profiles.remove(1);
    save_settings(&loaded, &path).unwrap();
    assert_eq!(
        read_table(&path)["controls"]["controller_profiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    loaded.controls.controller_profiles.clear();
    save_settings(&loaded, &path).unwrap();
    assert!(
        read_table(&path)["controls"]["controller_profiles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unknown_toml_values_and_full_width_seeds_survive_repeated_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    fs::write(
        &path,
        r#"
[launch]
seed = 18446744073709551615
[future]
maximum = 18446744073709551615
minimum = -9223372036854775808
timestamp = 2026-09-26T10:30:00Z
date = 2026-09-26
clock = 10:30:00
nan_value = nan
infinite = inf
"dotted.key" = { flag = true, nested = [1, "two"] }
records = [{ name = "one" }, { name = "two" }]
"#,
    )
    .unwrap();
    for _ in 0..3 {
        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.settings.launch.seed, u64::MAX);
        save_settings(&loaded.settings, &path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let root = toml::de::DeTable::parse(&text).unwrap();
        let future = root.get_ref()["future"].get_ref().as_table().unwrap();
        assert_eq!(
            future["maximum"].get_ref().as_integer().unwrap().as_str(),
            "18446744073709551615"
        );
        assert_eq!(
            future["minimum"].get_ref().as_integer().unwrap().as_str(),
            "-9223372036854775808"
        );
        for key in ["timestamp", "date", "clock"] {
            assert!(matches!(
                future[key].get_ref(),
                toml::de::DeValue::Datetime(_)
            ));
        }
        assert!(
            future["nan_value"]
                .get_ref()
                .as_float()
                .unwrap()
                .as_str()
                .parse::<f64>()
                .unwrap()
                .is_nan()
        );
        assert_eq!(
            future["infinite"]
                .get_ref()
                .as_float()
                .unwrap()
                .as_str()
                .parse::<f64>()
                .unwrap(),
            f64::INFINITY
        );
        assert!(future.contains_key("dotted.key"));
        assert_eq!(future["records"].get_ref().as_array().unwrap().len(), 2);
        assert_eq!(load_settings(&path).unwrap().status, LoadStatus::Existing);
    }
}

#[test]
fn save_preserves_new_unknown_disk_settings_not_just_the_startup_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let settings = Settings::default();
    save_settings(&settings, &path).unwrap();
    let mut stored = read_table(&path);
    stored.insert("future".into(), "added-after-startup".into());
    fs::write(&path, toml::to_string(&stored).unwrap()).unwrap();
    save_settings(&settings, &path).unwrap();
    assert_eq!(
        read_table(&path)["future"].as_str(),
        Some("added-after-startup")
    );
}

#[test]
fn invalid_utf8_and_syntax_are_backed_up_before_save_even_without_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    for original in [b"\xff\xfe".as_slice(), b"[invalid\n"] {
        fs::write(&path, original).unwrap();
        let loaded = load_settings(&path).unwrap();
        let LoadStatus::RecoveredMalformed { backup_path, .. } = loaded.status else {
            panic!()
        };
        assert_eq!(fs::read(backup_path).unwrap(), original);
        save_settings(&loaded.settings, &path).unwrap();
        assert_eq!(load_settings(&path).unwrap().status, LoadStatus::Existing);
    }
    let without_load = dir.path().join("unloaded.toml");
    fs::write(&without_load, "not = valid ???").unwrap();
    save_settings(&Settings::default(), &without_load).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("unloaded.toml.bad")).unwrap(),
        "not = valid ???"
    );
}

#[test]
fn excessive_invalid_records_fail_without_overwriting_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let input = format!(
        "[controls]\ncontroller_profiles = [{}]\n",
        vec!["false"; 129].join(",")
    );
    fs::write(&path, &input).unwrap();
    assert!(matches!(
        load_settings(&path),
        Err(SettingsError::Recovery(_))
    ));
    assert!(matches!(
        save_settings(&Settings::default(), &path),
        Err(SettingsError::Recovery(_))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), input);
}
