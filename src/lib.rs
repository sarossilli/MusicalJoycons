//! # Musical JoyCons
//!
//! A Rust library for playing MIDI files through Nintendo JoyCon rumble motors,
//! turning your game controllers into musical instruments.
//!
//! ## Overview
//!
//! This library provides a complete solution for:
//! - Discovering and connecting to Nintendo JoyCons via Bluetooth/USB
//! - Parsing MIDI files and converting notes to rumble frequencies
//! - Intelligent track selection based on musical analysis
//! - Synchronized multi-JoyCon playback with dynamic track switching
//!
//! ## Features
//!
//! - **JoyCon Management**: Automatic device discovery with retry logic,
//!   support for Left/Right JoyCons and Pro Controllers
//! - **MIDI Parsing**: Full MIDI file support with tempo changes, velocity scaling,
//!   and proper timing calculations
//! - **Track Analysis**: Scores tracks based on note density, melodic movement,
//!   pitch range, and other musical characteristics
//! - **Synchronized Playback**: Coordinates multiple JoyCons playing different
//!   tracks with intelligent track switching during silent periods
//!
//! ## Quick Start
//!
//! The simplest way to play a MIDI file:
//!
//! ```no_run
//! use musical_joycons::midi::play_midi_file;
//! use std::path::PathBuf;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let path = PathBuf::from("music.mid");
//!     play_midi_file(path)?;
//!     Ok(())
//! }
//! ```
//!
//! ## Manual JoyCon Control
//!
//! For more control over the JoyCons:
//!
//! ```no_run
//! use musical_joycons::joycon::{JoyConManager, JoyConType};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     // Create manager and find JoyCons
//!     let manager = JoyConManager::new()?;
//!     let mut joycons = manager.connect_and_initialize_joycons()?;
//!     
//!     // Play a note on the first JoyCon (440 Hz = A4, 80% amplitude)
//!     if let Some(joycon) = joycons.first_mut() {
//!         joycon.rumble(440.0, 0.8)?;
//!         std::thread::sleep(std::time::Duration::from_millis(500));
//!         joycon.rumble(0.0, 0.0)?; // Stop rumble
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Frequency Range
//!
//! The JoyCon rumble motor supports frequencies from approximately 40 Hz to 1252 Hz.
//! Frequencies outside this range are automatically octave-shifted to fit within
//! the supported range.
//!
//! ## Architecture
//!
//! The library is organized into two main modules:
//!
//! - [`joycon`]: Device discovery, connection management, and rumble control
//! - [`midi`]: MIDI parsing, track analysis, and playback coordination
//!
//! ## Platform Support
//!
//! This library uses the `hidapi` crate for HID communication, which supports:
//! - Windows (native HID)
//! - macOS (IOHidManager)
//! - Linux (hidraw or libusb)

pub mod joycon;
pub mod midi;

/// Prelude module for convenient imports.
///
/// This module re-exports the most commonly used types from the library,
/// allowing you to quickly get started with a single import.
///
/// # Example
///
/// ```no_run
/// use musical_joycons::prelude::*;
/// use std::path::PathBuf;
///
/// fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///     // All common types are now in scope
///     let manager = JoyConManager::new()?;
///     let joycons = manager.connect_and_initialize_joycons()?;
///     
///     for joycon in &joycons {
///         match joycon.get_type() {
///             JoyConType::Left => println!("Found Left JoyCon"),
///             JoyConType::Right => println!("Found Right JoyCon"),
///             JoyConType::ProController => println!("Found Pro Controller"),
///             JoyConType::Other(id) => println!("Found unknown device: {}", id),
///         }
///     }
///     
///     Ok(())
/// }
/// ```
pub mod prelude {
    pub use crate::joycon::{JoyCon, JoyConError, JoyConManager, JoyConType};
    pub use crate::midi::{play_midi_file, RumbleCommand, RumbleTrack, TrackMetrics, TrackType};
}
