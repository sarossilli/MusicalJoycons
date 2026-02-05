//! # Musical JoyCons
//!
//! A library for playing MIDI files through Nintendo JoyCon rumble motors.
//!
//! ## Features
//!
//! - **JoyCon Management**: Discover, connect to, and control Nintendo JoyCons
//! - **MIDI Parsing**: Convert MIDI files to rumble commands
//! - **Track Analysis**: Intelligently select and score tracks for playback
//! - **Synchronized Playback**: Play music across multiple JoyCons
//!
//! ## Quick Start
//!
//! ```no_run
//! use musical_joycons::joycon::JoyConManager;
//! use musical_joycons::midi::play_midi_file;
//! use std::path::PathBuf;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let path = PathBuf::from("music.mid");
//!     play_midi_file(path)?;
//!     Ok(())
//! }
//! ```

pub mod joycon;
pub mod midi;

/// Prelude module for convenient imports.
///
/// Use `use musical_joycons::prelude::*;` to import commonly used types.
pub mod prelude {
    pub use crate::joycon::{JoyCon, JoyConError, JoyConManager, JoyConType};
    pub use crate::midi::{play_midi_file, RumbleCommand, RumbleTrack, TrackMetrics, TrackType};
}