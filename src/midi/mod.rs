//! MIDI file parsing and playback functionality.
//!
//! This module provides everything needed to parse MIDI files and play them
//! through JoyCon rumble motors, including intelligent track selection and
//! multi-JoyCon coordination.
//!
//! # Overview
//!
//! The MIDI module handles:
//! - Parsing standard MIDI files (SMF format)
//! - Converting MIDI notes to rumble frequencies
//! - Analyzing tracks to select the most "playable" ones
//! - Coordinating synchronized playback across multiple JoyCons
//! - Dynamic track switching during silent periods
//!
//! # Quick Start
//!
//! The simplest way to play a MIDI file:
//!
//! ```no_run
//! use musical_joycons::midi::play_midi_file;
//! use std::path::PathBuf;
//!
//! let path = PathBuf::from("song.mid");
//! play_midi_file(path)?;
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! # Track Analysis
//!
//! When a MIDI file contains multiple tracks, the library automatically
//! analyzes each track and selects the best ones for playback. Tracks are
//! scored based on:
//!
//! - **Note density**: How many notes per second
//! - **Melodic movement**: How much the pitch varies
//! - **Pitch range**: The span from lowest to highest note
//! - **Velocity variance**: Dynamic range of note volumes
//! - **Track type**: Melody, harmony, bass, or drums
//!
//! Percussion tracks (MIDI channel 10) are automatically excluded as they
//! don't translate well to rumble frequencies.
//!
//! # MIDI to Frequency Conversion
//!
//! MIDI note numbers are converted to frequencies using the standard formula:
//!
//! ```text
//! frequency = 440 * 2^((note - 69) / 12)
//! ```
//!
//! Where note 69 is A4 (440 Hz). Frequencies outside the JoyCon's range
//! (approximately 40-1252 Hz) are octave-shifted to fit.
//!
//! # Multi-JoyCon Playback
//!
//! When multiple JoyCons are connected, the library:
//!
//! 1. Assigns the highest-scoring tracks to each JoyCon
//! 2. Monitors track activity during playback
//! 3. Switches JoyCons to more active tracks during silent periods
//! 4. Maintains synchronization across all devices
//!
//! # Tempo and Timing
//!
//! The module correctly handles:
//! - MIDI tempo changes
//! - Different time signatures
//! - Variable tick resolutions
//! - Proper note-on/note-off pairing

mod playback;
mod rumble;
mod track_analysis;
mod track_types;

// Re-export public types
pub use playback::play_midi_file;
pub use rumble::{
    parse_midi_to_rumble, ParseError, RumbleCommand, RumbleTrack, TrackMergeController,
    TrackSwitchPoint,
};
pub use track_analysis::analyze_track;
pub use track_types::{TrackMetrics, TrackType};
