//! MIDI Part normalization.
//!
//! Groups raw MIDI events into [`Part`]s keyed by `(channel, program)`,
//! providing a stable musical unit even when MIDI files have chaotic track
//! layouts, shared channels, or mid-stream program changes.

use std::collections::HashMap;

use midly::{Smf, TrackEventKind};

/// Identifies a musical part by its MIDI channel and GM program number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartKey {
    pub channel: u8,
    pub program: u8,
}

/// A single resolved note with absolute tick positions.
#[derive(Debug, Clone)]
pub struct NoteObject {
    pub start_tick: u32,
    pub end_tick: u32,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
    pub track_index: usize,
    pub program: u8,
    pub is_drum: bool,
}

/// A musical part: all notes that share the same `(channel, program)`.
#[derive(Debug, Clone)]
pub struct Part {
    pub key: PartKey,
    pub notes: Vec<NoteObject>,
    pub is_drum: bool,
    pub name: Option<String>,
}

/// Pending (not-yet-closed) note-on event.
struct PendingNote {
    start_tick: u32,
    pitch: u8,
    velocity: u8,
    channel: u8,
    track_index: usize,
    program: u8,
    is_drum: bool,
}

/// Parse an entire [`Smf`] and return a `Vec<Part>` grouped by (channel, program).
///
/// Mid-stream program changes are handled by assigning the *dominant* program
/// (the one with the most total note ticks) to all notes on that channel.
/// Channel 9 (0-based) is always marked as drums.
pub fn normalize_to_parts(smf: &Smf) -> Vec<Part> {
    let mut all_notes: Vec<NoteObject> = Vec::new();
    let mut track_names: HashMap<usize, String> = HashMap::new();

    // Per-channel current program and program-duration accumulator.
    let mut channel_program: [u8; 16] = [0; 16];
    let mut channel_program_ticks: HashMap<(u8, u8), u64> = HashMap::new();

    // First pass: collect pending notes and resolve program changes per channel.
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut current_tick: u32 = 0;
        let mut pending: Vec<PendingNote> = Vec::new();
        let mut local_program: [u8; 16] = channel_program;

        for event in track.iter() {
            current_tick += event.delta.as_int();

            match event.kind {
                TrackEventKind::Meta(midly::MetaMessage::TrackName(name)) => {
                    track_names
                        .entry(track_idx)
                        .or_insert_with(|| String::from_utf8_lossy(name).into_owned());
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    let is_drum = ch == 9;

                    match message {
                        midly::MidiMessage::ProgramChange { program } => {
                            local_program[ch as usize] = program.as_int();
                        }
                        midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            pending.push(PendingNote {
                                start_tick: current_tick,
                                pitch: key.as_int(),
                                velocity: vel.as_int(),
                                channel: ch,
                                track_index: track_idx,
                                program: local_program[ch as usize],
                                is_drum,
                            });
                        }
                        midly::MidiMessage::NoteOff { key, .. }
                        | midly::MidiMessage::NoteOn { key, .. } => {
                            // vel == 0 NoteOn also ends up here
                            let pitch = key.as_int();
                            if let Some(pos) = pending
                                .iter()
                                .rposition(|p| p.pitch == pitch && p.channel == ch)
                            {
                                let p = pending.remove(pos);
                                let end_tick = current_tick.max(p.start_tick + 1);
                                let duration_ticks = (end_tick - p.start_tick) as u64;

                                *channel_program_ticks
                                    .entry((p.channel, p.program))
                                    .or_insert(0) += duration_ticks;

                                all_notes.push(NoteObject {
                                    start_tick: p.start_tick,
                                    end_tick,
                                    pitch: p.pitch,
                                    velocity: p.velocity,
                                    channel: p.channel,
                                    track_index: p.track_index,
                                    program: p.program,
                                    is_drum: p.is_drum,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Close any un-terminated notes at the end of the track.
        for p in pending {
            let end_tick = current_tick.max(p.start_tick + 1);
            let duration_ticks = (end_tick - p.start_tick) as u64;
            *channel_program_ticks
                .entry((p.channel, p.program))
                .or_insert(0) += duration_ticks;

            all_notes.push(NoteObject {
                start_tick: p.start_tick,
                end_tick,
                pitch: p.pitch,
                velocity: p.velocity,
                channel: p.channel,
                track_index: p.track_index,
                program: p.program,
                is_drum: p.is_drum,
            });
        }

        channel_program = local_program;
    }

    // Resolve dominant program per channel: the program with the most note-ticks wins.
    let mut dominant_program: [u8; 16] = [0; 16];
    for ch in 0u8..16 {
        let best = channel_program_ticks
            .iter()
            .filter(|((c, _), _)| *c == ch)
            .max_by_key(|(_, &ticks)| ticks)
            .map(|((_, prog), _)| *prog)
            .unwrap_or(0);
        dominant_program[ch as usize] = best;
    }

    // Re-key every note to its channel's dominant program so that mid-stream
    // program changes don't fragment parts.
    for note in &mut all_notes {
        note.program = dominant_program[note.channel as usize];
    }

    // Group into Parts.
    let mut part_map: HashMap<PartKey, Vec<NoteObject>> = HashMap::new();
    for note in all_notes {
        let key = PartKey {
            channel: note.channel,
            program: note.program,
        };
        part_map.entry(key).or_default().push(note);
    }

    let mut parts: Vec<Part> = part_map
        .into_iter()
        .map(|(key, mut notes)| {
            notes.sort_by_key(|n| n.start_tick);
            let is_drum = key.channel == 9;

            // Try to find a track name from the first note's track.
            let name = notes
                .first()
                .and_then(|n| track_names.get(&n.track_index))
                .cloned();

            Part {
                key,
                notes,
                is_drum,
                name,
            }
        })
        .collect();

    // Stable ordering: drums last, then by channel, then by program.
    parts.sort_by_key(|p| (p.is_drum as u8, p.key.channel, p.key.program));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{Format, Header, MidiMessage, Timing, TrackEvent, TrackEventKind};

    fn make_smf(tracks: Vec<Vec<TrackEvent<'static>>>) -> Vec<u8> {
        let smf = Smf {
            header: Header {
                format: if tracks.len() == 1 {
                    Format::SingleTrack
                } else {
                    Format::Parallel
                },
                timing: Timing::Metrical(480.into()),
            },
            tracks,
        };
        let mut buf = Vec::new();
        smf.write(&mut buf).unwrap();
        buf
    }

    fn note_on(delta: u32, ch: u8, key: u8, vel: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: ch.into(),
                message: MidiMessage::NoteOn {
                    key: key.into(),
                    vel: vel.into(),
                },
            },
        }
    }

    fn note_off(delta: u32, ch: u8, key: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: ch.into(),
                message: MidiMessage::NoteOff {
                    key: key.into(),
                    vel: 0.into(),
                },
            },
        }
    }

    fn program_change(delta: u32, ch: u8, program: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: ch.into(),
                message: MidiMessage::ProgramChange {
                    program: program.into(),
                },
            },
        }
    }

    #[test]
    fn groups_by_channel_program() {
        let data = make_smf(vec![vec![
            program_change(0, 0, 1),
            note_on(0, 0, 60, 100),
            note_off(480, 0, 60),
            program_change(0, 1, 33),
            note_on(0, 1, 40, 80),
            note_off(480, 1, 40),
        ]]);
        let smf = Smf::parse(&data).unwrap();
        let parts = normalize_to_parts(&smf);

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].key.channel, 0);
        assert_eq!(parts[0].key.program, 1);
        assert_eq!(parts[1].key.channel, 1);
        assert_eq!(parts[1].key.program, 33);
    }

    #[test]
    fn marks_drums_channel_9() {
        let data = make_smf(vec![vec![note_on(0, 9, 36, 100), note_off(480, 9, 36)]]);
        let smf = Smf::parse(&data).unwrap();
        let parts = normalize_to_parts(&smf);

        assert_eq!(parts.len(), 1);
        assert!(parts[0].is_drum);
        assert!(parts[0].notes[0].is_drum);
    }

    #[test]
    fn dominant_program_wins() {
        // Two program changes on ch0: program 5 gets 1 tick, program 10 gets 960 ticks.
        let data = make_smf(vec![vec![
            program_change(0, 0, 5),
            note_on(0, 0, 60, 100),
            note_off(1, 0, 60),
            program_change(0, 0, 10),
            note_on(0, 0, 64, 100),
            note_off(960, 0, 64),
        ]]);
        let smf = Smf::parse(&data).unwrap();
        let parts = normalize_to_parts(&smf);

        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].key.program, 10);
        assert_eq!(parts[0].notes.len(), 2);
    }

    #[test]
    fn multi_track_file() {
        let data = make_smf(vec![
            vec![note_on(0, 0, 60, 100), note_off(480, 0, 60)],
            vec![note_on(0, 1, 48, 80), note_off(480, 1, 48)],
        ]);
        let smf = Smf::parse(&data).unwrap();
        let parts = normalize_to_parts(&smf);

        assert_eq!(parts.len(), 2);
    }
}
