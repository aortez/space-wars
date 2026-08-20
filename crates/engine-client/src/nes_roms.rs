//! Discovery and bounded loading for user-supplied NES cartridge images.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use engine_nes::{CartridgeError, CartridgeImage, CartridgeMetadata};
use sha2::{Digest, Sha256};

pub const ROM_DIRECTORY_NAME: &str = "roms";
pub const MAX_ROM_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RomDigest([u8; 32]);

impl RomDigest {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn as_hex(self) -> String {
        let mut value = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            use fmt::Write as _;
            write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
        }
        value
    }
}

impl fmt::Display for RomDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NesRomCompatibility {
    Supported(CartridgeMetadata),
    Rejected(CartridgeError),
    Unreadable(String),
    TooLarge { actual: u64, maximum: usize },
}

impl NesRomCompatibility {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported(_))
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Supported(metadata) => {
                let chr = if metadata.chr_is_ram {
                    "CHR RAM".to_string()
                } else {
                    format!("{} KiB CHR", metadata.chr_rom_len / 1024)
                };
                format!(
                    "Mapper {} · {} KiB PRG · {chr}",
                    metadata.mapper,
                    metadata.prg_rom_len / 1024,
                )
            }
            Self::Rejected(error) => error.to_string(),
            Self::Unreadable(error) => format!("Could not read cartridge: {error}"),
            Self::TooLarge { actual, maximum } => format!(
                "Cartridge is {} bytes; the library limit is {} bytes",
                actual, maximum
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NesRomCatalogEntry {
    pub id: String,
    pub display_name: String,
    pub path: PathBuf,
    pub digest: Option<RomDigest>,
    pub byte_len: u64,
    pub compatibility: NesRomCompatibility,
}

impl NesRomCatalogEntry {
    pub fn is_supported(&self) -> bool {
        self.compatibility.is_supported()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NesRomAsset {
    pub display_name: String,
    pub source_path: PathBuf,
    pub digest: RomDigest,
    pub image: CartridgeImage,
}

#[derive(Debug)]
pub enum NesRomLoadError {
    NotFound {
        id: String,
    },
    Unavailable {
        name: String,
        detail: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    TooLarge {
        path: PathBuf,
        maximum: usize,
    },
    Changed {
        path: PathBuf,
    },
    Cartridge {
        path: PathBuf,
        source: CartridgeError,
    },
}

impl fmt::Display for NesRomLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { id } => write!(formatter, "ROM {id:?} is no longer in the library"),
            Self::Unavailable { name, detail } => {
                write!(formatter, "ROM {name:?} cannot be launched: {detail}")
            }
            Self::Io { path, source } => {
                write!(formatter, "could not read ROM {}: {source}", path.display())
            }
            Self::TooLarge { path, maximum } => write!(
                formatter,
                "ROM {} exceeds the {maximum}-byte library limit",
                path.display()
            ),
            Self::Changed { path } => write!(
                formatter,
                "ROM {} changed after the library was scanned; reopen the launcher",
                path.display()
            ),
            Self::Cartridge { path, source } => {
                write!(
                    formatter,
                    "ROM {} is not supported: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for NesRomLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Cartridge { source, .. } => Some(source),
            Self::NotFound { .. }
            | Self::Unavailable { .. }
            | Self::TooLarge { .. }
            | Self::Changed { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct NesRomCatalog {
    directory: PathBuf,
    entries: Vec<NesRomCatalogEntry>,
}

impl NesRomCatalog {
    pub fn new(config_directory: &Path) -> Self {
        Self {
            directory: config_directory.join(ROM_DIRECTORY_NAME),
            entries: Vec::new(),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn entries(&self) -> &[NesRomCatalogEntry] {
        &self.entries
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let mut paths = fs::read_dir(&self.directory)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                (file_type.is_file() && has_nes_extension(&entry.path())).then(|| entry.path())
            })
            .collect::<Vec<_>>();
        paths.sort_by_key(|path| path_sort_key(path));

        let mut digests = BTreeSet::new();
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            let entry = inspect_path(path);
            if entry.digest.is_some_and(|digest| !digests.insert(digest)) {
                continue;
            }
            entries.push(entry);
        }
        entries.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        self.entries = entries;
        Ok(())
    }

    pub fn first_supported_id(&self) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.is_supported())
            .map(|entry| entry.id.as_str())
    }

    pub fn entry(&self, id: &str) -> Option<&NesRomCatalogEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn load(&self, id: &str) -> Result<NesRomAsset, NesRomLoadError> {
        let entry = self
            .entry(id)
            .ok_or_else(|| NesRomLoadError::NotFound { id: id.into() })?;
        if !entry.is_supported() {
            return Err(NesRomLoadError::Unavailable {
                name: entry.display_name.clone(),
                detail: entry.compatibility.summary(),
            });
        }
        load_entry(entry)
    }
}

pub fn load_path(path: &Path) -> Result<NesRomAsset, NesRomLoadError> {
    let bytes = read_bounded(path)?;
    let digest = RomDigest::from_bytes(&bytes);
    let image = CartridgeImage::parse(&bytes).map_err(|source| NesRomLoadError::Cartridge {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(NesRomAsset {
        display_name: display_name(path),
        source_path: path.to_path_buf(),
        digest,
        image,
    })
}

fn load_entry(entry: &NesRomCatalogEntry) -> Result<NesRomAsset, NesRomLoadError> {
    let asset = load_path(&entry.path)?;
    if entry.digest != Some(asset.digest) {
        return Err(NesRomLoadError::Changed {
            path: entry.path.clone(),
        });
    }
    Ok(asset)
}

fn inspect_path(path: PathBuf) -> NesRomCatalogEntry {
    let display_name = display_name(&path);
    let fallback_id = format!("path-{:016x}", fnv1a64(path_sort_key(&path).as_bytes()));
    match read_bounded(&path) {
        Ok(bytes) => {
            let digest = RomDigest::from_bytes(&bytes);
            let compatibility = match CartridgeImage::parse(&bytes) {
                Ok(image) => NesRomCompatibility::Supported(image.metadata()),
                Err(error) => NesRomCompatibility::Rejected(error),
            };
            NesRomCatalogEntry {
                id: digest.as_hex(),
                display_name,
                path,
                digest: Some(digest),
                byte_len: bytes.len() as u64,
                compatibility,
            }
        }
        Err(NesRomLoadError::TooLarge { .. }) => {
            let actual = fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            NesRomCatalogEntry {
                id: fallback_id,
                display_name,
                path,
                digest: None,
                byte_len: actual,
                compatibility: NesRomCompatibility::TooLarge {
                    actual,
                    maximum: MAX_ROM_BYTES,
                },
            }
        }
        Err(error) => NesRomCatalogEntry {
            id: fallback_id,
            display_name,
            path,
            digest: None,
            byte_len: 0,
            compatibility: NesRomCompatibility::Unreadable(error.to_string()),
        },
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, NesRomLoadError> {
    let file = File::open(path).map_err(|source| NesRomLoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_ROM_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| NesRomLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_ROM_BYTES {
        return Err(NesRomLoadError::TooLarge {
            path: path.to_path_buf(),
            maximum: MAX_ROM_BYTES,
        });
    }
    Ok(bytes)
}

fn display_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Unnamed cartridge".into())
}

fn has_nes_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("nes"))
}

fn path_sort_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_nes::test_rom::{
        AxromBuilder, CnromBuilder, Mmc1Builder, Mmc3Builder, NromBuilder, UxromBuilder,
    };

    fn write_rom(path: &Path) {
        fs::write(path, NromBuilder::new_16k().build()).unwrap();
    }

    #[test]
    fn refresh_creates_the_managed_library_directory() {
        let config = tempfile::tempdir().unwrap();
        let mut catalog = NesRomCatalog::new(config.path());

        catalog.refresh().unwrap();

        assert!(catalog.directory().is_dir());
        assert!(catalog.entries().is_empty());
    }

    #[test]
    fn catalog_discovers_supported_roms_in_stable_name_order() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        write_rom(&directory.join("Zebra.nes"));
        let mut second = NromBuilder::new_16k();
        second.write(0x8000, &[0x42]);
        fs::write(directory.join("alpha.NES"), second.build()).unwrap();
        fs::write(directory.join("notes.txt"), b"not a cartridge").unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        assert_eq!(catalog.entries().len(), 2);
        assert_eq!(catalog.entries()[0].display_name, "alpha");
        assert_eq!(catalog.entries()[1].display_name, "Zebra");
        assert!(
            catalog
                .entries()
                .iter()
                .all(NesRomCatalogEntry::is_supported)
        );
        assert!(catalog.first_supported_id().is_some());
    }

    #[test]
    fn catalog_accepts_mapper_two_cartridges() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("uxrom.nes"), UxromBuilder::new(8).build()).unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Supported(CartridgeMetadata { mapper: 2, .. })
        ));
    }

    #[test]
    fn catalog_accepts_mapper_one_cartridges() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("mmc1.nes"),
            Mmc1Builder::with_chr_rom(8, 4).build(),
        )
        .unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Supported(CartridgeMetadata { mapper: 1, .. })
        ));
    }

    #[test]
    fn catalog_accepts_mapper_three_cartridges() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("cnrom.nes"),
            CnromBuilder::new_32k(4).build(),
        )
        .unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Supported(CartridgeMetadata { mapper: 3, .. })
        ));
    }

    #[test]
    fn catalog_accepts_mapper_four_cartridges() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("mmc3.nes"),
            Mmc3Builder::with_chr_rom(8, 4).build(),
        )
        .unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Supported(CartridgeMetadata { mapper: 4, .. })
        ));
    }

    #[test]
    fn catalog_accepts_mapper_seven_cartridges() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("axrom.nes"), AxromBuilder::new(8).build()).unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Supported(CartridgeMetadata { mapper: 7, .. })
        ));
    }

    #[test]
    fn catalog_retains_an_unsupported_mapper_with_a_clear_reason() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        let mut bytes = NromBuilder::new_16k().build();
        bytes[6] = (bytes[6] & 0x0f) | 0x50;
        fs::write(directory.join("mapper-five.nes"), bytes).unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        let entry = &catalog.entries()[0];
        assert!(!entry.is_supported());
        assert!(matches!(
            entry.compatibility,
            NesRomCompatibility::Rejected(CartridgeError::UnsupportedMapper(5))
        ));
        assert!(entry.compatibility.summary().contains("mapper 5"));
    }

    #[test]
    fn identical_cartridges_are_deduplicated_by_content() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        write_rom(&directory.join("first.nes"));
        write_rom(&directory.join("second.nes"));

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].display_name, "first");
    }

    #[test]
    fn loading_rejects_a_cartridge_changed_since_the_scan() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        let path = directory.join("changing.nes");
        write_rom(&path);

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();
        let id = catalog.entries()[0].id.clone();
        let mut replacement = NromBuilder::new_16k();
        replacement.write(0x8000, &[0x99]);
        fs::write(&path, replacement.build()).unwrap();

        assert!(matches!(
            catalog.load(&id),
            Err(NesRomLoadError::Changed { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn catalog_does_not_follow_symlinks_outside_the_library() {
        use std::os::unix::fs::symlink;

        let config = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_rom = outside.path().join("outside.nes");
        write_rom(&outside_rom);
        let directory = config.path().join(ROM_DIRECTORY_NAME);
        fs::create_dir(&directory).unwrap();
        symlink(outside_rom, directory.join("linked.nes")).unwrap();

        let mut catalog = NesRomCatalog::new(config.path());
        catalog.refresh().unwrap();

        assert!(catalog.entries().is_empty());
    }
}
