//! What a Super Mario Bros. cartridge says about itself, above the core and
//! below the bots.
//!
//! The core in `engine-nes` emulates hardware and deliberately knows nothing
//! about this game: it has no word for a pipe, a goomba or a level. Everything
//! that does lives here, on the host side of that boundary, and it is arranged in
//! four layers because they fail for different reasons and a failure has to be
//! attributable to one of them.
//!
//! | Layer | Asks | Cannot say |
//! | --- | --- | --- |
//! | [`profile`] | what does this byte of RAM mean | what is on screen |
//! | [`frame`] | what is drawn, and where in the level | whether it can be stood on |
//! | [`resistance`] | what the game refused to let him through | what to do about it |
//! | [`affordances`] | what a body can act on right now | how to act |
//!
//! Each layer may use the ones above it and never a lower one, which is what makes
//! "the reader saw it and was wrong" a question with an answer. There is no
//! re-export at the crate root on purpose: a consumer's `use` line names the layer
//! it is asking, so a skill that has reached down for a tile number or a PPU
//! register says so in its own imports.
//!
//! [`cartridge`] is not a fifth layer. It is the check that the image under the
//! reader is the image this map was written for, which every layer depends on and
//! none of them can perform.

#![forbid(unsafe_code)]

pub mod affordances;
pub mod cartridge;
pub mod frame;
pub mod profile;
pub mod resistance;
