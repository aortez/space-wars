//! Settings file resolution, migration, and safe persistence.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str;

use engine_common::Settings;
use tempfile::NamedTempFile;

/// Environment variable that overrides the platform-default config directory.
///
/// Used by the Pi kiosk build (wants `/var/lib/spacewars/`) and by tests.
pub const CONFIG_DIR_ENV: &str = "SPACEWARS_CONFIG_DIR";

/// Settings filename within the config directory.
pub const SETTINGS_FILENAME: &str = "settings.toml";

/// Settings loaded from disk plus what the loader had to do with the file.
#[derive(Debug, Clone)]
pub struct LoadedSettings {
    pub settings: Settings,
    pub status: LoadStatus,
}

/// Outcome of loading the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    /// The file existed and already matched the normalized schema.
    Existing,
    /// The file did not exist; defaults should be written.
    Missing,
    /// The file was readable, but needs writeback to include defaults or
    /// normalized formatting.
    Migrated,
    /// The file was malformed. A byte-for-byte backup was written and defaults
    /// should replace the original file.
    RecoveredMalformed {
        backup_path: PathBuf,
        reason: String,
    },
}

impl LoadStatus {
    pub fn needs_writeback(&self) -> bool {
        !matches!(self, Self::Existing)
    }
}

/// Errors that can arise while loading or saving settings.
#[derive(Debug)]
pub enum SettingsError {
    /// No config directory could be resolved (no home dir, no override).
    NoConfigDir,
    /// Filesystem I/O failed.
    Io(io::Error),
    /// TOML serialization failed.
    Serialize(toml::ser::Error),
    /// Too many malformed-file backup names already exist.
    BackupPathExhausted(PathBuf),
}

impl core::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoConfigDir => write!(f, "could not resolve a config directory"),
            Self::Io(e) => write!(f, "settings I/O error: {e}"),
            Self::Serialize(e) => write!(f, "settings serialize error: {e}"),
            Self::BackupPathExhausted(path) => {
                write!(
                    f,
                    "could not choose a backup path for malformed settings at {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serialize(e) => Some(e),
            Self::NoConfigDir | Self::BackupPathExhausted(_) => None,
        }
    }
}

impl From<io::Error> for SettingsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::ser::Error> for SettingsError {
    fn from(e: toml::ser::Error) -> Self {
        Self::Serialize(e)
    }
}

/// Resolve the config directory for this install.
///
/// Priority: `$SPACEWARS_CONFIG_DIR`, then the platform default from
/// `directories::ProjectDirs` (e.g., `~/.config/spacewars/` on Linux,
/// `%APPDATA%\spacewars\` on Windows).
pub fn config_dir() -> Result<PathBuf, SettingsError> {
    if let Some(override_dir) = env::var_os(CONFIG_DIR_ENV) {
        return Ok(PathBuf::from(override_dir));
    }
    let dirs =
        directories::ProjectDirs::from("", "", "spacewars").ok_or(SettingsError::NoConfigDir)?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Full path to the settings file.
pub fn settings_path() -> Result<PathBuf, SettingsError> {
    Ok(config_dir()?.join(SETTINGS_FILENAME))
}

/// Load settings from disk. Missing fields or groups are filled from defaults.
///
/// Callers should write the returned settings back when
/// [`LoadStatus::needs_writeback`] is true.
pub fn load_settings(path: &Path) -> Result<LoadedSettings, SettingsError> {
    match fs::read(path) {
        Ok(bytes) => load_settings_from_bytes(path, &bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(LoadedSettings {
            settings: Settings::default(),
            status: LoadStatus::Missing,
        }),
        Err(e) => Err(e.into()),
    }
}

fn load_settings_from_bytes(path: &Path, bytes: &[u8]) -> Result<LoadedSettings, SettingsError> {
    let text = match str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => return recover_malformed(path, bytes, e.to_string()),
    };

    match toml::from_str::<Settings>(text) {
        Ok(mut settings) => {
            settings.audio = settings.audio.normalized();
            let normalized = serialize_settings(&settings)?;
            let status = if normalized.as_bytes() == bytes {
                LoadStatus::Existing
            } else {
                LoadStatus::Migrated
            };
            Ok(LoadedSettings { settings, status })
        }
        Err(e) => recover_malformed(path, bytes, e.to_string()),
    }
}

fn recover_malformed(
    path: &Path,
    bytes: &[u8],
    reason: String,
) -> Result<LoadedSettings, SettingsError> {
    let backup_path = write_malformed_backup(path, bytes)?;
    Ok(LoadedSettings {
        settings: Settings::default(),
        status: LoadStatus::RecoveredMalformed {
            backup_path,
            reason,
        },
    })
}

/// Save settings using a temp file in the destination directory, fsync, then an
/// atomic replace.
pub fn save_settings(settings: &Settings, path: &Path) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent)?;

    let text = serialize_settings(settings)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file_mut().sync_all()?;

    temp.into_temp_path()
        .persist(path)
        .map_err(io::Error::from)?;
    sync_parent_dir(parent)?;
    Ok(())
}

fn serialize_settings(settings: &Settings) -> Result<String, SettingsError> {
    Ok(toml::to_string_pretty(settings)?)
}

fn write_malformed_backup(path: &Path, bytes: &[u8]) -> Result<PathBuf, SettingsError> {
    for attempt in 0..1000 {
        let backup_path = malformed_backup_path(path, attempt);
        if let Some(parent) = backup_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.sync_all()?;
                if let Some(parent) = backup_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    sync_parent_dir(parent)?;
                }
                return Ok(backup_path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }

    Err(SettingsError::BackupPathExhausted(path.to_path_buf()))
}

fn malformed_backup_path(path: &Path, attempt: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| SETTINGS_FILENAME.into());
    let backup_name = if attempt == 0 {
        format!("{file_name}.bad")
    } else {
        format!("{file_name}.bad.{attempt}")
    };
    path.with_file_name(backup_name)
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use engine_common::{
        AudioSettings, ClockTimeFormat, LaunchSettings, NesSettings, RendererSetting,
        SpacewarsController, SpacewarsSettings, VideoSettings,
    };

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn fps_counter_defaults_off_and_persists_without_resetting_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[video]\nwidth = 1024\n[audio]\nmaster_volume = 0.05\n[launch]\nscenario = \"clock\"\n").unwrap();
        let mut loaded = load_settings(&path).unwrap();
        assert!(!loaded.settings.video.show_fps);
        assert_eq!(loaded.status, LoadStatus::Migrated);
        loaded.settings.video.show_fps = true;
        save_settings(&loaded.settings, &path).unwrap();
        let reloaded = load_settings(&path).unwrap();
        assert!(reloaded.settings.video.show_fps);
        assert_eq!(reloaded.settings.video.width, 1024);
        assert_eq!(reloaded.settings.audio.master_volume, 0.05);
        assert_eq!(reloaded.settings.launch.scenario, "clock");
        assert_eq!(reloaded.status, LoadStatus::Existing);
    }

    #[test]
    fn clock_message_defaults_migrates_and_round_trips_without_resetting_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "[clock]\nmarquee_preset = \"text-ribbon\"\n[clock.events]\nmarquee = false\n",
        )
        .unwrap();
        let mut loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.settings.clock.marquee_message.as_str(), "SPACE WARS");
        assert_eq!(
            loaded.settings.clock.marquee_preset,
            engine_common::ClockMarqueePreset::TextRibbon
        );
        assert!(!loaded.settings.clock.events.marquee);
        loaded.settings.clock.marquee_message = "Hi, it's 12:34!".parse().unwrap();
        save_settings(&loaded.settings, &path).unwrap();
        let reloaded = load_settings(&path).unwrap();
        assert_eq!(reloaded.settings.clock, loaded.settings.clock);
        assert_eq!(reloaded.status, LoadStatus::Existing);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("marquee_message = \"HI, IT'S 12:34!\"")
        );
    }

    #[test]
    fn malformed_clock_message_is_backed_up_not_silently_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        for text in ["", "   ", "é", "A_B", &"A".repeat(33)] {
            let original = format!("[clock]\nmarquee_message = {text:?}\n");
            fs::write(&path, &original).unwrap();
            let loaded = load_settings(&path).unwrap();
            let LoadStatus::RecoveredMalformed {
                backup_path,
                reason,
            } = loaded.status
            else {
                panic!()
            };
            assert!(reason.contains("Clock message"));
            assert_eq!(fs::read_to_string(backup_path).unwrap(), original);
            assert_eq!(loaded.settings.clock.marquee_message.as_str(), "SPACE WARS");
        }
    }

    #[test]
    fn previous_settings_default_to_one_expedition_player_and_both_counts_round_trip() {
        use engine_common::SurfaceExpeditionPlayers;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[launch]\nscenario = \"surface-expedition\"\n").unwrap();
        let mut loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.status, LoadStatus::Migrated);
        assert_eq!(loaded.settings.launch.scenario, "surface-expedition");
        assert_eq!(
            loaded.settings.surface_expedition.players,
            SurfaceExpeditionPlayers::One
        );
        for players in [SurfaceExpeditionPlayers::Two, SurfaceExpeditionPlayers::One] {
            loaded.settings.surface_expedition.players = players;
            save_settings(&loaded.settings, &path).unwrap();
            let reloaded = load_settings(&path).unwrap();
            assert_eq!(reloaded.status, LoadStatus::Existing);
            assert_eq!(reloaded.settings.surface_expedition.players, players);
            assert_eq!(reloaded.settings.launch.scenario, "surface-expedition");
        }
    }

    #[test]
    fn previous_clock_settings_default_to_calm_and_profiles_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[clock]\ntime_format = \"12-hour\"\n").unwrap();
        let mut loaded = load_settings(&path).unwrap();
        assert_eq!(
            loaded.settings.clock.event_profile,
            engine_common::ClockEventProfile::Calm
        );
        assert_eq!(
            loaded.settings.clock.time_format,
            ClockTimeFormat::TwelveHour
        );
        assert_eq!(
            loaded.settings.clock.events,
            engine_common::ClockEvents::default()
        );
        for profile in [
            engine_common::ClockEventProfile::Off,
            engine_common::ClockEventProfile::Calm,
            engine_common::ClockEventProfile::Demo,
        ] {
            loaded.settings.clock.event_profile = profile;
            loaded.settings.clock.events = engine_common::ClockEvents {
                falling: false,
                color_cycle: true,
                meltdown: false,
                duck: false,
                marquee: false,
                digit_slide: false,
            };
            save_settings(&loaded.settings, &path).unwrap();
            assert_eq!(
                load_settings(&path).unwrap().settings.clock,
                loaded.settings.clock
            );
        }
    }

    #[test]
    fn older_event_switches_keep_their_values_when_meltdown_defaults_on() {
        let settings: Settings =
            toml::from_str("[clock.events]\nfalling = false\ncolor_cycle = false\n").unwrap();
        assert!(!settings.clock.events.falling);
        assert!(!settings.clock.events.color_cycle);
        assert!(settings.clock.events.meltdown);
        assert!(settings.clock.events.duck);
    }

    #[test]
    fn pre_marquee_settings_keep_existing_switches_and_recipes_round_trip() {
        let mut settings:Settings=toml::from_str("[clock]\nevent_profile='off'\n[clock.events]\nfalling=false\ncolor_cycle=true\nmeltdown=false\nduck=false\n").unwrap();
        assert!(settings.clock.events.marquee);
        assert!(!settings.clock.events.duck);
        assert!(!settings.clock.events.falling);
        assert_eq!(
            settings.clock.marquee_preset,
            engine_common::ClockMarqueePreset::ClockWave
        );
        for preset in engine_common::ClockMarqueePreset::ALL {
            settings.clock.marquee_preset = preset;
            let decoded: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(decoded.clock, settings.clock);
        }
    }

    #[test]
    fn digit_slide_defaults_on_without_changing_existing_clock_choices() {
        let mut settings: Settings = toml::from_str("[clock]\ntime_format='12-hour'\nevent_profile='off'\nmarquee_preset='text-ribbon'\nmarquee_message='HELLO'\n[clock.events]\nfalling=false\ncolor_cycle=true\nmeltdown=false\nduck=false\nmarquee=false\n").unwrap();
        assert!(settings.clock.events.digit_slide);
        assert!(!settings.clock.events.marquee);
        assert_eq!(
            settings.clock.event_profile,
            engine_common::ClockEventProfile::Off
        );
        assert_eq!(
            settings.clock.time_format,
            engine_common::ClockTimeFormat::TwelveHour
        );
        assert_eq!(settings.clock.marquee_message.as_str(), "HELLO");
        for enabled in [true, false] {
            settings.clock.events.digit_slide = enabled;
            let restored: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.clock, settings.clock);
        }
    }

    #[test]
    fn pre_duck_settings_preserve_all_existing_switches() {
        let settings: Settings = toml::from_str("[clock]\nevent_profile = 'off'\n[clock.events]\nfalling = false\ncolor_cycle = true\nmeltdown = false\n").unwrap();
        assert!(settings.clock.events.duck);
        assert!(!settings.clock.events.falling);
        assert!(settings.clock.events.color_cycle);
        assert!(!settings.clock.events.meltdown);
        assert_eq!(
            settings.clock.event_profile,
            engine_common::ClockEventProfile::Off
        );
    }

    #[test]
    fn missing_file_yields_defaults_and_needs_writeback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.status, LoadStatus::Missing);
        assert!(loaded.status.needs_writeback());
        assert_eq!(loaded.settings.last_scenario, None);
        assert_eq!(loaded.settings.video.width, 1280);
        assert!(!loaded.settings.video.fullscreen);
        assert_eq!(loaded.settings.launch.scenario, "spacewars");
        assert_eq!(loaded.settings.launch.seed, 0);
        assert_eq!(loaded.settings.launch.renderer, RendererSetting::Vector);
        assert_eq!(loaded.settings.launch.raster_scale, 1.0);
        assert_eq!(loaded.settings.spacewars, SpacewarsSettings::default());
        assert_eq!(
            loaded.settings.clock.time_format,
            ClockTimeFormat::TwentyFourHour
        );
        assert!(!path.exists(), "load should not create the file.");
    }

    #[test]
    fn partial_file_uses_defaults_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[video]\nwidth = 1920\n").unwrap();

        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.status, LoadStatus::Migrated);
        assert_eq!(loaded.settings.video.width, 1920);
        assert_eq!(loaded.settings.video.height, 720);
        assert!(!loaded.settings.video.fullscreen);
        assert_eq!(loaded.settings.audio.master_volume, 0.25);
        assert_eq!(loaded.settings.launch.scenario, "spacewars");
        assert_eq!(loaded.settings.spacewars, SpacewarsSettings::default());
        assert_eq!(loaded.settings.runtime.log_level, "info");

        save_settings(&loaded.settings, &path).unwrap();
        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("[video]"));
        assert!(migrated.contains("width = 1920"));
        assert!(migrated.contains("height = 720"));
        assert!(migrated.contains("fullscreen = false"));
        assert!(migrated.contains("[audio]"));
        assert!(migrated.contains("[launch]"));
        assert!(migrated.contains("[spacewars]"));
        assert!(migrated.contains("[clock]"));
        assert!(migrated.contains("[runtime]"));
    }

    #[test]
    fn audio_load_preserves_saved_levels_and_normalizes_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        for (raw, expected) in [
            ("0.8", 0.8),
            ("0.0", 0.0),
            ("-1.0", 0.0),
            ("3.0", 1.0),
            ("nan", 0.25),
            ("inf", 0.25),
        ] {
            let text = format!("[audio]\nmaster_volume = {raw}\nmuted = true\n");
            let loaded = load_settings_from_bytes(&path, text.as_bytes()).unwrap();
            assert_eq!(loaded.settings.audio.master_volume, expected, "{raw}");
            assert!(loaded.settings.audio.muted);
            save_settings(&loaded.settings, &path).unwrap();
            assert_eq!(
                load_settings(&path).unwrap().settings.audio,
                loaded.settings.audio
            );
        }
    }

    #[test]
    fn invalid_launch_fields_fall_back_and_migrate_without_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "[launch]\nscenario = 99\nseed = -7\nrenderer = \"gpu\"\nraster_scale = \"wide\"\n",
        )
        .unwrap();

        let loaded = load_settings(&path).unwrap();

        assert_eq!(loaded.status, LoadStatus::Migrated);
        assert_eq!(loaded.settings.launch.scenario, "spacewars");
        assert_eq!(loaded.settings.launch.seed, 0);
        assert_eq!(loaded.settings.launch.renderer, RendererSetting::Vector);
        assert_eq!(loaded.settings.launch.raster_scale, 1.0);
        assert!(
            !path.with_file_name("settings.toml.bad").exists(),
            "launch field validation should not trigger malformed-file recovery"
        );
    }

    #[test]
    fn combat_breaks_default_in_old_settings_and_roundtrip_off_and_custom_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[launch]\nseed = 42\n").unwrap();
        let mut loaded = load_settings(&path).unwrap();
        assert_eq!(
            loaded.settings.combat_breaks,
            engine_common::CombatBreakSettings::default()
        );
        for interval in [0, 8, 30] {
            loaded.settings.combat_breaks = engine_common::CombatBreakSettings {
                interval_seconds: interval,
                duration_seconds: 6,
            };
            save_settings(&loaded.settings, &path).unwrap();
            assert_eq!(
                load_settings(&path).unwrap().settings.combat_breaks,
                loaded.settings.combat_breaks
            );
        }
    }

    #[test]
    fn legacy_player_selection_defaults_p1_to_human_and_new_bot_choices_roundtrip() {
        use engine_common::SpacewarsController::{Human, RuleBot};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[spacewars]\nplayer_2_controller = \"rule-bot\"\n").unwrap();
        let mut loaded = load_settings(&path).unwrap().settings;
        assert_eq!(loaded.spacewars.player_1_controller, Human);
        assert_eq!(loaded.spacewars.player_2_controller, RuleBot);
        loaded.spacewars.player_1_controller = RuleBot;
        save_settings(&loaded, &path).unwrap();
        assert_eq!(
            load_settings(&path).unwrap().settings.spacewars,
            loaded.spacewars
        );
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.toml");

        let settings = Settings {
            last_scenario: Some("spacewars".into()),
            video: VideoSettings {
                width: 1920,
                fullscreen: true,
                ..Default::default()
            },
            audio: AudioSettings {
                muted: true,
                ..Default::default()
            },
            launch: LaunchSettings {
                scenario: "spacewars".into(),
                seed: 42,
                renderer: RendererSetting::Raster,
                raster_scale: 2.0,
            },
            nes: NesSettings {
                selected_rom_id: Some("0123456789abcdef".into()),
            },
            spacewars: SpacewarsSettings {
                universe_radius: 2400,
                asteroids_enabled: false,
                asteroid_probability_per_sec: 80.0,
                player_health_percent: 250,
                player_2_controller: SpacewarsController::RuleBot,
                ..Default::default()
            },
            ..Default::default()
        };
        save_settings(&settings, &path).unwrap();

        let reloaded = load_settings(&path).unwrap();
        assert_eq!(reloaded.status, LoadStatus::Existing);
        assert_eq!(
            reloaded.settings.last_scenario.as_deref(),
            Some("spacewars")
        );
        assert_eq!(reloaded.settings.video.width, 1920);
        assert!(reloaded.settings.video.fullscreen);
        assert!(reloaded.settings.audio.muted);
        assert_eq!(reloaded.settings.launch.scenario, "spacewars");
        assert_eq!(reloaded.settings.launch.seed, 42);
        assert_eq!(reloaded.settings.launch.renderer, RendererSetting::Raster);
        assert_eq!(reloaded.settings.launch.raster_scale, 2.0);
        assert_eq!(
            reloaded.settings.nes.selected_rom_id.as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(reloaded.settings.spacewars.universe_radius, 2400);
        assert!(!reloaded.settings.spacewars.asteroids_enabled);
        assert_eq!(
            reloaded.settings.spacewars.asteroid_probability_per_sec,
            80.0
        );
        assert_eq!(reloaded.settings.spacewars.player_health_percent, 250);
        assert_eq!(
            reloaded.settings.clock.time_format,
            ClockTimeFormat::TwentyFourHour
        );
        assert_eq!(
            reloaded.settings.spacewars.player_2_controller,
            SpacewarsController::RuleBot
        );
    }

    #[test]
    fn malformed_file_is_backed_up_and_defaults_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, b"[runtime]\ncrash_behavior = ???\n").unwrap();

        let loaded = load_settings(&path).unwrap();
        let backup_path = match loaded.status {
            LoadStatus::RecoveredMalformed { backup_path, .. } => backup_path,
            other => panic!("unexpected status: {other:?}"),
        };

        assert_eq!(loaded.settings.video.width, 1280);
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "[runtime]\ncrash_behavior = ???\n"
        );

        save_settings(&loaded.settings, &path).unwrap();
        let reloaded = load_settings(&path).unwrap();
        assert_eq!(reloaded.status, LoadStatus::Existing);
    }

    #[test]
    fn config_dir_env_override_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();

        // SAFETY: this test owns SPACEWARS_CONFIG_DIR while ENV_LOCK is held.
        unsafe {
            env::set_var(CONFIG_DIR_ENV, dir.path());
        }
        let resolved = config_dir().unwrap();
        unsafe {
            env::remove_var(CONFIG_DIR_ENV);
        }

        assert_eq!(resolved, dir.path());
    }
}
