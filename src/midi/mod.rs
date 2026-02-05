//! MIDI file parsing and playback functionality.
//!
//! This module provides:
//! - MIDI file parsing and conversion to rumble commands
//! - Track analysis and scoring
//! - Playback coordination

mod playback;
mod rumble;
mod track_analysis;
mod track_types;

// Re-export public types
pub use playback::play_midi_file;
pub use rumble::{ParseError, RumbleCommand, RumbleTrack, TrackMergeController, TrackSwitchPoint};
pub use track_analysis::analyze_track;
pub use track_types::{TrackMetrics, TrackType};