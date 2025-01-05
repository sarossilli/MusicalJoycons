use midly::{Track, TrackEventKind};
use std::collections::HashMap;

use super::track_types::TrackMetrics;

pub fn analyze_track(track: &Track, ticks_per_beat: f32, default_tempo: u32) -> TrackMetrics {
    let mut metrics = TrackMetrics::default();
    let mut active_notes: HashMap<u8, f32> = HashMap::new();
    let mut note_pitches = Vec::new();
    let mut note_timings = Vec::new();
    let mut velocities = Vec::new();
    let mut total_velocity = 0;
    let mut note_durations = Vec::new();
    let mut total_note_duration = 0.0;
    let mut program_number = None;

    let mut current_time = 0.0;
    let mut current_tempo = default_tempo as f32;
    let microseconds_per_tick = current_tempo / ticks_per_beat;

    for event in track.iter() {
        current_time += event.delta.as_int() as f32 * microseconds_per_tick / 1_000_000.0;

        match event.kind {
            TrackEventKind::Meta(meta) => match meta {
                midly::MetaMessage::Tempo(tempo) => {
                    current_tempo = tempo.as_int() as f32;
                }
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
                        total_velocity += velocity as u32;
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

    // Calculate basic metrics
    metrics.total_duration = current_time;
    metrics.note_density = if current_time > 0.0 {
        metrics.note_count as f32 / current_time
    } else {
        0.0
    };

    metrics.avg_velocity = if metrics.note_count > 0 {
        total_velocity as f32 / metrics.note_count as f32
    } else {
        0.0
    };

    if velocities.len() > 1 {
        let mean = metrics.avg_velocity;
        metrics.velocity_variance = (velocities.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
            / (velocities.len() - 1) as f32)
            .sqrt();
    }

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
        let mut intervals: Vec<f32> = note_timings.windows(2).map(|w| w[1] - w[0]).collect();

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

/// TESTS

#[cfg(test)]
mod tests {
    use crate::midi::track_types::TrackType;

    use super::*;
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    };
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
}
