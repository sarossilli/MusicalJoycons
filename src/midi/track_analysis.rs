// src/midi/track_analysis.rs
use midly::{Track, TrackEventKind};

#[derive(Debug)]
pub struct TrackMetrics {
    pub track_index: usize,
    pub note_count: usize,
    pub unique_notes: usize,
    pub avg_velocity: f32,
    pub velocity_variance: f32,
    pub avg_note_duration: f32,
    pub total_duration: f32,
    pub note_density: f32, // notes per second
    pub is_percussion: bool,
    pub track_name: Option<String>,
    pub track_instrument: Option<String>,
    pub track_type: TrackType,
}

#[derive(Debug, PartialEq)]
pub enum TrackType {
    Melody,
    Harmony,
    Bass,
    Drums,
    Vocals,
    Unknown,
}

struct TrackWeights {
    note_density: f32,
    velocity_variance: f32,
    unique_notes: f32,
    avg_note_duration: f32,
}

impl TrackMetrics {
    pub fn calculate_score(&self) -> f32 {
        if self.is_percussion {
            return 0.0;
        }

        let weights = TrackWeights {
            note_density: 0.35,
            velocity_variance: 0.20,
            unique_notes: 0.80,
            avg_note_duration: 0.10,
        };

        let density_score = (self.note_density / 10.0).min(1.0) * weights.note_density;
        let velocity_score = (self.velocity_variance / 30.0).min(1.0) * weights.velocity_variance;
        let variety_score = (self.unique_notes as f32 / 88.0).min(1.0) * weights.unique_notes;
        let duration_score = (self.avg_note_duration / 1.0).min(1.0) * weights.avg_note_duration;
        let base_score = density_score + velocity_score + variety_score + duration_score;

        let type_multiplier = match self.track_type {
            TrackType::Vocals => 2.0,
            TrackType::Melody => 1.3,
            TrackType::Bass => 1.1,
            TrackType::Harmony => 1.0,
            TrackType::Drums => 0.0,
            TrackType::Unknown => 1.0,
        };

        base_score * type_multiplier
    }

    fn determine_track_type(&mut self) {
        if self.is_percussion {
            self.track_type = TrackType::Drums;
            return;
        }

        let track_name = self.track_name.as_deref().unwrap_or("").to_lowercase();
        let instrument = self
            .track_instrument
            .as_deref()
            .unwrap_or("")
            .to_lowercase();

        self.track_type = if track_name.contains("vocal")
            || track_name.contains("voice")
            || track_name.contains("sing")
            || instrument.contains("vocal")
            || track_name.contains("vox")
        {
            TrackType::Vocals
        } else if track_name.contains("melody")
            || track_name.contains("lead")
            || instrument.contains("lead")
        {
            TrackType::Melody
        } else if track_name.contains("bass") || instrument.contains("bass") {
            TrackType::Bass
        } else if track_name.contains("harm")
            || track_name.contains("pad")
            || instrument.contains("pad")
            || instrument.contains("strings")
        {
            TrackType::Harmony
        } else if track_name.contains("drum") || track_name.contains("percussion") {
            TrackType::Drums
        } else {
            TrackType::Unknown
        };
    }
}

pub fn analyze_track(track: &Track, ticks_per_beat: f32, default_tempo: u32) -> TrackMetrics {
    let mut note_count = 0;
    let mut total_velocity = 0;
    let mut velocities = Vec::new();
    let mut unique_notes = std::collections::HashSet::new();
    let mut total_duration = 0.0;
    let mut is_percussion = false;
    let mut active_notes = std::collections::HashMap::new();
    let mut note_durations = Vec::new();
    let mut track_name = None;
    let mut track_instrument = None;
    let mut program_number = None;

    let mut current_time = 0.0;
    let mut current_tempo = default_tempo as f32;
    let mut microseconds_per_tick = current_tempo / ticks_per_beat;

    for event in track.iter() {
        current_time += event.delta.as_int() as f32 * microseconds_per_tick / 1_000_000.0;

        match event.kind {
            TrackEventKind::Meta(meta) => match meta {
                midly::MetaMessage::Tempo(tempo) => {
                    current_tempo = tempo.as_int() as f32;
                    microseconds_per_tick = current_tempo / ticks_per_beat;
                }
                midly::MetaMessage::TrackName(name) => {
                    track_name = Some(String::from_utf8_lossy(name).into_owned());
                }
                midly::MetaMessage::InstrumentName(name) => {
                    track_instrument = Some(String::from_utf8_lossy(name).into_owned());
                }
                _ => {}
            },
            TrackEventKind::Midi { channel, message } => {
                if channel.as_int() == 9 {
                    is_percussion = true;
                }

                match message {
                    midly::MidiMessage::ProgramChange { program } => {
                        program_number = Some(program.as_int());
                    }
                    midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                        note_count += 1;
                        total_velocity += vel.as_int() as u32;
                        velocities.push(vel.as_int() as f32);
                        unique_notes.insert(key.as_int());
                        active_notes.insert(key.as_int(), current_time);
                    }
                    midly::MidiMessage::NoteOff { key, vel }
                    | midly::MidiMessage::NoteOn { key, vel }
                        if vel.as_int() == 0 =>
                    {
                        if let Some(start_time) = active_notes.remove(&key.as_int()) {
                            let duration = current_time - start_time;
                            note_durations.push(duration);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let avg_velocity = if note_count > 0 {
        total_velocity as f32 / note_count as f32
    } else {
        0.0
    };

    let velocity_variance = if velocities.len() > 1 {
        let mean = velocities.iter().sum::<f32>() / velocities.len() as f32;
        let variance = velocities.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
            / (velocities.len() - 1) as f32;
        variance.sqrt()
    } else {
        0.0
    };

    let avg_note_duration = if !note_durations.is_empty() {
        note_durations.iter().sum::<f32>() / note_durations.len() as f32
    } else {
        0.0
    };

    let mut metrics = TrackMetrics {
        track_index: 0,
        note_count,
        unique_notes: unique_notes.len(),
        avg_velocity,
        velocity_variance,
        avg_note_duration,
        total_duration: current_time,
        note_density: if current_time > 0.0 {
            note_count as f32 / current_time
        } else {
            0.0
        },
        is_percussion,
        track_name,
        track_instrument,
        track_type: TrackType::Unknown,
    };

    if let Some(program) = program_number {
        if program >= 56 && program <= 63 {
            metrics.track_type = TrackType::Vocals;
        } else if program >= 32 && program <= 39 {
            metrics.track_type = TrackType::Bass;
        }
    }

    metrics.determine_track_type();
    metrics
}
