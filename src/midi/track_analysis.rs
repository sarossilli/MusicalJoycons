//! MIDI track analysis and metrics calculation.
//!
//! This module provides the [`analyze_track`] function for extracting
//! musical characteristics from MIDI tracks. The resulting metrics are
//! used to score and classify tracks for optimal rumble playback.
//!
//! # Analysis Process
//!
//! The analyzer makes a single pass through the track, collecting:
//! - Note events (pitch, velocity, timing, duration)
//! - Metadata (track name, instrument)
//! - Channel information (for percussion detection)
//! - Program changes (for instrument identification)
//!
//! # Computed Metrics
//!
//! From the raw data, the following metrics are computed:
//!
//! - **Note density**: Notes per second of track duration
//! - **Pitch range**: Difference between highest and lowest notes
//! - **Melodic movement**: Average pitch change between notes
//! - **Velocity variance**: Standard deviation of note velocities
//! - **Sustain ratio**: Fraction of time with active notes
//! - **Rhythmic regularity**: Consistency of note timing

use std::collections::HashMap;

use midly::TrackEventKind;

use super::parts::Part;
use super::track_types::TrackMetrics;

/// Analyzes a MIDI track and returns comprehensive metrics.
///
/// This function performs a complete analysis of a MIDI track, extracting
/// all the information needed for scoring and classification.
///
/// # Arguments
///
/// * `track` - The MIDI track to analyze (slice of track events)
/// * `ticks_per_beat` - MIDI timing resolution (from file header)
/// * `default_tempo` - Default tempo in microseconds per beat (typically 500,000 = 120 BPM)
///
/// # Returns
///
/// A [`TrackMetrics`] struct containing all computed metrics. The `track_type`
/// field is set based on detected characteristics.
///
/// # Timing Calculation
///
/// Time values are calculated using:
/// ```text
/// time_seconds = ticks * (tempo_microseconds / ticks_per_beat) / 1,000,000
/// ```
///
/// Note: This function uses the `default_tempo` for all calculations.
/// For accurate timing with tempo changes, use the full conversion in `rumble.rs`.
///
/// # Percussion Detection
///
/// Tracks on MIDI channel 10 (index 9) are automatically marked as percussion
/// and assigned `TrackType::Drums`.
///
/// # Example
///
/// ```no_run
/// use musical_joycons::midi::analyze_track;
/// use midly::Smf;
///
/// let midi_data = std::fs::read("song.mid")?;
/// let smf = Smf::parse(&midi_data)?;
///
/// for (i, track) in smf.tracks.iter().enumerate() {
///     let metrics = analyze_track(track, 480.0, 500_000);
///     println!("Track {}: {} notes, type: {:?}",
///              i, metrics.note_count, metrics.track_type);
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn analyze_track(track: &[midly::TrackEvent], ticks_per_beat: f32, default_tempo: u32) -> TrackMetrics {
    let mut metrics = TrackMetrics::default();
    let mut active_notes: HashMap<u8, f32> = HashMap::new();
    let mut note_pitches = Vec::new();
    let mut note_timings = Vec::new();
    let mut velocities = Vec::new();
    let mut note_durations = Vec::new();
    let mut total_note_duration = 0.0;
    let mut program_number = None;

    let mut current_time = 0.0;
    let current_tempo = default_tempo as f32;
    let microseconds_per_tick = current_tempo / ticks_per_beat;
    let mut max_velocity: u32 = 1;

    for event in track.iter() {
        current_time += event.delta.as_int() as f32 * microseconds_per_tick / 1_000_000.0;

        match event.kind {
            TrackEventKind::Meta(meta) => match meta {
                /*
                midly::MetaMessage::Tempo(tempo) => {
                    current_tempo = tempo.as_int() as f32;
                }
                 */
                midly::MetaMessage::TrackName(name) => {
                    metrics.track_name = Some(String::from_utf8_lossy(name).into_owned());
                }
                midly::MetaMessage::InstrumentName(name) => {
                    metrics.track_instrument = Some(String::from_utf8_lossy(name).into_owned());
                }
                _ => {}
            },
            TrackEventKind::Midi { channel, message } => {
                if channel.as_int() == 9 {
                    metrics.is_percussion = true;
                }

                match message {
                    midly::MidiMessage::ProgramChange { program } => {
                        program_number = Some(program.as_int());
                    }
                    midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                        metrics.note_count += 1;
                        let pitch = key.as_int();
                        note_pitches.push(pitch);
                        note_timings.push(current_time);

                        let velocity = vel.as_int();
                        max_velocity = max_velocity.max(velocity as u32); // Track maximum velocity
                        velocities.push(velocity as f32);

                        active_notes.insert(pitch, current_time);
                    }
                    midly::MidiMessage::NoteOff { key, vel }
                    | midly::MidiMessage::NoteOn { key, vel }
                        if vel.as_int() == 0 =>
                    {
                        if let Some(start_time) = active_notes.remove(&key.as_int()) {
                            let duration = current_time - start_time;
                            note_durations.push(duration);
                            total_note_duration += duration;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Normalize velocities using max_velocity
    velocities = velocities
        .into_iter()
        .map(|v| v / max_velocity as f32)
        .collect();

    // Scale velocities to full MIDI range (0-127)
    velocities = velocities.into_iter().map(|v| v / 127.0).collect();

    // Recalculate metrics with properly scaled velocities
    metrics.avg_velocity = if metrics.note_count > 0 {
        velocities.iter().sum::<f32>() / metrics.note_count as f32
    } else {
        0.0
    };

    if velocities.len() > 1 {
        let mean = metrics.avg_velocity;
        metrics.velocity_variance = (velocities.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
            / (velocities.len() - 1) as f32)
            .sqrt();
    }

    // Calculate basic metrics
    metrics.total_duration = current_time;
    metrics.note_density = if current_time > 0.0 {
        metrics.note_count as f32 / current_time
    } else {
        0.0
    };

    metrics.avg_note_duration = if !note_durations.is_empty() {
        note_durations.iter().sum::<f32>() / note_durations.len() as f32
    } else {
        0.0
    };

    let unique_pitches: std::collections::HashSet<_> = note_pitches.iter().copied().collect();
    metrics.unique_notes = unique_pitches.len();

    if !note_pitches.is_empty() {
        let min_pitch = note_pitches.iter().min().copied().unwrap_or(0);
        let max_pitch = note_pitches.iter().max().copied().unwrap_or(0);
        metrics.pitch_range = max_pitch.saturating_sub(min_pitch);
    }

    if note_pitches.len() > 1 {
        let mut total_movement = 0.0;
        for window in note_pitches.windows(2) {
            total_movement += (window[1] as f32 - window[0] as f32).abs();
        }
        metrics.melodic_movement = total_movement / (note_pitches.len() - 1) as f32;
    }

    metrics.sustain_ratio = if metrics.total_duration > 0.0 {
        total_note_duration / metrics.total_duration
    } else {
        0.0
    };

    if note_timings.len() > 1 {
        let intervals: Vec<f32> = note_timings.windows(2).map(|w| w[1] - w[0]).collect();

        let mean_interval = intervals.iter().sum::<f32>() / intervals.len() as f32;
        let variance = intervals
            .iter()
            .map(|x| (x - mean_interval).powi(2))
            .sum::<f32>()
            / intervals.len() as f32;

        metrics.rhythmic_regularity = 1.0 / (1.0 + (variance / mean_interval).sqrt());
    }

    metrics.determine_track_type(program_number);

    metrics
}

/// Rich feature set computed from a [`Part`] for the dual-scoring system.
///
/// Unlike [`TrackMetrics`] which is computed from raw MIDI track events,
/// `PartFeatures` operates on pre-resolved `NoteObject`s with absolute
/// start/end ticks, enabling polyphony and interval analysis.
#[derive(Debug, Clone)]
pub struct PartFeatures {
    // -- activity / salience --
    pub note_count: usize,
    pub total_note_time: f32,
    pub notes_per_sec: f32,
    pub active_ratio: f32,
    pub velocity_mean: f32,
    pub velocity_p80: f32,

    // -- pitch --
    pub median_pitch: f32,
    pub p75_pitch: f32,
    pub pitch_range_p10_p90: f32,

    // -- melody-likeness --
    pub monophony_ratio: f32,
    pub chordiness: f32,
    pub stepwise_motion: f32,

    // -- instrument priors (soft biases) --
    pub melody_bias: f32,
    pub accompaniment_bias: f32,
    pub bass_bias: f32,

    pub is_drum: bool,
    pub program: u8,
}

impl Default for PartFeatures {
    fn default() -> Self {
        Self {
            note_count: 0,
            total_note_time: 0.0,
            notes_per_sec: 0.0,
            active_ratio: 0.0,
            velocity_mean: 0.0,
            velocity_p80: 0.0,
            median_pitch: 0.0,
            p75_pitch: 0.0,
            pitch_range_p10_p90: 0.0,
            monophony_ratio: 0.0,
            chordiness: 0.0,
            stepwise_motion: 0.0,
            melody_bias: 0.0,
            accompaniment_bias: 0.0,
            bass_bias: 0.0,
            is_drum: false,
            program: 0,
        }
    }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p * (sorted.len() - 1) as f32).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Computes GM-program-based instrument priors: (melody_bias, accompaniment_bias, bass_bias).
fn instrument_priors(program: u8) -> (f32, f32, f32) {
    match program {
        0..=7 => (0.3, 0.3, 0.0),       // Piano – could be either
        8..=15 => (0.0, 0.4, 0.0),       // Chromatic Percussion
        16..=23 => (0.0, 0.5, 0.0),      // Organ
        24..=31 => (0.2, 0.4, 0.0),      // Guitar
        32..=39 => (0.0, 0.0, 0.8),      // Bass
        40..=47 => (0.1, 0.5, 0.0),      // Strings
        48..=55 => (0.1, 0.5, 0.0),      // Ensemble
        56..=63 => (0.6, 0.0, 0.0),      // Brass
        64..=71 => (0.6, 0.0, 0.0),      // Reed / Sax
        72..=79 => (0.6, 0.0, 0.0),      // Pipe / Flute
        80..=87 => (0.7, 0.0, 0.0),      // Synth Lead
        88..=95 => (0.0, 0.6, 0.0),      // Synth Pad
        96..=103 => (0.0, 0.4, 0.0),     // Synth Effects
        104..=111 => (0.5, 0.1, 0.0),    // Ethnic
        112..=119 => (0.0, 0.0, 0.0),    // Percussive (handled by is_drum)
        _ => (0.0, 0.0, 0.0),
    }
}

/// Analyze a [`Part`] and return its [`PartFeatures`].
///
/// `ticks_per_beat` and `default_tempo` are used to convert ticks to seconds.
/// `song_end_tick` is the latest tick across **all** parts in the file so that
/// `active_ratio` reflects how much of the *whole song* this part covers.
pub fn analyze_part(
    part: &Part,
    ticks_per_beat: f32,
    default_tempo: u32,
    song_end_tick: u32,
) -> PartFeatures {
    let mut features = PartFeatures {
        is_drum: part.is_drum,
        program: part.key.program,
        ..Default::default()
    };

    if part.notes.is_empty() {
        return features;
    }

    let micros_per_tick = default_tempo as f64 / ticks_per_beat as f64;
    let tick_to_sec = |t: u32| (t as f64 * micros_per_tick / 1_000_000.0) as f32;

    features.note_count = part.notes.len();

    // Collect per-note data.
    let mut pitches: Vec<f32> = Vec::with_capacity(part.notes.len());
    let mut velocities: Vec<f32> = Vec::with_capacity(part.notes.len());
    let mut onset_secs: Vec<f32> = Vec::with_capacity(part.notes.len());
    let mut total_note_time: f32 = 0.0;

    let mut latest_tick = 0u32;

    for n in &part.notes {
        pitches.push(n.pitch as f32);
        velocities.push(n.velocity as f32 / 127.0);
        onset_secs.push(tick_to_sec(n.start_tick));
        let dur = tick_to_sec(n.end_tick) - tick_to_sec(n.start_tick);
        total_note_time += dur.max(0.0);

        latest_tick = latest_tick.max(n.end_tick);
    }

    // Use full song duration for active_ratio so sparse fill-parts
    // don't appear artificially active.
    let global_duration = tick_to_sec(song_end_tick).max(0.001);
    // Per-part duration is still useful for notes_per_sec.
    let part_duration = tick_to_sec(latest_tick).max(0.001);

    features.total_note_time = total_note_time;
    features.notes_per_sec = features.note_count as f32 / part_duration;
    features.velocity_mean = velocities.iter().sum::<f32>() / velocities.len() as f32;

    // Percentile computations.
    let mut sorted_pitches = pitches.clone();
    sorted_pitches.sort_by(|a, b| a.partial_cmp(b).unwrap());
    features.median_pitch = percentile(&sorted_pitches, 0.5);
    features.p75_pitch = percentile(&sorted_pitches, 0.75);
    let p10 = percentile(&sorted_pitches, 0.10);
    let p90 = percentile(&sorted_pitches, 0.90);
    features.pitch_range_p10_p90 = p90 - p10;

    let mut sorted_vel = velocities.clone();
    sorted_vel.sort_by(|a, b| a.partial_cmp(b).unwrap());
    features.velocity_p80 = percentile(&sorted_vel, 0.80);

    // --- Monophony ratio & chordiness ---
    // Build a timeline of (tick, +1/-1) events and sweep.
    let mut events: Vec<(u32, i32)> = Vec::with_capacity(part.notes.len() * 2);
    for n in &part.notes {
        events.push((n.start_tick, 1));
        events.push((n.end_tick, -1));
    }
    events.sort_by_key(|e| (e.0, -e.1)); // ends before starts at same tick

    let mut active = 0i32;
    let mut prev_tick = events.first().map(|e| e.0).unwrap_or(0);
    let mut mono_ticks: f64 = 0.0;
    let mut active_ticks: f64 = 0.0;
    let mut weighted_polyphony: f64 = 0.0;

    for &(tick, delta) in &events {
        if tick > prev_tick {
            let span = (tick - prev_tick) as f64;
            if active >= 1 {
                active_ticks += span;
                weighted_polyphony += span * active as f64;
                if active <= 1 {
                    mono_ticks += span;
                }
            }
        }
        active += delta;
        prev_tick = tick;
    }

    features.monophony_ratio = if active_ticks > 0.0 {
        (mono_ticks / active_ticks) as f32
    } else {
        1.0
    };

    features.chordiness = if active_ticks > 0.0 {
        (weighted_polyphony / active_ticks) as f32
    } else {
        0.0
    };

    features.active_ratio = {
        let active_secs = (active_ticks * micros_per_tick / 1_000_000.0) as f32;
        (active_secs / global_duration).min(1.0)
    };

    // --- Stepwise motion ---
    if pitches.len() > 1 {
        let mut total_interval = 0.0f32;
        for w in pitches.windows(2) {
            total_interval += (w[1] - w[0]).abs();
        }
        features.stepwise_motion = total_interval / (pitches.len() - 1) as f32;
    }

    // --- Instrument priors ---
    let (mel, acc, bas) = instrument_priors(part.key.program);
    features.melody_bias = mel;
    features.accompaniment_bias = acc;
    features.bass_bias = bas;

    features
}

#[cfg(test)]
mod tests {
    use super::super::track_types::TrackType;
    use super::*;
    use midly::{MetaMessage, MidiMessage, TrackEvent, TrackEventKind};
    fn create_note_on(delta: u32, key: u8, velocity: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: 0.into(),
                message: MidiMessage::NoteOn {
                    key: key.into(),
                    vel: velocity.into(),
                },
            },
        }
    }

    fn create_note_off(delta: u32, key: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: 0.into(),
                message: MidiMessage::NoteOff {
                    key: key.into(),
                    vel: 0.into(),
                },
            },
        }
    }

    fn create_track_name(name: &str) -> TrackEvent<'static> {
        let bytes: &'static [u8] = Box::leak(name.as_bytes().to_owned().into_boxed_slice());
        TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(bytes)),
        }
    }

    fn create_test_track(events: Vec<TrackEvent<'static>>) -> Vec<TrackEvent<'static>> {
        events
    }

    #[test]
    fn test_empty_track() {
        let track = create_test_track(vec![]);
        let metrics = analyze_track(&track, 480.0, 500_000);

        assert_eq!(metrics.note_count, 0);
        assert_eq!(metrics.unique_notes, 0);
        assert_eq!(metrics.note_density, 0.0);
        assert_eq!(metrics.velocity_variance, 0.0);
        assert!(!metrics.is_percussion);
    }

    #[test]
    fn test_single_note() {
        let track = create_test_track(vec![
            create_note_on(0, 60, 100), // Middle C, velocity 100
            create_note_off(480, 60),   // Duration of 1 beat
        ]);

        let metrics = analyze_track(&track, 480.0, 500_000);

        assert_eq!(metrics.note_count, 1);
        assert_eq!(metrics.unique_notes, 1);
        assert_eq!(metrics.pitch_range, 0); // Only one note
        assert!(metrics.note_density > 0.0);
    }

    #[test]
    fn test_track_name_and_type() {
        let track = create_test_track(vec![
            create_track_name("Bass Guitar"),
            create_note_on(0, 40, 100),
            create_note_off(480, 40),
        ]);

        let metrics = analyze_track(&track, 480.0, 500_000);

        assert_eq!(metrics.track_name, Some("Bass Guitar".to_string()));
        assert_eq!(metrics.track_type, TrackType::Bass); // Should detect bass from name
    }

    #[test]
    fn test_percussion_channel() {
        let mut percussion_event = create_note_on(0, 60, 100);
        if let TrackEventKind::Midi { channel, .. } = &mut percussion_event.kind {
            *channel = 9.into(); // Channel 10 (9 in 0-based) is percussion
        }

        let track = create_test_track(vec![percussion_event]);
        let metrics = analyze_track(&track, 480.0, 500_000);

        assert!(metrics.is_percussion);
        assert_eq!(metrics.track_type, TrackType::Drums);
    }

    #[test]
    fn test_note_density_calculation() {
        let track = create_test_track(vec![
            create_note_on(0, 60, 100),
            create_note_off(240, 60), // Quarter note duration
            create_note_on(0, 64, 100),
            create_note_off(240, 64), // Another quarter note
        ]);

        let metrics = analyze_track(&track, 480.0, 500_000);
        assert!(metrics.note_density > 0.0);
        assert_eq!(metrics.note_count, 2);
        assert_eq!(metrics.unique_notes, 2);
    }

    #[test]
    fn test_pitch_range() {
        let track = create_test_track(vec![
            create_note_on(0, 60, 100), // Middle C
            create_note_off(240, 60),
            create_note_on(0, 72, 100), // C one octave higher
            create_note_off(240, 72),
        ]);

        let metrics = analyze_track(&track, 480.0, 500_000);
        assert_eq!(metrics.pitch_range, 12); // One octave difference
    }

    #[test]
    fn test_velocity_variance() {
        let track = create_test_track(vec![
            create_note_on(0, 60, 50), // Soft note
            create_note_off(240, 60),
            create_note_on(0, 60, 100), // Loud note
            create_note_off(240, 60),
        ]);

        let metrics = analyze_track(&track, 480.0, 500_000);
        assert!(metrics.velocity_variance > 0.0);
        assert_eq!(metrics.note_count, 2);
    }

    // ---- analyze_part tests ----

    use super::super::parts::{NoteObject, Part, PartKey};

    fn make_note(start: u32, end: u32, pitch: u8, vel: u8) -> NoteObject {
        NoteObject {
            start_tick: start,
            end_tick: end,
            pitch,
            velocity: vel,
            channel: 0,
            track_index: 0,
            program: 0,
            is_drum: false,
        }
    }

    fn make_part(notes: Vec<NoteObject>) -> Part {
        Part {
            key: PartKey {
                channel: 0,
                program: 0,
            },
            notes,
            is_drum: false,
            name: None,
        }
    }

    #[test]
    fn test_analyze_empty_part() {
        let part = make_part(vec![]);
        let f = analyze_part(&part, 480.0, 500_000, 1440);
        assert_eq!(f.note_count, 0);
        assert_eq!(f.monophony_ratio, 0.0);
    }

    #[test]
    fn test_monophonic_part() {
        // Three sequential non-overlapping notes → monophony_ratio == 1.0
        let part = make_part(vec![
            make_note(0, 480, 60, 100),
            make_note(480, 960, 62, 100),
            make_note(960, 1440, 64, 100),
        ]);
        let f = analyze_part(&part, 480.0, 500_000, 1440);
        assert_eq!(f.note_count, 3);
        assert!((f.monophony_ratio - 1.0).abs() < 0.01);
        assert!((f.chordiness - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_chordal_part() {
        // Three notes sounding simultaneously for 480 ticks
        let part = make_part(vec![
            make_note(0, 480, 60, 100),
            make_note(0, 480, 64, 100),
            make_note(0, 480, 67, 100),
        ]);
        let f = analyze_part(&part, 480.0, 500_000, 480);
        assert!(f.monophony_ratio < 0.01); // not monophonic
        assert!((f.chordiness - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_stepwise_motion() {
        // C→D→E: intervals of 2 semitones each
        let part = make_part(vec![
            make_note(0, 480, 60, 100),
            make_note(480, 960, 62, 100),
            make_note(960, 1440, 64, 100),
        ]);
        let f = analyze_part(&part, 480.0, 500_000, 1440);
        assert!((f.stepwise_motion - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_instrument_priors_bass() {
        let mut part = make_part(vec![make_note(0, 480, 40, 100)]);
        part.key.program = 33; // Electric Bass
        let f = analyze_part(&part, 480.0, 500_000, 480);
        assert!(f.bass_bias > 0.5);
        assert!(f.melody_bias < 0.1);
    }

    #[test]
    fn test_instrument_priors_lead_synth() {
        let mut part = make_part(vec![make_note(0, 480, 72, 100)]);
        part.key.program = 80; // Synth Lead
        let f = analyze_part(&part, 480.0, 500_000, 480);
        assert!(f.melody_bias > 0.5);
        assert!(f.bass_bias < 0.1);
    }
}
