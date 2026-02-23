//! MIDI to rumble command conversion.
//!
//! This module handles converting MIDI data into sequences of rumble commands
//! that can be played on JoyCon devices. It includes the core data structures
//! for rumble playback and the parsing logic for MIDI files.
//!
//! # Key Types
//!
//! - [`RumbleCommand`]: A single frequency/amplitude/timing instruction
//! - [`RumbleTrack`]: A complete sequence of commands for one track
//! - [`TrackMergeController`]: Coordinates multi-track playback
//!
//! # Conversion Process
//!
//! 1. Parse MIDI file using the `midly` crate
//! 2. Extract tempo changes for accurate timing
//! 3. Convert each track's note events to rumble commands
//! 4. Identify silent periods for potential track switching
//! 5. Normalize amplitudes across all tracks

use std::time::Duration;

use midly::{Smf, Track, TrackEventKind};
use thiserror::Error;

use super::track_analysis::analyze_track;
use super::track_types::TrackMetrics;

/// A single rumble command to send to a JoyCon.
///
/// Each command specifies a frequency, amplitude, and the time to wait
/// before executing. Commands with `frequency = 0.0` and `amplitude = 0.0`
/// represent silence (stopping the rumble motor).
///
/// # Example
///
/// ```
/// use std::time::Duration;
/// use musical_joycons::midi::RumbleCommand;
///
/// let command = RumbleCommand {
///     frequency: 440.0,           // A4 note
///     amplitude: 0.8,             // 80% volume
///     wait_before: Duration::from_millis(100),  // Wait 100ms first
/// };
/// ```
#[derive(Debug, Clone)]
pub struct RumbleCommand {
    /// Frequency in Hz. Use `0.0` for silence.
    ///
    /// Common musical frequencies:
    /// - C4 (Middle C): 261.63 Hz
    /// - A4 (Concert Pitch): 440.0 Hz
    /// - C5: 523.25 Hz
    pub frequency: f32,

    /// Amplitude from `0.0` (silent) to `1.0` (maximum).
    ///
    /// Values are typically normalized from MIDI velocity (0-127).
    pub amplitude: f32,

    /// Time to wait before executing this command.
    ///
    /// This creates the timing between notes. A value of `Duration::ZERO`
    /// means the command executes immediately after the previous one.
    pub wait_before: Duration,
}

/// A point in time where track switching may occur.
///
/// During playback, JoyCons can switch from one track to another during
/// silent periods. This struct identifies when such switches are possible
/// and which track to switch to.
#[derive(Debug, Clone)]
pub struct TrackSwitchPoint {
    /// Time offset from the start of the track where switching is possible.
    pub time: Duration,

    /// Index of the recommended alternative track to switch to.
    ///
    /// This is determined based on which tracks are most active at this time.
    pub alternative_track_index: usize,
}

/// A sequence of rumble commands representing a single MIDI track.
///
/// This is the primary data structure for MIDI playback. It contains all
/// the rumble commands needed to play a track, along with metadata about
/// the track's characteristics and potential switch points.
///
/// # Creating a RumbleTrack
///
/// Tracks are created by `parse_midi_to_rumble()`, which converts raw
/// MIDI data into playable rumble tracks:
///
/// ```no_run
/// use musical_joycons::midi::RumbleTrack;
///
/// // Tracks are typically created via parse_midi_to_rumble
/// // let tracks = parse_midi_to_rumble(&midi_data, vec![])?;
/// ```
#[derive(Debug, Clone)]
pub struct RumbleTrack {
    /// The sequence of rumble commands to execute.
    ///
    /// Commands are in chronological order. Execute them in sequence,
    /// respecting each command's `wait_before` duration.
    pub commands: Vec<RumbleCommand>,

    /// Total duration of the track from first to last note.
    pub total_duration: Duration,

    /// Points where track switching is possible (during silent periods).
    ///
    /// These are identified automatically by looking for gaps in note activity.
    pub switch_points: Vec<TrackSwitchPoint>,

    /// The original index of this track in the MIDI file.
    ///
    /// Useful for debugging and for correlating with track names.
    pub track_index: usize,

    /// Analysis metrics for this track (note count, density, etc.).
    ///
    /// Used for scoring and track type identification.
    pub metrics: TrackMetrics,
}

/// Errors that can occur during MIDI parsing and conversion.
///
/// This error type covers all failure modes from file I/O to MIDI format issues.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Failed to read the MIDI file from disk.
    ///
    /// The inner error contains the I/O error details.
    #[error("Failed to read MIDI file: {0}")]
    FileError(#[from] std::io::Error),

    /// Failed to parse the MIDI file format.
    ///
    /// This can occur if the file is corrupted, not a valid MIDI file,
    /// or uses unsupported MIDI features.
    #[error("Failed to parse MIDI file: {0}")]
    MidiError(#[from] midly::Error),

    /// The MIDI file contains no playable tracks.
    ///
    /// This happens when all tracks are either empty, contain only
    /// percussion, or have no note events.
    #[error("No tracks found")]
    NoTracks,
}

#[derive(Debug, Clone)]
struct TempoChange {
    time: u32,  // In ticks
    tempo: u32, // In microseconds per beat
}

const BASE_FREQUENCY: f32 = 880.0; // A5 note frequency (up one octave from A4)
const MIDI_A4_NOTE: i32 = 69; // MIDI note number for A4
const DEFAULT_TEMPO: u32 = 500_000; // Default tempo (120 BPM)
const SILENCE_THRESHOLD: Duration = Duration::from_millis(300); // Reduced from 500ms

fn note_to_frequency(note: i32) -> f32 {
    BASE_FREQUENCY * 2.0f32.powf((note - MIDI_A4_NOTE - 12) as f32 / 12.0)
}

fn collect_tempo_changes(smf: &Smf) -> Vec<TempoChange> {
    let mut tempo_changes = Vec::new();
    tempo_changes.push(TempoChange {
        time: 0,
        tempo: DEFAULT_TEMPO,
    });

    for track in smf.tracks.iter() {
        let mut current_time = 0;
        for event in track.iter() {
            current_time += event.delta.as_int();
            if let TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)) = event.kind {
                tempo_changes.push(TempoChange {
                    time: current_time,
                    tempo: tempo.as_int(),
                });
            }
        }
    }

    tempo_changes.sort_by_key(|tc| tc.time);
    tempo_changes.dedup_by_key(|tc| tc.time);
    tempo_changes
}

fn ticks_to_duration(
    start_tick: u32,
    end_tick: u32,
    tempo_changes: &[TempoChange],
    ticks_per_beat: f32,
) -> Duration {
    let mut total_micros = 0.0;
    let mut current_tick = start_tick;
    let mut tempo_idx = 0;

    while current_tick < end_tick && tempo_idx < tempo_changes.len() {
        let current_tempo = tempo_changes[tempo_idx].tempo as f32;
        let next_change_tick = if tempo_idx + 1 < tempo_changes.len() {
            tempo_changes[tempo_idx + 1].time
        } else {
            end_tick
        };

        let ticks_in_segment = (end_tick.min(next_change_tick) - current_tick) as f32;
        let micros_per_tick = current_tempo / ticks_per_beat;
        total_micros += ticks_in_segment * micros_per_tick;

        current_tick = next_change_tick;
        tempo_idx += 1;
    }

    Duration::from_micros(total_micros as u64)
}

fn find_silent_periods(commands: &[RumbleCommand]) -> Vec<(Duration, Duration)> {
    let mut silent_periods = Vec::new();
    let mut silence_start: Option<Duration> = None;
    let mut current_time = Duration::ZERO;
    let mut had_non_zero_amplitude = false;

    // Check if track starts with silence
    if !commands.is_empty() && commands[0].amplitude == 0.0 {
        silence_start = Some(Duration::ZERO);
    }

    for cmd in commands {
        current_time += cmd.wait_before;

        if cmd.amplitude > 0.0 {
            had_non_zero_amplitude = true;
            if let Some(start) = silence_start {
                if current_time - start >= SILENCE_THRESHOLD {
                    silent_periods.push((start, current_time));
                }
                silence_start = None;
            }
        } else if silence_start.is_none() {
            silence_start = Some(current_time);
        }
    }

    // Handle case where track ends in silence or never had any notes
    if let Some(start) = silence_start {
        if had_non_zero_amplitude || start == Duration::ZERO {
            silent_periods.push((start, current_time));
        }
    }

    silent_periods
}

fn convert_track_with_tempo(
    track: &Track,
    tempo_changes: &[TempoChange],
    ticks_per_beat: f32,
    track_index: usize,
    metrics: TrackMetrics,  // Add metrics parameter
) -> Result<RumbleTrack, ParseError> {
    let mut commands = Vec::new();
    let mut current_time_ticks = 0u32;
    let mut active_notes: Vec<(u8, f32)> = Vec::new();
    let mut last_event_had_wait = false;

    println!("Converting track {}: {} notes", track_index, metrics.note_count);
    
    for event in track.iter() {
        let delta_ticks = event.delta.as_int();
        if delta_ticks > 0 {
            let wait_duration = ticks_to_duration(
                current_time_ticks,
                current_time_ticks + delta_ticks,
                tempo_changes,
                ticks_per_beat,
            );

            if !wait_duration.is_zero() {
                let (frequency, amplitude) = if active_notes.is_empty() {
                    (0.0, 0.0)
                } else {
                    (
                        note_to_frequency(active_notes[0].0 as i32),
                        active_notes[0].1,
                    )
                };

                commands.push(RumbleCommand {
                    frequency,
                    amplitude,
                    wait_before: wait_duration,
                });
                last_event_had_wait = true;
            }
        }

        current_time_ticks += delta_ticks;

        if let TrackEventKind::Midi { message, .. } = event.kind { match message {
            midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                let frequency = note_to_frequency(i32::from(key.as_int()));
                // Scale amplitude to full range (0.0-1.0)
                let amplitude = f32::from(vel.as_int()) / 127.0;
                
                println!("  Note On: key={}, freq={:.1}, amp={:.2}", key.as_int(), frequency, amplitude);
                active_notes.push((key.as_int(), amplitude));

                if !last_event_had_wait || !commands.is_empty() {
                    commands.push(RumbleCommand {
                        frequency,
                        amplitude,
                        wait_before: Duration::ZERO,
                    });
                } else if let Some(last_command) = commands.last_mut() {
                    last_command.frequency = frequency;
                    last_command.amplitude = amplitude;
                }
                last_event_had_wait = false;
            }
            midly::MidiMessage::NoteOff { key, vel: _ } => {
                active_notes.retain(|(note, _)| *note != key.as_int());
                let (frequency, amplitude) = if active_notes.is_empty() {
                    (0.0, 0.0)
                } else {
                    (
                        note_to_frequency(active_notes[0].0 as i32),
                        active_notes[0].1,
                    )
                };

                if !last_event_had_wait {
                    commands.push(RumbleCommand {
                        frequency,
                        amplitude,
                        wait_before: Duration::ZERO,
                    });
                } else if let Some(last_command) = commands.last_mut() {
                    last_command.frequency = frequency;
                    last_command.amplitude = amplitude;
                }
                last_event_had_wait = false;
            }
            _ => {}
        } }
    }
    let final_duration = ticks_to_duration(0, current_time_ticks, tempo_changes, ticks_per_beat);

    if let Some(last) = commands.last() {
        if last.frequency != 0.0 || last.amplitude != 0.0 {
            commands.push(RumbleCommand {
                frequency: 0.0,
                amplitude: 0.0,
                wait_before: Duration::ZERO,
            });
        }
    }

    let silent_periods = find_silent_periods(&commands);
    let switch_points = silent_periods
        .into_iter()
        .map(|(time, _)| TrackSwitchPoint {
            time,
            alternative_track_index: track_index,  // Will be updated later
        })
        .collect();

    println!("Track {} converted: {} commands", track_index, commands.len());
    
    Ok(RumbleTrack {
        commands,
        total_duration: final_duration,
        switch_points,
        track_index,
        metrics,  // Include metrics in struct initialization
    })
}

// Update the is_track_active_at_time function to look ahead a bit
fn find_commands_at_time(commands: &[RumbleCommand], time: Duration) -> usize {
    let mut current_time = Duration::ZERO;
    for (i, cmd) in commands.iter().enumerate() {
        current_time += cmd.wait_before;
        if current_time >= time {
            return i;
        }
    }
    commands.len()
}

/// Controller for coordinating multi-track playback with dynamic track switching.
///
/// The `TrackMergeController` manages playback across multiple JoyCons, deciding
/// when and where to switch tracks based on musical activity. It aims to keep
/// each JoyCon playing the most interesting content available.
///
/// # Track Switching Logic
///
/// The controller switches tracks when:
/// 1. The current track enters a silent period
/// 2. Another track has significantly more activity
/// 3. Enough time has passed since the last switch (minimum 2 seconds)
/// 4. The switch occurs at a musically appropriate moment (note-off)
///
/// # Example Usage
///
/// ```no_run
/// # use musical_joycons::midi::{RumbleTrack, TrackMergeController};
/// # fn example(tracks: Vec<RumbleTrack>) {
/// let num_joycons = 2;
/// let controller = TrackMergeController::new(tracks, num_joycons);
///
/// // During playback, check if a switch should occur
/// // let should_switch = controller.should_switch_tracks(0, current_track, target_track, cmd_idx);
/// # }
/// ```
#[derive(Debug)]
pub struct TrackMergeController {
    /// All available tracks for playback
    pub tracks: Vec<RumbleTrack>,

    /// Current playback time position
    current_time: Duration,

    /// Last switch time for each JoyCon (to enforce minimum interval)
    last_switch_times: Vec<Duration>,
}

impl TrackMergeController {
    /// Size of the look-ahead window for evaluating track activity (2 seconds).
    ///
    /// When deciding whether to switch tracks, the controller looks at how many
    /// notes each track will play in the next 2 seconds.
    pub const FUTURE_WINDOW_SIZE: Duration = Duration::from_secs(2);

    /// Creates a new TrackMergeController.
    ///
    /// # Arguments
    ///
    /// * `tracks` - All available rumble tracks from the MIDI file
    /// * `num_joycons` - Number of JoyCons that will be playing
    pub fn new(tracks: Vec<RumbleTrack>, num_joycons: usize) -> Self {
        Self {
            tracks,
            current_time: Duration::ZERO,
            last_switch_times: vec![Duration::ZERO; num_joycons],
        }
    }

    /// Evaluates a track section's activity level.
    ///
    /// Counts notes and finds maximum amplitude within a time window.
    ///
    /// # Arguments
    ///
    /// * `commands` - The rumble commands to analyze
    /// * `start_time` - Start of the evaluation window
    /// * `window_size` - Duration of the evaluation window
    ///
    /// # Returns
    ///
    /// A tuple of `(note_count, max_amplitude)` within the window.
    pub fn evaluate_track_section(
        commands: &[RumbleCommand],
        start_time: Duration,
        window_size: Duration
    ) -> (usize, f32) {
        let mut note_count = 0;
        let mut max_amplitude: f32 = 0.0;
        let mut current_time = Duration::ZERO;
        let end_time = start_time + window_size;

        for cmd in commands {
            current_time += cmd.wait_before;
            if current_time >= start_time && current_time <= end_time && cmd.amplitude > 0.0 {
                note_count += 1;
                max_amplitude = max_amplitude.max(cmd.amplitude);
            }
            if current_time > end_time {
                break;
            }
        }
        (note_count, max_amplitude)
    }

    /// Determines whether a JoyCon should switch from its current track to a new one.
    ///
    /// This method implements the track switching heuristics, considering:
    /// - Time since last switch (minimum 2 seconds)
    /// - Note activity in both tracks for the next 2 seconds
    /// - Track scores and quality metrics
    ///
    /// # Arguments
    ///
    /// * `joycon_idx` - Index of the JoyCon considering the switch
    /// * `current_track_idx` - Index of the track currently being played
    /// * `target_track_idx` - Index of the proposed new track
    /// * `command_index` - Current position in the track's command list
    ///
    /// # Returns
    ///
    /// `true` if the switch should occur, `false` otherwise.
    ///
    /// # Switching Criteria
    ///
    /// A switch is recommended when:
    /// 1. At least 2 seconds have passed since the last switch
    /// 2. The target track has significantly more notes coming up
    /// 3. The target track's maximum amplitude is at least 0.3
    /// 4. The target track's score is at least 1.5x the current track's score
    pub fn should_switch_tracks(
        &self,
        joycon_idx: usize,
        current_track_idx: usize,
        target_track_idx: usize,
        command_index: usize,
    ) -> bool {
        const MIN_SWITCH_INTERVAL: Duration = Duration::from_secs(2);
        const MIN_NOTE_COUNT_FOR_SWITCH: usize = 3;
        const MIN_SCORE_DIFF_FOR_SWITCH: f32 = 1.5;

        // Check if enough time has passed since last switch
        if self.current_time.saturating_sub(self.last_switch_times[joycon_idx]) < MIN_SWITCH_INTERVAL {
            return false;
        }

        let current_track = &self.tracks[current_track_idx];
        let target_track = &self.tracks[target_track_idx];

        // Evaluate both current and target tracks
        let (current_notes, _current_max_amp) = Self::evaluate_track_section(
            &current_track.commands[command_index..],
            self.current_time,
            Self::FUTURE_WINDOW_SIZE
        );
        
        let target_start_idx = find_commands_at_time(&target_track.commands, self.current_time);
        let (target_notes, target_max_amp) = Self::evaluate_track_section(
            &target_track.commands[target_start_idx..],
            self.current_time,
            Self::FUTURE_WINDOW_SIZE
        );

        // Compare track scores and activity
        let activity_check = 
            (target_notes > current_notes + 1 || 
            (current_notes == 0 && target_notes >= MIN_NOTE_COUNT_FOR_SWITCH)) &&
            target_max_amp >= 0.3;

        if activity_check {
            let current_score = current_track.metrics.calculate_score();
            let target_score = target_track.metrics.calculate_score();
            target_score > current_score * MIN_SCORE_DIFF_FOR_SWITCH
        } else {
            false
        }
    }

    /// Records that a track switch occurred for the specified JoyCon.
    ///
    /// This updates the last switch time, which is used to enforce the
    /// minimum interval between switches.
    ///
    /// # Arguments
    ///
    /// * `joycon_idx` - Index of the JoyCon that performed the switch
    pub fn record_switch(&mut self, joycon_idx: usize) {
        self.last_switch_times[joycon_idx] = self.current_time;
    }

    /// Updates the controller's current time position.
    ///
    /// Call this as playback progresses to keep the controller's time
    /// synchronized with actual playback.
    ///
    /// # Arguments
    ///
    /// * `delta` - Amount of time that has elapsed
    pub fn update_time(&mut self, delta: Duration) {
        self.current_time += delta;
    }
}

/// Parses MIDI data and converts it to rumble tracks.
///
/// This is the main entry point for MIDI-to-rumble conversion. It handles
/// the complete process of parsing, track selection, and conversion.
///
/// # Arguments
///
/// * `midi_data` - Raw bytes of a MIDI file
/// * `track_selections` - Optional manual track selection. Pass an empty `Vec`
///   or a `Vec` of `None` values for automatic selection.
///
/// # Track Selection
///
/// If `track_selections` is empty or contains only `None` values, the function
/// automatically selects tracks based on their scores. Tracks are scored on:
/// - Note density (notes per second)
/// - Melodic movement
/// - Pitch range
/// - Velocity variance
/// - Track type (melody > harmony > bass > drums)
///
/// Percussion tracks (MIDI channel 10) are excluded from automatic selection.
///
/// # Returns
///
/// A `Vec` of [`RumbleTrack`] objects, sorted by score (best first).
///
/// # Errors
///
/// - [`ParseError::MidiError`] - Invalid MIDI file format
/// - [`ParseError::NoTracks`] - No playable tracks found
///
/// # Example
///
/// ```no_run
/// use musical_joycons::midi::parse_midi_to_rumble;
///
/// let midi_data = std::fs::read("song.mid")?;
///
/// // Automatic track selection
/// let tracks = parse_midi_to_rumble(&midi_data, vec![])?;
///
/// // Manual track selection (use tracks 0 and 2)
/// let tracks = parse_midi_to_rumble(&midi_data, vec![Some(0), Some(2)])?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_midi_to_rumble(
    midi_data: &[u8],
    track_selections: Vec<Option<usize>>,
) -> Result<Vec<RumbleTrack>, ParseError> {
    let smf = Smf::parse(midi_data)?;
    let _num_joycons = track_selections.len();

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(timing) => timing.as_int() as f32,
        _ => 24.0,
    };

    let tempo_changes = collect_tempo_changes(&smf);
    let mut track_metrics: Vec<TrackMetrics> = smf
        .tracks
        .iter()
        .enumerate()
        .map(|(idx, track)| {
            let mut metrics = analyze_track(track, ticks_per_beat, DEFAULT_TEMPO);
            metrics.track_index = idx;
            metrics
        })
        .collect();

    println!("\n🎵 Found {} tracks in MIDI file", track_metrics.len());

    let selected_tracks: Vec<&TrackMetrics> = if track_selections.is_empty() || track_selections.iter().all(Option::is_none) {
        // Sort all tracks by score
        track_metrics.sort_by(|a, b| {
            b.calculate_score()
                .partial_cmp(&a.calculate_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take all valid tracks (not just num_joycons)
        track_metrics
            .iter()
            .filter(|m| !m.is_percussion && m.note_count > 0)
            .collect()
    } else {
        // Handle manual selection
        track_selections
            .iter()
            .filter_map(|&selection| selection.and_then(|idx| track_metrics.get(idx)))
            .collect()
    };

    if selected_tracks.is_empty() {
        return Err(ParseError::NoTracks);
    }

    let mut rumble_tracks: Vec<RumbleTrack> = selected_tracks
        .iter()
        .map(|metrics| {
            convert_track_with_tempo(
                &smf.tracks[metrics.track_index],
                &tempo_changes,
                ticks_per_beat,
                metrics.track_index,
                (*metrics).clone(),  // Dereference and clone the metrics
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    // First, find the global maximum velocity
    let mut max_velocity = 1.0f32;
    for track in smf.tracks.iter() {
        for event in track.iter() {
            if let TrackEventKind::Midi { message, .. } = event.kind {
                if let midly::MidiMessage::NoteOn { vel, .. } = message {
                    max_velocity = max_velocity.max(vel.as_int() as f32 / 127.0);
                }
            }
        }
    }

    // Normalize all amplitudes in the tracks
    for track in &mut rumble_tracks {
        for cmd in &mut track.commands {
            cmd.amplitude /= max_velocity;
        }
    }

    // Update switch points with multiple alternative tracks
    for i in 0..rumble_tracks.len() {
        let current_track_idx = rumble_tracks[i].track_index;
        println!("🔍 Finding alternative tracks for track {}", current_track_idx + 1);
        
        // First, collect all potential alternative tracks and their activity periods
        let alternative_tracks: Vec<(usize, f32, Vec<(Duration, bool)>)> = track_metrics
            .iter()
            .enumerate()
            .filter(|(idx, m)| {
                !m.is_percussion 
                && m.note_count > 0 
                && *idx != current_track_idx  // Don't include current track
            })
            .map(|(idx, m)| {
                let score = m.calculate_score();
                println!("  - Track {} score: {:.2}", idx + 1, score);
                
                // For each track, create an activity map
                let activity = if let Some(track) = rumble_tracks.iter().find(|t| t.track_index == idx) {
                    let mut activity_periods = Vec::new();
                    let mut current_time = Duration::ZERO;
                    for cmd in &track.commands {
                        current_time += cmd.wait_before;
                        activity_periods.push((current_time, cmd.amplitude > 0.0));
                    }
                    activity_periods
                } else {
                    Vec::new()
                };
                
                (idx, score, activity)
            })
            .collect();

        // Now update switch points
        for switch_point in &mut rumble_tracks[i].switch_points {
            let mut available_tracks: Vec<(usize, f32)> = alternative_tracks
                .iter()
                .filter(|(_idx, _score, activity)| {
                    // Find tracks that will be active at or soon after the switch point
                    let mut is_active = false;
                    let look_ahead = Duration::from_millis(500);

                    for (time, active) in activity.iter() {
                        if *time >= switch_point.time && *time <= switch_point.time + look_ahead && *active {
                            is_active = true;
                            break;
                        }
                    }
                    is_active
                })
                .map(|(idx, score, _)| (*idx, *score))
                .collect();

            // Sort available tracks by score
            available_tracks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            if let Some((best_idx, score)) = available_tracks.first() {
                println!("  ✨ At {:?}: Best alternative for track {} is track {} (score: {:.2})", 
                    switch_point.time,
                    current_track_idx + 1,
                    best_idx + 1,
                    score
                );
                switch_point.alternative_track_index = *best_idx;
            } else {
                println!("  ⚠️ No active alternatives found for track {} at {:?}", 
                    current_track_idx + 1,
                    switch_point.time
                );
            }
        }
    }

    Ok(rumble_tracks)
}
