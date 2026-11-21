//! The one cartridge this profile decodes, and the shared way of getting it loaded.
//!
//! An address map is only true of the image it was read off, and the core will run
//! any iNES image without complaint. So the check has to sit on this side of the
//! boundary: [`machine`] refuses an image whose identity is not the pinned one, and
//! a wrong cartridge then fails as "wrong cartridge" rather than as a decoded world
//! that is quietly nonsense somewhere two screens in.
//!
//! The identity is the core's own [`CartridgeIdentity`], a length and an FNV-1a-64
//! over the header plus PRG ROM plus CHR ROM plus trainer, because that is what a
//! running machine can answer without anything being re-parsed. The SHA-256 that
//! documents and tests quote is a different function over a different span and is
//! what a person can check against a file, so it stays there, behind a
//! dev-dependency, rather than becoming a dependency of the model.

use std::{env, fmt, fs, io, path::PathBuf};

use engine_nes::{CartridgeError, CartridgeIdentity, CartridgeImage, MachineConfig, NesMachine};

/// The image this map was verified against.
pub const SMB_NAME: &str = "Super Mario Bros. (JU) (PRG0) [!]";

/// [`CartridgeImage::parse`]'s identity for [`SMB_NAME`]: a 16-byte header, 32 KiB
/// of PRG ROM and 8 KiB of CHR ROM, no trainer.
pub const SMB_IDENTITY: CartridgeIdentity = CartridgeIdentity {
    byte_len: 40_976,
    fnv1a64: 0xbf12_0bdc_b897_9836,
};

/// Point the reader at a cartridge other than [`DEFAULT_ROM`].
pub const ROM_PATH_ENV: &str = "SPACEWARS_SMB_ROM";

/// Where the cartridge is looked for when the environment does not say otherwise.
/// `data/roms` is not in the repository, so this path is allowed to not exist.
pub const DEFAULT_ROM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/roms/smb.nes");

/// Why a cartridge could not be put under this profile.
#[derive(Debug)]
pub enum Unusable {
    /// Nothing readable was at [`path`].
    Unreadable { path: PathBuf, reason: io::Error },
    /// An iNES image that is not the game and revision this map decodes.
    Wrong {
        expected: CartridgeIdentity,
        actual: CartridgeIdentity,
    },
    /// Not a loadable iNES image at all.
    Malformed(CartridgeError),
}

impl fmt::Display for Unusable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, reason } => {
                write!(f, "no cartridge readable at {}: {reason}", path.display())
            }
            Self::Wrong { expected, actual } => write!(
                f,
                "cartridge is not {SMB_NAME}: expected {expected:?}, got {actual:?}",
            ),
            Self::Malformed(error) => write!(f, "cartridge is not an iNES image: {error}"),
        }
    }
}

impl std::error::Error for Unusable {}

/// The path a cartridge will be read from: [`ROM_PATH_ENV`] if set, else
/// [`DEFAULT_ROM`].
pub fn path() -> PathBuf {
    env::var_os(ROM_PATH_ENV).map_or_else(|| PathBuf::from(DEFAULT_ROM), PathBuf::from)
}

/// An image's identity as the core would see it, without building a machine.
pub fn identity(rom: &[u8]) -> Result<CartridgeIdentity, CartridgeError> {
    CartridgeImage::parse(rom).map(|image| image.identity())
}

/// Read the cartridge at [`path`].
///
/// `Ok(None)` means the file is simply not there. An absent cartridge is a skip for
/// a test and a usage error for an example, and only the caller knows which, so
/// this reports which of the three cases it is in rather than picking one.
pub fn read() -> Result<Option<Vec<u8>>, Unusable> {
    read_from(None)
}

/// [`read`], for a caller that was handed a path and should honour it.
///
/// Every driver takes an optional cartridge argument and falls back to [`path`], so
/// one command line works on a machine whose image lives elsewhere and an explicit
/// argument still wins over the environment.
pub fn read_from(chosen: Option<PathBuf>) -> Result<Option<Vec<u8>>, Unusable> {
    let path = chosen.unwrap_or_else(path);
    match fs::read(&path) {
        Ok(rom) => Ok(Some(rom)),
        Err(reason) if reason.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "no cartridge at {}; set {ROM_PATH_ENV} to point at one",
                path.display(),
            );
            Ok(None)
        }
        Err(reason) => Err(Unusable::Unreadable { path, reason }),
    }
}

/// [`read`], with a read that failed reported as no cartridge at all.
///
/// A test that needs a cartridge and cannot have one skips rather than fails, and a
/// file that is present but will not open is rare enough that reporting it and
/// treating it as absent beats every caller writing the same match to get there.
pub fn read_or_skip() -> Option<Vec<u8>> {
    match read() {
        Ok(rom) => rom,
        Err(error) => {
            eprintln!("skipping: {error}");
            None
        }
    }
}

/// Power on a machine that is known to be running the cartridge this profile
/// decodes.
///
/// The identity is read back out of the machine rather than computed here, so the
/// check is against what is actually running and not against a second parse that
/// could disagree with the first.
pub fn machine(rom: &[u8]) -> Result<NesMachine, Unusable> {
    let machine =
        NesMachine::from_ines(rom, MachineConfig::default()).map_err(Unusable::Malformed)?;
    let actual = machine.cartridge_identity();
    if actual != SMB_IDENTITY {
        return Err(Unusable::Wrong {
            expected: SMB_IDENTITY,
            actual,
        });
    }
    Ok(machine)
}
