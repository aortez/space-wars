//! Players for the Super Mario Bros. world model, and the places they train.
//!
//! `smb-model` says what a cartridge means; this crate decides what to do about it.
//! The split is the one this repository already uses for its own game, where the
//! world lives in a scenario and the players in an AI crate, with the cartridge
//! standing in for the scenario. Nothing here is reachable from `engine-nes`, and
//! the model never sees any of it: a bot borrows the machine and reads it, which is
//! the same thing a person looking at the screen does.
//!
//! | Module | What it is |
//! | --- | --- |
//! | [`policy`] | the base policy, and the frame-by-frame envelope it acts through |
//! | [`gym`] | a start state, an objective, a frame budget and a verdict |
//! | [`gyms`] | the gyms this project has actually needed |
//! | [`skills`] | measured behaviours, and the scripted bot that strings them together |
//!
//! A gym stores a tape rather than a savestate, so a core change that moves the game
//! fails loudly at the tape instead of quietly moving the goalposts. See
//! `docs/smb-execution-plan.md` for why that is the opinionated call, and
//! `docs/smb-mission.md` for what all of it is for.

#![forbid(unsafe_code)]

pub mod gym;
pub mod gyms;
pub mod policy;
pub mod skills;
