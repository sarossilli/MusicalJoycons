//! MIDI track type detection, scoring, and playback planning.
//!
//! This module provides the [`TrackMetrics`] struct for analyzing MIDI tracks,
//! the [`TrackType`] enum for classifying track musical roles, and the
//! [`PlaybackPlan`] / [`SectionAssignment`] types for pre-computed track
//! assignments based on skyline melody detection.
//!
//! # Track Type Detection
//!
//! Track types are detected using:
//! 1. MIDI program number (instrument)
//! 2. Track/instrument name keywords
//! 3. Musical characteristics (pitch range, density)

use std::time::Duration;

/// Metrics and analysis data for a MIDI track.
///
/// This struct contains all the information needed to score and classify
/// a MIDI track for rumble playback. It is populated by [`analyze_track`](super::analyze_track).
///
/// # Scoring
///
/// Use [`calculate_score`](Self::calculate_score) to get an overall quality score
/// that can be used to rank tracks for playback selection.
///
/// # Example
///
/// ```
/// use musical_joycons::midi::TrackMetrics;
///
/// let metrics = TrackMetrics::default();
/// let score = metrics.calculate_score();
/// println!("Track score: {:.2}", score);
/// ```
#[derive(Debug, Clone)]
pub struct TrackMetrics {
    /// Index of this track in the original MIDI file (0-based).
    pub track_index: usize,

    /// Total number of note-on events in the track.
    pub note_count: usize,

    /// Number of unique pitches used in the track.
    pub unique_notes: usize,

    /// Average MIDI velocity (normalized to 0.0-1.0).
    pub avg_velocity: f32,

    /// Standard deviation of velocity values.
    /// Higher values indicate more dynamic range.
    pub velocity_variance: f32,

    /// Average duration of notes in seconds.
    pub avg_note_duration: f32,

    /// Total duration of the track in seconds.
    pub total_duration: f32,

    /// Notes per second (note_count / total_duration).
    pub note_density: f32,

    /// Whether this track is on MIDI channel 10 (percussion).
    pub is_percussion: bool,

    /// Track name from MIDI metadata (if present).
    pub track_name: Option<String>,

    /// Instrument name from MIDI metadata (if present).
    pub track_instrument: Option<String>,

    /// Detected musical role of this track.
    pub track_type: TrackType,

    /// Difference between highest and lowest MIDI note numbers.
    pub pitch_range: u8,

    /// Average pitch change between consecutive notes.
    /// Higher values indicate more melodic movement.
    pub melodic_movement: f32,

    /// Ratio of time notes are sounding vs. total time.
    /// Value from 0.0 (all silence) to 1.0 (continuous sound).
    pub sustain_ratio: f32,

    /// Measure of timing consistency (0.0 to 1.0).
    /// Higher values indicate more regular rhythm.
    pub rhythmic_regularity: f32,
}

/// Classification of MIDI track musical role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    /// Primary melodic content
    Melody,
    /// Harmonic/accompaniment content
    Harmony,
    /// Bass line
    Bass,
    /// Percussion/drums (channel 10)
    Drums,
    /// Vocal-like content
    Vocals,
    /// Could not determine track type
    Unknown,
}

/// A section of the song with fixed JoyCon-to-track assignments.
///
/// Each section represents a contiguous period where the melody (and complement)
/// stay on the same tracks, as determined by skyline analysis.
#[derive(Debug, Clone)]
pub struct SectionAssignment {
    /// Time at which this section begins.
    pub start_time: Duration,
    /// Track index assigned to each JoyCon for this section.
    /// Index 0 is the melody track, index 1+ are complements.
    pub track_indices: Vec<usize>,
}

/// Pre-computed playback plan mapping song sections to track assignments.
///
/// Built by analyzing the full MIDI file with the skyline algorithm before
/// playback begins. During playback, JoyCon threads follow this plan instead
/// of re-ranking tracks in real time.
#[derive(Debug, Clone)]
pub struct PlaybackPlan {
    /// Ordered list of sections sorted by `start_time`.
    pub sections: Vec<SectionAssignment>,
}

impl PlaybackPlan {
    /// Returns the track index assigned to a JoyCon at the given time.
    pub fn track_for(&self, joycon_idx: usize, time: Duration) -> usize {
        self.sections
            .iter()
            .rev()
            .find(|s| s.start_time <= time)
            .and_then(|s| s.track_indices.get(joycon_idx).copied())
            .unwrap_or(0)
    }

    /// Returns the start time of the next section after `time`, if any.
    pub fn next_section_time(&self, time: Duration) -> Option<Duration> {
        self.sections
            .iter()
            .find(|s| s.start_time > time)
            .map(|s| s.start_time)
    }
}

pub(crate) struct TrackWeights {
    pub note_density: f32,
    pub velocity_variance: f32,
    pub unique_notes: f32,
    pub avg_note_duration: f32,
    pub pitch_range: f32,
    pub melodic_movement: f32,
    pub sustain_ratio: f32,
    pub rhythmic_regularity: f32,
}

impl Default for TrackMetrics {
    fn default() -> Self {
        Self {
            track_index: 0,
            note_count: 0,
            unique_notes: 0,
            avg_velocity: 0.0,
            velocity_variance: 0.0,
            avg_note_duration: 0.0,
            total_duration: 0.0,
            note_density: 0.0,
            is_percussion: false,
            track_name: None,
            track_instrument: None,
            track_type: TrackType::Unknown,
            pitch_range: 0,
            melodic_movement: 0.0,
            sustain_ratio: 0.0,
            rhythmic_regularity: 0.0,
        }
    }
}

impl TrackMetrics {
    /// Calculates an overall quality score for this track.
    ///
    /// The score represents how well this track will work for rumble playback.
    /// Higher scores indicate better tracks. Percussion tracks always return 0.0.
    ///
    /// # Scoring Components
    ///
    /// The score is computed from weighted components:
    ///
    /// | Component | Weight | Ideal Value |
    /// |-----------|--------|-------------|
    /// | Note density | 25% | 3 notes/sec |
    /// | Velocity variance | 10% | Higher is better |
    /// | Note variety | 15% | 50% unique |
    /// | Note duration | 15% | 0.3 seconds |
    /// | Pitch range | 15% | ~38 semitones |
    /// | Melodic movement | 10% | 2 semitones avg |
    /// | Sustain ratio | 5% | 60% |
    /// | Rhythmic regularity | 5% | Higher is better |
    ///
    /// # Track Type Multipliers
    ///
    /// The base score is then multiplied based on track type:
    /// - Harmony: 1.5x
    /// - Melody: 1.3x
    /// - Bass: 1.1x
    /// - Vocals: 1.1x
    /// - Drums: 0.2x
    ///
    /// # Bonus
    ///
    /// Tracks with ideal characteristics get an additional 1.3x bonus.
    ///
    /// # Returns
    ///
    /// A score value, typically between 0.0 and 1.0 for most tracks,
    /// but can exceed 1.0 with multipliers.
    pub fn calculate_score(&self) -> f32 {
        if self.is_percussion {
            return 0.0;
        }

        let weights = TrackWeights {
            note_density: 0.25,
            velocity_variance: 0.10,
            unique_notes: 0.15,
            avg_note_duration: 0.15,
            pitch_range: 0.15,
            melodic_movement: 0.10,
            sustain_ratio: 0.05,
            rhythmic_regularity: 0.05,
        };

        let density_score = {
            let ideal_density = 3.0;
            let density_diff = (self.note_density - ideal_density).abs();
            (1.0 - (density_diff / 5.0)).max(0.0) * weights.note_density
        };

        let velocity_score = (self.velocity_variance / 30.0).min(1.0) * weights.velocity_variance;

        let variety_score = {
            let unique_ratio = self.unique_notes as f32 / self.note_count.max(1) as f32;
            let ideal_ratio = 0.5;
            (1.0 - (unique_ratio - ideal_ratio).abs()) * weights.unique_notes
        };

        let duration_score = {
            let ideal_duration = 0.3;
            let duration_diff = (self.avg_note_duration - ideal_duration).abs();
            (1.0 - (duration_diff / 1.0)).max(0.0) * weights.avg_note_duration
        };

        let pitch_range_score = {
            let normalized_range = self.pitch_range as f32 / 127.0;
            let ideal_range = 0.3;
            (1.0 - (normalized_range - ideal_range).abs()) * weights.pitch_range
        };

        let melodic_movement_score = {
            let ideal_movement = 2.0;
            let movement_diff = (self.melodic_movement - ideal_movement).abs();
            (1.0 - (movement_diff / 12.0)).max(0.0) * weights.melodic_movement
        };

        let sustain_score = {
            let ideal_sustain = 0.6;
            (1.0 - (self.sustain_ratio - ideal_sustain).abs()) * weights.sustain_ratio
        };

        let rhythmic_score = self.rhythmic_regularity * weights.rhythmic_regularity;

        let base_score = density_score
            + velocity_score
            + variety_score
            + duration_score
            + pitch_range_score
            + melodic_movement_score
            + sustain_score
            + rhythmic_score;

        let type_multiplier = match self.track_type {
            TrackType::Harmony => 1.5,
            TrackType::Melody => 1.3,
            TrackType::Bass => 1.1,
            TrackType::Vocals => 1.1,
            TrackType::Drums => 0.2,
            TrackType::Unknown => 1.0,
        };

        let bonus_multiplier = if self.note_density >= 2.0
            && self.note_density <= 4.0
            && self.unique_notes >= 5
            && self.unique_notes <= 20
            && self.note_count >= 300
        {
            1.3
        } else {
            1.0
        };

        base_score * type_multiplier * bonus_multiplier
    }

    /// Calculates a score for a specific time window of this track.
    ///
    /// Unlike [`calculate_score`](Self::calculate_score) which evaluates the whole track,
    /// this method scores a specific section. It's used during playback to determine
    /// if switching to this track would be beneficial.
    ///
    /// # Arguments
    ///
    /// * `window_note_count` - Number of notes in the time window
    /// * `window_duration` - Duration of the window in seconds
    ///
    /// # Scoring
    ///
    /// The score primarily weights note count (80%), with bonuses for:
    /// - Ideal note density (around 3 notes/sec): up to 20% bonus
    /// - Melody tracks: 15% bonus
    /// - Bass tracks: 10% bonus
    ///
    /// # Returns
    ///
    /// Window score (0.0 for percussion, otherwise positive).
    pub fn calculate_window_score(&self, window_note_count: usize, window_duration: f32) -> f32 {
        if self.is_percussion {
            return 0.0;
        }

        // Make note count the primary factor
        let note_score = window_note_count as f32 * 0.8; // 80% of score comes from note count

        // Calculate density as a bonus multiplier
        let window_density = if window_duration > 0.0 {
            window_note_count as f32 / window_duration
        } else {
            0.0
        };

        // Small bonus for good density (avoid extremely sparse or dense sections)
        let density_multiplier = {
            let ideal_density = 3.0;
            let density_diff = (window_density - ideal_density).abs();
            1.0 + (0.2 * (1.0 - (density_diff / 5.0)).max(0.0)) // Up to 20% bonus
        };

        // Apply track type as a smaller multiplier
        let type_multiplier = match self.track_type {
            TrackType::Melody => 1.15,  // Reduced from 1.3
            TrackType::Bass => 1.1,     // Same
            TrackType::Harmony => 1.05, // Reduced from 1.5
            TrackType::Vocals => 1.1,   // Same
            TrackType::Drums => 0.0,
            TrackType::Unknown => 1.0,
        };

        note_score * density_multiplier * type_multiplier
    }

    /// Determines and sets the track type based on available information.
    ///
    /// This method uses a multi-stage detection process:
    ///
    /// 1. **Percussion check**: If the track is on MIDI channel 10, it's drums
    /// 2. **Program number**: MIDI instrument numbers map to categories
    /// 3. **Name keywords**: Track/instrument names are searched for hints
    /// 4. **Musical characteristics**: Density, pitch range, sustain patterns
    ///
    /// # Arguments
    ///
    /// * `program_number` - MIDI program number (0-127) if a Program Change
    ///   event was found, or `None`
    ///
    /// # Program Number Mapping
    ///
    /// | Program Range | Track Type |
    /// |---------------|------------|
    /// | 0-7 | Melody (Piano) |
    /// | 8-31 | Harmony (Chromatic/Organ/Guitar) |
    /// | 32-39 | Bass |
    /// | 40-55 | Harmony (Strings/Ensemble) |
    /// | 56-87 | Melody (Brass/Reed/Pipe/Synth Lead) |
    /// | 88-103 | Harmony (Synth Pad/Effects) |
    /// | 104-111 | Melody (Ethnic) |
    /// | 112-119 | Drums (Percussive) |
    /// | 120-127 | Unknown (Sound Effects) |
    ///
    /// # Fallback Detection
    ///
    /// If no program number or name hints are available:
    /// - High density + many unique notes → Melody
    /// - High sustain ratio → Harmony
    /// - Otherwise → Unknown
    pub fn determine_track_type(&mut self, program_number: Option<u8>) {
        if self.is_percussion {
            self.track_type = TrackType::Drums;
            return;
        }

        // First check program numbers if available
        if let Some(program) = program_number {
            self.track_type = match program {
                0..=7 => TrackType::Melody,      // Piano family
                8..=15 => TrackType::Harmony,    // Chromatic Percussion
                16..=23 => TrackType::Harmony,   // Organ
                24..=31 => TrackType::Harmony,   // Guitar
                32..=39 => TrackType::Bass,      // Bass
                40..=47 => TrackType::Harmony,   // Strings
                48..=55 => TrackType::Harmony,   // Ensemble
                56..=63 => TrackType::Melody,    // Brass
                64..=71 => TrackType::Melody,    // Reed
                72..=79 => TrackType::Melody,    // Pipe
                80..=87 => TrackType::Melody,    // Synth Lead
                88..=95 => TrackType::Harmony,   // Synth Pad
                96..=103 => TrackType::Harmony,  // Synth Effects
                104..=111 => TrackType::Melody,  // Ethnic
                112..=119 => TrackType::Drums,   // Percussive
                120..=127 => TrackType::Unknown, // Sound Effects
                _ => TrackType::Unknown,
            };
            return;
        }

        // Check track name and instrument hints
        let track_name = self.track_name.as_deref().unwrap_or("").to_lowercase();
        let instrument = self
            .track_instrument
            .as_deref()
            .unwrap_or("")
            .to_lowercase();

        let is_likely_vocal = track_name.contains("voc")
            || track_name.contains("sing")
            || track_name.contains("voice")
            || instrument.contains("vocal");

        let is_likely_bass = track_name.contains("bass") || instrument.contains("bass");

        let is_likely_melody = track_name.contains("lead")
            || track_name.contains("melody")
            || instrument.contains("lead");

        // Determine type based on characteristics if no clear naming matches
        if is_likely_vocal {
            self.track_type = TrackType::Vocals;
        } else if is_likely_bass {
            self.track_type = TrackType::Bass;
        } else if is_likely_melody || (self.note_density > 3.0 && self.unique_notes > 12) {
            self.track_type = TrackType::Melody;
        } else if self.sustain_ratio > 0.7 {
            self.track_type = TrackType::Harmony;
        } else {
            self.track_type = TrackType::Unknown;
        }
    }
}
