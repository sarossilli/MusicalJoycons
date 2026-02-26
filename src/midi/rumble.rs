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

use std::collections::BTreeSet;
use std::time::Duration;

use midly::{Smf, TrackEventKind};
use thiserror::Error;

use super::parts::{normalize_to_parts, Part};
use super::scoring::{secondary_score, select_parts, PartSelection};
use super::track_analysis::{analyze_part, PartFeatures};
use super::track_types::{PlaybackPlan, SectionAssignment, TrackMetrics, TrackType};

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

const MIDI_A4_NOTE: i32 = 69;
const DEFAULT_TEMPO: u32 = 500_000; // 120 BPM
const SILENCE_THRESHOLD: Duration = Duration::from_millis(300);

const RUMBLE_FREQ_MIN: f32 = 400.0;
const RUMBLE_FREQ_MAX: f32 = 1252.0;

/// Convert a MIDI note number to a frequency within the JoyCon rumble range.
/// Notes below [`RUMBLE_FREQ_MIN`] are octave-shifted up; notes above
/// [`RUMBLE_FREQ_MAX`] are octave-shifted down.
fn note_to_frequency(note: i32) -> f32 {
    let mut freq = 440.0 * 2.0f32.powf((note - MIDI_A4_NOTE) as f32 / 12.0);
    while freq < RUMBLE_FREQ_MIN {
        freq *= 2.0;
    }
    while freq > RUMBLE_FREQ_MAX {
        freq *= 0.5;
    }
    freq
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
    let mut total_micros: f64 = 0.0;
    let mut current_tick = start_tick;
    let mut tempo_idx = 0;
    let tpb = ticks_per_beat as f64;

    while current_tick < end_tick && tempo_idx < tempo_changes.len() {
        let current_tempo = tempo_changes[tempo_idx].tempo as f64;
        let next_change_tick = if tempo_idx + 1 < tempo_changes.len() {
            tempo_changes[tempo_idx + 1].time
        } else {
            end_tick
        };

        let ticks_in_segment = (end_tick.min(next_change_tick) - current_tick) as f64;
        total_micros += ticks_in_segment * (current_tempo / tpb);

        current_tick = next_change_tick;
        tempo_idx += 1;
    }

    Duration::from_secs_f64(total_micros / 1_000_000.0)
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

/// Convert a [`Part`] (pre-resolved NoteObjects) into a [`RumbleTrack`].
fn convert_part_to_rumble(
    part: &Part,
    part_index: usize,
    features: &PartFeatures,
    tempo_changes: &[TempoChange],
    ticks_per_beat: f32,
) -> RumbleTrack {
    // Build a flat event list: (tick, is_on, pitch, velocity)
    let mut events: Vec<(u32, bool, u8, f32)> = Vec::with_capacity(part.notes.len() * 2);
    for n in &part.notes {
        events.push((n.start_tick, true, n.pitch, n.velocity as f32 / 127.0));
        events.push((n.end_tick, false, n.pitch, 0.0));
    }
    events.sort_by_key(|e| (e.0, !e.1)); // note-offs before note-ons at same tick

    let mut commands = Vec::new();
    let mut active_notes: Vec<(u8, f32)> = Vec::new();
    let mut current_tick = 0u32;

    for (tick, is_on, pitch, vel) in &events {
        if *tick > current_tick {
            let wait = ticks_to_duration(current_tick, *tick, tempo_changes, ticks_per_beat);
            if !wait.is_zero() {
                let (freq, amp) = if active_notes.is_empty() {
                    (0.0, 0.0)
                } else {
                    (
                        note_to_frequency(active_notes[0].0 as i32),
                        active_notes[0].1,
                    )
                };
                commands.push(RumbleCommand {
                    frequency: freq,
                    amplitude: amp,
                    wait_before: wait,
                });
            }
            current_tick = *tick;
        }

        if *is_on {
            active_notes.push((*pitch, *vel));
            let freq = note_to_frequency(*pitch as i32);
            commands.push(RumbleCommand {
                frequency: freq,
                amplitude: *vel,
                wait_before: Duration::ZERO,
            });
        } else {
            active_notes.retain(|(p, _)| *p != *pitch);
            let (freq, amp) = if active_notes.is_empty() {
                (0.0, 0.0)
            } else {
                (
                    note_to_frequency(active_notes[0].0 as i32),
                    active_notes[0].1,
                )
            };
            commands.push(RumbleCommand {
                frequency: freq,
                amplitude: amp,
                wait_before: Duration::ZERO,
            });
        }
    }

    // Ensure track ends with silence.
    if let Some(last) = commands.last() {
        if last.frequency != 0.0 || last.amplitude != 0.0 {
            commands.push(RumbleCommand {
                frequency: 0.0,
                amplitude: 0.0,
                wait_before: Duration::ZERO,
            });
        }
    }

    let final_tick = part.notes.iter().map(|n| n.end_tick).max().unwrap_or(0);
    let total_duration = ticks_to_duration(0, final_tick, tempo_changes, ticks_per_beat);

    let silent_periods = find_silent_periods(&commands);
    let switch_points = silent_periods
        .into_iter()
        .map(|(time, _)| TrackSwitchPoint {
            time,
            alternative_track_index: part_index,
        })
        .collect();

    // Build a minimal TrackMetrics from the PartFeatures for backward compat.
    let metrics = TrackMetrics {
        track_index: part_index,
        note_count: features.note_count,
        is_percussion: features.is_drum,
        total_duration: total_duration.as_secs_f32(),
        note_density: features.notes_per_sec,
        track_name: part.name.clone(),
        track_type: if features.is_drum {
            TrackType::Drums
        } else if features.bass_bias > 0.5 {
            TrackType::Bass
        } else if features.melody_bias > 0.3 {
            TrackType::Melody
        } else if features.accompaniment_bias > 0.3 {
            TrackType::Harmony
        } else {
            TrackType::Unknown
        },
        ..TrackMetrics::default()
    };

    RumbleTrack {
        commands,
        total_duration,
        switch_points,
        track_index: part_index,
        metrics,
    }
}

struct NoteEvent {
    tick: u32,
    track_idx: usize,
    pitch: u8,
    is_on: bool,
}

struct SkylinePeriod {
    start_tick: u32,
    end_tick: u32,
    track_idx: usize,
}

/// Builds a pre-computed playback plan using the skyline algorithm,
/// sourcing note events directly from [`Part`]s.
///
/// `candidate_parts` is a slice of `(part_index_into_all_parts, &Part)`
/// whose rumble-track index corresponds to position in `rumble_tracks`.
fn build_playback_plan_from_parts(
    candidate_parts: &[&Part],
    rumble_tracks: &[RumbleTrack],
    all_features: &[PartFeatures],
    candidate_feature_indices: &[usize],
    num_joycons: usize,
    ticks_per_beat: f32,
    tempo_changes: &[TempoChange],
) -> PlaybackPlan {
    const WINDOW_SECS: f32 = 4.0;
    const MIN_SECTION_SECS: f32 = 4.0;

    let mut events: Vec<NoteEvent> = Vec::new();
    let num_rumble_tracks = rumble_tracks.len();

    for (rumble_idx, part) in candidate_parts.iter().enumerate() {
        if part.is_drum {
            continue;
        }
        for n in &part.notes {
            events.push(NoteEvent {
                tick: n.start_tick,
                track_idx: rumble_idx,
                pitch: n.pitch,
                is_on: true,
            });
            events.push(NoteEvent {
                tick: n.end_tick,
                track_idx: rumble_idx,
                pitch: n.pitch,
                is_on: false,
            });
        }
    }

    events.sort_by_key(|e| e.tick);

    if events.is_empty() {
        return PlaybackPlan {
            sections: vec![SectionAssignment {
                start_time: Duration::ZERO,
                track_indices: vec![0; num_joycons],
            }],
        };
    }

    // 2. Walk through events tracking which track holds the skyline (highest note).
    let mut active_notes: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); num_rumble_tracks];
    let mut skyline_periods: Vec<SkylinePeriod> = Vec::new();
    let mut prev_tick: u32 = 0;

    for event in &events {
        if event.tick > prev_tick {
            let mut best_track = None;
            let mut best_pitch = 0u8;
            for (idx, notes) in active_notes.iter().enumerate() {
                if let Some(&highest) = notes.iter().next_back() {
                    if highest > best_pitch {
                        best_pitch = highest;
                        best_track = Some(idx);
                    }
                }
            }
            if let Some(winner) = best_track {
                skyline_periods.push(SkylinePeriod {
                    start_tick: prev_tick,
                    end_tick: event.tick,
                    track_idx: winner,
                });
            }
        }

        if event.is_on {
            active_notes[event.track_idx].insert(event.pitch);
        } else {
            active_notes[event.track_idx].remove(&event.pitch);
        }
        prev_tick = event.tick;
    }

    // 3. Convert skyline periods to the time domain and aggregate into windows.
    let total_ticks = events.last().map(|e| e.tick).unwrap_or(0);
    let total_duration = ticks_to_duration(0, total_ticks, tempo_changes, ticks_per_beat);
    let window_duration = Duration::from_secs_f32(WINDOW_SECS);

    struct TimedPeriod {
        start: Duration,
        end: Duration,
        track_idx: usize,
    }
    let timed_periods: Vec<TimedPeriod> = skyline_periods
        .iter()
        .map(|p| TimedPeriod {
            start: ticks_to_duration(0, p.start_tick, tempo_changes, ticks_per_beat),
            end: ticks_to_duration(0, p.end_tick, tempo_changes, ticks_per_beat),
            track_idx: p.track_idx,
        })
        .collect();

    let mut window_melodies: Vec<(Duration, usize)> = Vec::new();
    let mut window_start = Duration::ZERO;

    while window_start < total_duration {
        let window_end = (window_start + window_duration).min(total_duration);

        let mut track_durations: Vec<Duration> = vec![Duration::ZERO; num_rumble_tracks];
        for period in &timed_periods {
            let overlap_start = period.start.max(window_start);
            let overlap_end = period.end.min(window_end);
            if overlap_start < overlap_end {
                track_durations[period.track_idx] += overlap_end - overlap_start;
            }
        }

        let melody_idx = track_durations
            .iter()
            .enumerate()
            .max_by_key(|(_, &d)| d)
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        window_melodies.push((window_start, melody_idx));
        window_start = window_end;
    }

    // 4. Merge consecutive windows with the same melody winner into sections.
    let mut raw_sections: Vec<(Duration, usize)> = Vec::new();
    for &(time, melody) in &window_melodies {
        if let Some(last) = raw_sections.last() {
            if last.1 == melody {
                continue;
            }
        }
        raw_sections.push((time, melody));
    }

    // Apply hysteresis: absorb sections shorter than MIN_SECTION_SECS
    // into the previous section.
    let min_section = Duration::from_secs_f32(MIN_SECTION_SECS);
    let mut stable_sections: Vec<(Duration, usize)> = Vec::new();

    for i in 0..raw_sections.len() {
        let section_end = if i + 1 < raw_sections.len() {
            raw_sections[i + 1].0
        } else {
            total_duration
        };
        let section_duration = section_end.saturating_sub(raw_sections[i].0);

        if section_duration >= min_section || stable_sections.is_empty() {
            stable_sections.push(raw_sections[i]);
        }
    }

    // 5. For each section, assign melody + complement tracks using SecondaryScore.
    let sections: Vec<SectionAssignment> = stable_sections
        .iter()
        .map(|&(start_time, melody_idx)| {
            let mut track_indices = vec![melody_idx];

            for _ in 1..num_joycons {
                let melody_feat_idx = candidate_feature_indices
                    .get(melody_idx)
                    .copied()
                    .unwrap_or(0);
                let primary_feat = &all_features[melody_feat_idx];

                let complement = candidate_feature_indices
                    .iter()
                    .enumerate()
                    .filter(|(ri, _)| !track_indices.contains(ri))
                    .filter(|(_, &fi)| !all_features[fi].is_drum && all_features[fi].note_count > 0)
                    .max_by(|(_, &fi_a), (_, &fi_b)| {
                        let sa = secondary_score(&all_features[fi_a], primary_feat, all_features);
                        let sb = secondary_score(&all_features[fi_b], primary_feat, all_features);
                        sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(ri, _)| ri)
                    .unwrap_or(melody_idx);
                track_indices.push(complement);
            }

            SectionAssignment {
                start_time,
                track_indices,
            }
        })
        .collect();

    println!("\n🎼 Playback Plan ({} sections):", sections.len());
    for (i, section) in sections.iter().enumerate() {
        let end = if i + 1 < sections.len() {
            sections[i + 1].start_time
        } else {
            total_duration
        };
        let part_names: Vec<String> = section
            .track_indices
            .iter()
            .map(|&idx| {
                candidate_parts
                    .get(idx)
                    .and_then(|p| p.name.as_deref())
                    .unwrap_or("unnamed")
                    .to_string()
            })
            .collect();
        println!(
            "  Section {}: {:.1?} - {:.1?} → parts {:?} ({:?})",
            i + 1,
            section.start_time,
            end,
            section.track_indices,
            part_names
        );
    }

    PlaybackPlan { sections }
}

/// Parses MIDI data and converts it to rumble tracks with a playback plan.
///
/// This is the main entry point for MIDI-to-rumble conversion. It handles
/// parsing, part-based normalization and scoring, rumble conversion, and
/// builds a [`PlaybackPlan`] using the skyline algorithm constrained to the
/// top-scoring candidate parts.
///
/// # Arguments
///
/// * `midi_data` - Raw bytes of a MIDI file
/// * `num_joycons` - Number of JoyCons that will play simultaneously
///
/// # Returns
///
/// A tuple of:
/// - All converted [`RumbleTrack`]s (one per candidate part)
/// - A [`PlaybackPlan`] with per-section track assignments
/// - A [`PartSelection`] identifying primary/secondary and ranked candidates
///
/// # Errors
///
/// - [`ParseError::MidiError`] - Invalid MIDI file format
/// - [`ParseError::NoTracks`] - No playable parts found
pub fn parse_midi_to_rumble(
    midi_data: &[u8],
    num_joycons: usize,
) -> Result<(Vec<RumbleTrack>, PlaybackPlan, PartSelection), ParseError> {
    let smf = Smf::parse(midi_data)?;

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(timing) => timing.as_int() as f32,
        _ => 24.0,
    };

    let tempo_changes = collect_tempo_changes(&smf);

    // --- Part-based normalization ---
    let parts = normalize_to_parts(&smf);
    println!("\n🎵 Normalized into {} parts", parts.len());

    let song_end_tick = parts
        .iter()
        .flat_map(|p| p.notes.iter())
        .map(|n| n.end_tick)
        .max()
        .unwrap_or(0);

    let all_features: Vec<PartFeatures> = parts
        .iter()
        .map(|p| analyze_part(p, ticks_per_beat, DEFAULT_TEMPO, song_end_tick))
        .collect();

    for (i, (part, feat)) in parts.iter().zip(all_features.iter()).enumerate() {
        println!(
            "  Part {} (ch={}, prog={}): {} notes, active={:.2}, mono={:.2}, chord={:.1}, p75={:.0}, drum={}{}",
            i,
            part.key.channel,
            part.key.program,
            feat.note_count,
            feat.active_ratio,
            feat.monophony_ratio,
            feat.chordiness,
            feat.p75_pitch,
            feat.is_drum,
            part.name
                .as_ref()
                .map(|n| format!(", name={n}"))
                .unwrap_or_default()
        );
    }

    // --- Score & select primary / secondary ---
    let selection = select_parts(&all_features).ok_or(ParseError::NoTracks)?;

    println!(
        "\n🎯 Selected primary=Part {} ({}), secondary=Part {} ({})",
        selection.primary,
        parts[selection.primary]
            .name
            .as_deref()
            .unwrap_or("unnamed"),
        selection.secondary,
        parts[selection.secondary]
            .name
            .as_deref()
            .unwrap_or("unnamed"),
    );

    // Build candidate pool: primary + secondary + next few from primary_candidates.
    const MAX_CANDIDATES: usize = 6;
    let mut candidate_indices: Vec<usize> = Vec::new();
    candidate_indices.push(selection.primary);
    if selection.secondary != selection.primary {
        candidate_indices.push(selection.secondary);
    }
    for &idx in &selection.primary_candidates {
        if candidate_indices.len() >= MAX_CANDIDATES {
            break;
        }
        if !candidate_indices.contains(&idx) {
            candidate_indices.push(idx);
        }
    }

    // Convert candidate parts to RumbleTracks.
    let candidate_parts: Vec<&Part> = candidate_indices.iter().map(|&i| &parts[i]).collect();
    let mut rumble_tracks: Vec<RumbleTrack> = candidate_indices
        .iter()
        .enumerate()
        .map(|(rumble_idx, &part_idx)| {
            convert_part_to_rumble(
                &parts[part_idx],
                rumble_idx,
                &all_features[part_idx],
                &tempo_changes,
                ticks_per_beat,
            )
        })
        .collect();

    // Normalize amplitudes across all rumble tracks.
    let max_amp = rumble_tracks
        .iter()
        .flat_map(|t| t.commands.iter())
        .map(|c| c.amplitude)
        .fold(1.0f32, f32::max);
    if max_amp > 1.0 {
        for track in &mut rumble_tracks {
            for cmd in &mut track.commands {
                cmd.amplitude /= max_amp;
            }
        }
    }

    // Remap PartSelection indices from all-parts space to rumble-tracks space.
    let remap = |orig: usize| -> usize {
        candidate_indices
            .iter()
            .position(|&i| i == orig)
            .unwrap_or(0)
    };
    let remapped_selection = PartSelection {
        primary: remap(selection.primary),
        secondary: remap(selection.secondary),
        primary_candidates: selection
            .primary_candidates
            .iter()
            .filter_map(|&i| candidate_indices.iter().position(|&c| c == i))
            .collect(),
        secondary_candidates: selection
            .secondary_candidates
            .iter()
            .filter_map(|&i| candidate_indices.iter().position(|&c| c == i))
            .collect(),
    };

    // Build the playback plan using skyline, constrained to the candidate pool.
    let plan = build_playback_plan_from_parts(
        &candidate_parts,
        &rumble_tracks,
        &all_features,
        &candidate_indices,
        num_joycons,
        ticks_per_beat,
        &tempo_changes,
    );

    Ok((rumble_tracks, plan, remapped_selection))
}
