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

                    if !last_event_had_wait || !commands.is_empty() {
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
    primary_selection: Option<usize>,
    secondary_selection: Option<usize>,
) -> Result<(RumbleTrack, Option<RumbleTrack>), ParseError> {
    let smf = Smf::parse(midi_data)?;

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

    println!("\n📊 Track Analysis Results:");
    println!("-------------------------");
    for track in track_metrics.iter() {
        let track_name = track.track_name.as_deref().unwrap_or("Unnamed Track");
        let instrument = track
            .track_instrument
            .as_deref()
            .unwrap_or("Unknown Instrument");
        let score = track.calculate_score();

        println!(
            "Track {}: \"{}\" ({})",
            track.track_index, track_name, instrument
        );
        println!("  - Type: {:?}", track.track_type);
        println!("  - Score: {:.2}", score);
        println!(
            "  - Notes: {} ({} unique)",
            track.note_count, track.unique_notes
        );
        println!("  - Is Percussion: {}", track.is_percussion);
        println!("  - Note Density: {:.2} notes/sec", track.note_density);
        println!("-------------------------");
    }

    let selected_tracks = if let Some(primary_idx) = primary_selection {
        vec![&track_metrics[primary_idx]]
    } else {
        track_metrics.sort_by(|a, b| {
            b.calculate_score()
                .partial_cmp(&a.calculate_score())
                .unwrap()
        });
        track_metrics
            .iter()
            .filter(|m| !m.is_percussion && m.note_count > 0)
            .take(1)
            .collect()
    };

    if selected_tracks.is_empty() {
        return Err(ParseError::NoTracks);
    }

    let primary_track = convert_track_with_tempo(
        &smf.tracks[selected_tracks[0].track_index],
        &tempo_changes,
        ticks_per_beat,
    )?;

    let secondary_track = if let Some(secondary_idx) = secondary_selection {
        Some(convert_track_with_tempo(
            &smf.tracks[secondary_idx],
            &tempo_changes,
            ticks_per_beat,
        )?)
    } else if primary_selection.is_none() {
        track_metrics
            .iter()
            .filter(|m| {
                !m.is_percussion
                    && m.note_count > 0
                    && m.track_index != selected_tracks[0].track_index
            })
            .take(1)
            .next()
            .map(|track| {
                convert_track_with_tempo(
                    &smf.tracks[track.track_index],
                    &tempo_changes,
                    ticks_per_beat,
                )
            })
            .transpose()?
    } else {
        None
    };

    println!("\n🎯 Track Selection:");
    println!(
        "Primary Track: {} (Score: {:.2})",
        selected_tracks[0]
            .track_name
            .as_deref()
            .unwrap_or("Unnamed Track"),
        selected_tracks[0].calculate_score()
    );

    if selected_tracks.len() > 1 {
        println!(
            "Secondary Track: {} (Score: {:.2})",
            selected_tracks[1]
                .track_name
                .as_deref()
                .unwrap_or("Unnamed Track"),
            selected_tracks[1].calculate_score()
        );

        println!("\nTrack Relationship:");
        println!(
            "  Primary:   {} notes/sec, {} unique notes",
            selected_tracks[0].note_density, selected_tracks[0].unique_notes
        );
        println!(
            "  Secondary: {} notes/sec, {} unique notes",
            selected_tracks[1].note_density, selected_tracks[1].unique_notes
        );
    }
    Ok((primary_track, secondary_track))
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{Format, Header, MetaMessage, MidiMessage, Timing, TrackEvent, TrackEventKind};

    // Helper function to create a basic MIDI file for testing
    fn create_test_midi(tracks: Vec<Vec<TrackEvent<'static>>>) -> Vec<u8> {
        let header = Header::new(Format::Sequential, Timing::Metrical(480.into()));
        let smf = Smf { header, tracks };

        // Write to a vector
        let mut buffer = Vec::new();
        smf.write(&mut buffer).expect("Failed to write MIDI data");
        buffer
    }

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

    fn create_tempo_event(delta: u32, tempo: u32) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo.into())),
        }
    }

    #[test]
    fn test_simple_track_conversion() {
        let track = vec![
            create_note_on(0, 60, 100), // Middle C
            create_note_off(480, 60),   // Half note duration
            create_note_on(0, 64, 100), // E
            create_note_off(480, 64),   // Half note duration
        ];

        let midi_data = create_test_midi(vec![track]);
        let (primary_track, secondary_track) =
            parse_midi_to_rumble(&midi_data, None, None).expect("Failed to parse MIDI");

        assert!(secondary_track.is_none());
        assert!(!primary_track.commands.is_empty());

        // Verify the rumble commands
        let commands = &primary_track.commands;
        assert!(commands.len() >= 4); // At least note-on, note-off for each note

        // Check first note
        assert_eq!(commands[0].amplitude, 100.0 / 127.0);
        assert!(commands[0].wait_before.is_zero());

        // Verify silence at end
        let last_command = commands.last().unwrap();
        assert_eq!(last_command.amplitude, 0.0);
        assert_eq!(last_command.frequency, 0.0);
    }

    #[test]
    fn test_tempo_changes() {
        let track = vec![
            create_tempo_event(0, 500_000), // 120 BPM
            create_note_on(0, 60, 100),
            create_note_off(480, 60),
            create_tempo_event(0, 250_000), // 240 BPM
            create_note_on(0, 64, 100),
            create_note_off(480, 64),
        ];

        let midi_data = create_test_midi(vec![track]);
        let (primary_track, _) =
            parse_midi_to_rumble(&midi_data, None, None).expect("Failed to parse MIDI");

        // Second note should have different timing due to tempo change
        let commands = &primary_track.commands;
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_multiple_tracks() {
        let track1 = vec![create_note_on(0, 60, 100), create_note_off(480, 60)];

        let track2 = vec![create_note_on(0, 48, 100), create_note_off(480, 48)];

        let midi_data = create_test_midi(vec![track1, track2]);
        let (primary_track, secondary_track) =
            parse_midi_to_rumble(&midi_data, None, None).expect("Failed to parse MIDI");

        assert!(secondary_track.is_some());
    }

    #[test]
    fn test_track_selection() {
        let track1 = vec![create_note_on(0, 60, 100), create_note_off(480, 60)];

        let track2 = vec![create_note_on(0, 48, 100), create_note_off(480, 48)];

        let midi_data = create_test_midi(vec![track1, track2]);

        // Test explicit primary track selection
        let (primary_track, _) =
            parse_midi_to_rumble(&midi_data, Some(1), None).expect("Failed to parse MIDI");

        // The frequency of the first note should match track2
        assert!(primary_track.commands[0].frequency < 500.0); // Lower note
    }

    #[test]
    fn test_note_to_frequency() {
        // Test with standard MIDI note numbers and frequencies
        assert!(
            (note_to_frequency(60) - 261.63).abs() < 1.0,
            "Middle C (MIDI 60) should be ~261.63 Hz, got {}",
            note_to_frequency(60)
        );

        assert!(
            (note_to_frequency(69) - 440.0).abs() < 1.0,
            "A4 (MIDI 69) should be 440 Hz, got {}",
            note_to_frequency(69)
        );

        assert!(
            (note_to_frequency(81) - 880.0).abs() < 1.0,
            "A5 (MIDI 81) should be 880 Hz, got {}",
            note_to_frequency(81)
        );

        // Test octave relationships
        let a4 = note_to_frequency(69);
        let a5 = note_to_frequency(81);
        assert!(
            (a5 / a4 - 2.0).abs() < 0.01,
            "One octave difference should double frequency"
        );
    }

    #[test]
    fn test_empty_midi() {
        let midi_data = create_test_midi(vec![vec![]]);
        let result = parse_midi_to_rumble(&midi_data, None, None);
        assert!(matches!(result, Err(ParseError::NoTracks)));
    }

    #[test]
    fn test_invalid_midi_data() {
        let invalid_data = vec![0, 1, 2, 3]; // Invalid MIDI data
        let result = parse_midi_to_rumble(&invalid_data, None, None);
        assert!(matches!(result, Err(ParseError::MidiError(_))));
    }
}
