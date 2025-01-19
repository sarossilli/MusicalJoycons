use crate::midi::{track_analysis::analyze_track, track_types::TrackMetrics};
use midly::{Smf, Track, TrackEventKind};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct RumbleCommand {
    pub frequency: f32,
    pub amplitude: f32,
    pub wait_before: Duration,
}

#[derive(Debug, Clone)]
pub struct RumbleTrack {
    pub commands: Vec<RumbleCommand>,
    pub total_duration: Duration,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Failed to read MIDI file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to parse MIDI file: {0}")]
    MidiError(#[from] midly::Error),
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
const BASE_AMPLITUDE: f32 = 1.0; // Base amplitude for notes

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
            current_time += event.delta.as_int() as u32;
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
// In rubmle.rs
fn convert_track_with_tempo(
    track: &Track,
    tempo_changes: &[TempoChange],
    ticks_per_beat: f32,
) -> Result<RumbleTrack, ParseError> {
    let mut commands = Vec::new();
    let mut current_time_ticks = 0u32;
    let mut active_notes: Vec<(u8, f32)> = Vec::new();
    let mut last_event_had_wait = false;

    for event in track.iter() {
        let delta_ticks = event.delta.as_int() as u32;
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

        match event.kind {
            TrackEventKind::Midi { message, .. } => match message {
                midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                    let frequency = note_to_frequency(i32::from(key.as_int()));
                    let amplitude = (f32::from(vel.as_int()) / 127.0) * BASE_AMPLITUDE;
                    active_notes.push((key.as_int(), amplitude));

                    if (!last_event_had_wait || !commands.is_empty()) {
                        commands.push(RumbleCommand {
                            frequency,
                            amplitude,
                            wait_before: Duration::ZERO,
                        });
                    } else {
                        if let Some(last_command) = commands.last_mut() {
                            last_command.frequency = frequency;
                            last_command.amplitude = amplitude;
                        }
                    }
                    last_event_had_wait = false;
                }
                midly::MidiMessage::NoteOff { key, vel } => {
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
                    } else {
                        if let Some(last_command) = commands.last_mut() {
                            last_command.frequency = frequency;
                            last_command.amplitude = amplitude;
                        }
                    }
                    last_event_had_wait = false;
                }
                _ => {}
            },
            _ => {}
        }
    }
    let final_duration = ticks_to_duration(0, current_time_ticks, tempo_changes, ticks_per_beat);

    if !commands.is_empty()
        && (commands.last().unwrap().frequency != 0.0 || commands.last().unwrap().amplitude != 0.0)
    {
        commands.push(RumbleCommand {
            frequency: 0.0,
            amplitude: 0.0,
            wait_before: Duration::ZERO,
        });
    }

    Ok(RumbleTrack {
        commands,
        total_duration: final_duration,
    })
}

pub fn parse_midi_to_rumble(
    midi_data: &[u8],
    track_selections: Vec<Option<usize>>,
) -> Result<Vec<RumbleTrack>, ParseError> {
    let smf = Smf::parse(midi_data)?;
    let num_joycons = track_selections.len(); // This is our limit

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

    let selected_tracks: Vec<&TrackMetrics> = if track_selections.iter().all(Option::is_none) {
        // Sort by score and take only what we need
        track_metrics.sort_by(|a, b| {
            b.calculate_score()
                .partial_cmp(&a.calculate_score())
                .unwrap()
        });
        track_metrics
            .iter()
            .filter(|m| !m.is_percussion && m.note_count > 0)
            .take(num_joycons) // Only take as many tracks as we have JoyCons
            .collect()
    } else {
        // For manual selection, still respect the JoyCon limit
        track_selections
            .iter()
            .take(num_joycons) // Ensure we don't exceed JoyCon count
            .filter_map(|&selection| selection.and_then(|idx| track_metrics.get(idx)))
            .collect()
    };

    if selected_tracks.is_empty() {
        return Err(ParseError::NoTracks);
    }

    let rumble_tracks: Vec<RumbleTrack> = selected_tracks
        .iter()
        .map(|track| {
            convert_track_with_tempo(
                &smf.tracks[track.track_index],
                &tempo_changes,
                ticks_per_beat,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rumble_tracks)
}
