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
pub struct TrackSwitchPoint {
    pub time: Duration,
    pub alternative_track_index: usize,
}

#[derive(Debug, Clone)]
pub struct RumbleTrack {
    pub commands: Vec<RumbleCommand>,
    pub total_duration: Duration,
    pub switch_points: Vec<TrackSwitchPoint>,  // New field
    pub track_index: usize,                    // New field
    pub metrics: TrackMetrics,  // Add this field
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

#[derive(Debug)]
pub struct TrackMergeController {
    pub tracks: Vec<RumbleTrack>,
    current_time: Duration,
    last_switch_times: Vec<Duration>,
}

impl TrackMergeController {
    pub const FUTURE_WINDOW_SIZE: Duration = Duration::from_secs(2);

    pub fn new(tracks: Vec<RumbleTrack>, num_joycons: usize) -> Self {
        Self {
            tracks,
            current_time: Duration::ZERO,
            last_switch_times: vec![Duration::ZERO; num_joycons],
        }
    }

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
            if current_time >= start_time && current_time <= end_time {
                if cmd.amplitude > 0.0 {
                    note_count += 1;
                    max_amplitude = max_amplitude.max(cmd.amplitude);
                }
            }
            if current_time > end_time {
                break;
            }
        }
        (note_count, max_amplitude)
    }

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
        let (current_notes, current_max_amp) = Self::evaluate_track_section(
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

    pub fn record_switch(&mut self, joycon_idx: usize) {
        self.last_switch_times[joycon_idx] = self.current_time;
    }

    pub fn update_time(&mut self, delta: Duration) {
        self.current_time += delta;
    }
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

    let selected_tracks: Vec<&TrackMetrics> = if track_selections.is_empty() || track_selections.iter().all(Option::is_none) {
        // Sort all tracks by score
        track_metrics.sort_by(|a, b| {
            b.calculate_score()
                .partial_cmp(&a.calculate_score())
                .unwrap()
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
            cmd.amplitude = cmd.amplitude / max_velocity;
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
                .filter(|(idx, _score, activity)| {
                    // Find tracks that will be active at or soon after the switch point
                    let mut is_active = false;
                    let current_time = Duration::ZERO;
                    let look_ahead = Duration::from_millis(500);

                    for (time, active) in activity.iter() {
                        if *time >= switch_point.time && *time <= switch_point.time + look_ahead {
                            if *active {
                                is_active = true;
                                break;
                            }
                        }
                    }
                    is_active
                })
                .map(|(idx, score, _)| (*idx, *score))
                .collect();

            // Sort available tracks by score
            available_tracks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

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
