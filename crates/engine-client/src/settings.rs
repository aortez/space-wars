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
        Ok(settings) => {
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

    use engine_common::{AudioSettings, VideoSettings};

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn missing_file_yields_defaults_and_needs_writeback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        let loaded = load_settings(&path).unwrap();
        assert_eq!(loaded.status, LoadStatus::Missing);
        assert!(loaded.status.needs_writeback());
        assert_eq!(loaded.settings.last_scenario, None);
        assert_eq!(loaded.settings.video.width, 1280);
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
        assert_eq!(loaded.settings.audio.master_volume, 0.8);
        assert_eq!(loaded.settings.runtime.log_level, "info");

        save_settings(&loaded.settings, &path).unwrap();
        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("[video]"));
        assert!(migrated.contains("width = 1920"));
        assert!(migrated.contains("height = 720"));
        assert!(migrated.contains("[audio]"));
        assert!(migrated.contains("[runtime]"));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.toml");

        let settings = Settings {
            last_scenario: Some("spacewars".into()),
            video: VideoSettings {
                width: 1920,
                ..Default::default()
            },
            audio: AudioSettings {
                muted: true,
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
        assert!(reloaded.settings.audio.muted);
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
