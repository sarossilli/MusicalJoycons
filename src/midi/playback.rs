//! MIDI playback coordination across multiple JoyCons.
//!
//! This module provides the high-level playback functionality, coordinating
//! multiple JoyCons to play different tracks from a MIDI file simultaneously.
//!
//! # Playback Architecture
//!
//! The playback system uses multiple threads:
//!
//! 1. **Main thread**: Loads MIDI, initializes JoyCons, coordinates startup
//! 2. **Ranking thread**: Continuously evaluates track activity and reassigns tracks
//! 3. **JoyCon threads**: One per device, handles rumble command execution
//!
//! # Track Assignment
//!
//! Initial track assignment is based on track scores. During playback:
//! - The ranking thread evaluates upcoming note activity every 250ms
//! - If a better track is found, the assignment is updated
//! - JoyCon threads switch to new tracks during silent periods (note-off events)
//!
//! # Synchronization
//!
//! All JoyCon threads wait for a shared start signal before beginning playback,
//! ensuring synchronized start across all devices.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::joycon::JoyConManager;

use super::rumble::{parse_midi_to_rumble, RumbleCommand, TrackMergeController};

/// How often the ranking thread re-evaluates track assignments.
const RANKING_WINDOW: Duration = Duration::from_millis(500);

/// Finds the command index at or just before the specified time.
///
/// Used when switching tracks to find the right starting position.
///
/// # Arguments
///
/// * `commands` - The command sequence to search
/// * `target_time` - The time position to find
///
/// # Returns
///
/// Index of the first command that would execute after `target_time`,
/// or `commands.len()` if past the end.
fn find_commands_at_time(commands: &[RumbleCommand], target_time: Duration) -> usize {
    let mut accumulated_time = Duration::ZERO;

    for (idx, cmd) in commands.iter().enumerate() {
        if accumulated_time + cmd.wait_before > target_time {
            return idx;
        }
        accumulated_time += cmd.wait_before;
    }

    commands.len()
}

/// Determines if a command represents a note-off transition.
///
/// A note-off occurs when the previous command had non-zero amplitude
/// and the current command has zero amplitude. This is the ideal time
/// for track switching as it won't interrupt a sounding note.
///
/// # Arguments
///
/// * `cmd` - The current command
/// * `prev_cmd` - The previous command, if any
///
/// # Returns
///
/// `true` if this is a note-off transition, `false` otherwise.
fn is_note_off(cmd: &RumbleCommand, prev_cmd: Option<&RumbleCommand>) -> bool {
    match prev_cmd {
        Some(prev) => prev.amplitude > 0.0 && cmd.amplitude == 0.0,
        None => false,
    }
}

/// Plays a MIDI file through connected JoyCons.
///
/// This is the main entry point for MIDI playback. It handles the complete
/// process from file loading through synchronized multi-JoyCon playback.
///
/// # Process
///
/// 1. Initialize the JoyCon manager and connect to devices
/// 2. Load and parse the MIDI file
/// 3. Analyze tracks and assign the best ones to available JoyCons
/// 4. Start synchronized playback with dynamic track reassignment
/// 5. Wait for all tracks to complete
///
/// # Arguments
///
/// * `path` - Path to the MIDI file to play
///
/// # Errors
///
/// Returns an error if:
/// - No JoyCons can be found or connected
/// - The MIDI file cannot be read or parsed
/// - No playable tracks are found in the file
/// - HID communication fails during playback
///
/// # Example
///
/// ```no_run
/// use musical_joycons::midi::play_midi_file;
/// use std::path::PathBuf;
///
/// let path = PathBuf::from("song.mid");
/// play_midi_file(path)?;
/// # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
/// ```
///
/// # Blocking
///
/// This function blocks until playback completes. For non-blocking playback,
/// call from a separate thread.
///
/// # Console Output
///
/// The function prints status messages during playback:
/// - Device discovery progress
/// - Track assignment information
/// - Dynamic track switching events
/// - Playback completion
pub fn play_midi_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let manager = JoyConManager::new()?;
    let joycons = manager.connect_and_initialize_joycons()?;
    let num_joycons = joycons.len();

    println!("🎵 Loading MIDI file: {:?}", path);
    let midi_data = std::fs::read(&path)?;

    // Load ALL tracks by passing an empty vec
    let tracks = parse_midi_to_rumble(&midi_data, vec![])?;

    // Create initial assignments from the best tracks
    let mut track_scores: Vec<(usize, f32)> = tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| !track.metrics.is_percussion && track.metrics.note_count > 0)
        .map(|(idx, track)| (idx, track.metrics.calculate_score()))
        .collect();

    if track_scores.is_empty() {
        return Err("No playable tracks found in MIDI file".into());
    }

    track_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let initial_assignments: Vec<usize> = track_scores
        .iter()
        .take(num_joycons)
        .map(|(idx, _)| *idx)
        .collect();

    println!("Available tracks: {}", tracks.len());
    println!("Initial assignments: {:?}", initial_assignments);
    println!("Top track scores:");
    for (idx, score) in track_scores.iter().take(joycons.len()) {
        println!("  Track {}: {:.2}", idx, score);
    }

    let start_signal = Arc::new(Mutex::new(false));
    let current_assignments = Arc::new(Mutex::new(initial_assignments.clone()));
    let active_tracks = Arc::new(Mutex::new(vec![true; tracks.len()]));
    let mut handles: Vec<
        std::thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    > = Vec::new();

    // Spawn ranking thread
    let ranking_signal = Arc::clone(&start_signal);
    let ranking_assignments = Arc::clone(&current_assignments);
    let ranking_active = Arc::clone(&active_tracks);
    let ranking_tracks = tracks.clone();

    thread::spawn(move || {
        while !*ranking_signal.lock().unwrap_or_else(|e| e.into_inner()) {
            thread::sleep(Duration::from_millis(1));
        }

        let mut current_time = Duration::ZERO;

        loop {
            // Score ALL tracks, not just the currently assigned ones
            let mut track_scores: Vec<(usize, f32, usize)> = ranking_tracks
                .iter()
                .enumerate()
                .filter(|(idx, track)| {
                    ranking_active.lock().unwrap_or_else(|e| e.into_inner())[*idx]
                        && !track.metrics.is_percussion
                })
                .map(|(idx, track)| {
                    let (note_count, max_amp) = TrackMergeController::evaluate_track_section(
                        &track.commands,
                        current_time,
                        TrackMergeController::FUTURE_WINDOW_SIZE,
                    );
                    let score = if max_amp >= 0.3 && note_count > 0 {
                        track.metrics.calculate_window_score(
                            note_count,
                            TrackMergeController::FUTURE_WINDOW_SIZE.as_secs_f32(),
                        )
                    } else {
                        0.0
                    };
                    (idx, score, note_count)
                })
                .collect();

            // Sort by note count first, then by score
            track_scores.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            });

            // Take the top N tracks for N JoyCons
            let mut new_assignments = vec![];
            for (idx, _, _) in track_scores.iter().take(5) {
                new_assignments.push(*idx);
            }

            // Update assignments if changed and there are active notes
            let mut assignments: std::sync::MutexGuard<'_, Vec<usize>> = ranking_assignments
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if *assignments != new_assignments {
                let should_switch = track_scores
                    .iter()
                    .take(num_joycons)
                    .any(|(_, _, notes)| *notes > 0);

                if should_switch {
                    println!(
                        "🔄 Reassigning tracks: {:?} (notes: {:?}, scores: {:?})",
                        new_assignments,
                        track_scores
                            .iter()
                            .take(num_joycons)
                            .map(|(_, _, notes)| notes)
                            .collect::<Vec<_>>(),
                        track_scores
                            .iter()
                            .take(num_joycons)
                            .map(|(_, score, _)| format!("{:.2}", score))
                            .collect::<Vec<_>>()
                    );
                    *assignments = new_assignments;
                }
            }
            drop(assignments);

            thread::sleep(RANKING_WINDOW / 2);
            current_time += RANKING_WINDOW / 2;

            // Check if all tracks are complete
            if ranking_active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .all(|&active| !active)
            {
                break;
            }
        }
    });

    let merge_controller = Arc::new(Mutex::new(TrackMergeController::new(
        tracks.clone(),
        num_joycons,
    )));

    // Spawn JoyCon threads
    for (joycon_idx, mut joycon) in joycons.into_iter().enumerate() {
        let joycon_signal = Arc::clone(&start_signal);
        let joycon_assignments = Arc::clone(&current_assignments);
        let joycon_active = Arc::clone(&active_tracks);
        let joycon_tracks = tracks.clone();
        let initial_assignments = initial_assignments.clone();
        let merge_controller = Arc::clone(&merge_controller);

        handles.push(thread::spawn(move || {
            while !*joycon_signal.lock().unwrap_or_else(|e| e.into_inner()) {
                thread::sleep(Duration::from_millis(1));
            }

            println!("🎮 JoyCon {} starting playback", joycon_idx + 1);
            let mut current_time = Duration::ZERO;
            let mut current_track_idx = initial_assignments[joycon_idx]; // Start with initial assignment
            let mut command_index = 0;
            let mut pending_track_switch: Option<usize> = None;

            loop {
                let track = &joycon_tracks[current_track_idx];
                if command_index >= track.commands.len() {
                    println!(
                        "🎮 JoyCon {} finished track {}",
                        joycon_idx + 1,
                        current_track_idx
                    );
                    joycon_active.lock().unwrap_or_else(|e| e.into_inner())[current_track_idx] =
                        false;
                    break;
                }

                // Check for track reassignment
                let assignments = joycon_assignments.lock().unwrap_or_else(|e| e.into_inner());
                let assigned_track_idx = assignments[joycon_idx];
                drop(assignments);

                if assigned_track_idx != current_track_idx {
                    let should_switch = merge_controller
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .should_switch_tracks(
                            joycon_idx,
                            current_track_idx,
                            assigned_track_idx,
                            command_index,
                        );

                    if should_switch {
                        pending_track_switch = Some(assigned_track_idx);
                    }
                }

                let cmd = &track.commands[command_index];
                let prev_cmd = if command_index > 0 {
                    Some(&track.commands[command_index - 1])
                } else {
                    None
                };

                // If we have a pending switch and current command is a note-off
                if let Some(new_track_idx) = pending_track_switch {
                    if is_note_off(cmd, prev_cmd) {
                        println!(
                            "🎮 JoyCon {} switching from track {} to {} at {:?} (note off)",
                            joycon_idx + 1,
                            current_track_idx,
                            new_track_idx,
                            current_time
                        );

                        current_track_idx = new_track_idx;
                        let new_index = find_commands_at_time(
                            &joycon_tracks[current_track_idx].commands,
                            current_time,
                        );
                        println!(
                            "  → New command index: {} (total commands: {})",
                            new_index,
                            joycon_tracks[current_track_idx].commands.len()
                        );
                        command_index = new_index;
                        pending_track_switch = None;
                        merge_controller
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .record_switch(joycon_idx);
                        continue; // Restart loop with new track
                    }
                }

                // Debug output for commands
                if cmd.amplitude > 0.0 {
                    println!(
                        "🎮 JoyCon {} playing note: freq={:.1}, amp={:.2}, wait={:?}",
                        joycon_idx + 1,
                        cmd.frequency,
                        cmd.amplitude,
                        cmd.wait_before
                    );
                }

                if !cmd.wait_before.is_zero() {
                    thread::sleep(cmd.wait_before);
                    merge_controller
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .update_time(cmd.wait_before);
                    current_time += cmd.wait_before;
                }

                // Send the rumble command
                joycon.rumble(cmd.frequency, cmd.amplitude)?;
                command_index += 1;
            }

            println!("🎮 JoyCon {} stopping", joycon_idx + 1);
            joycon.rumble(0.0, 0.0)?;
            Ok(())
        }));
    }

    // Start playback
    println!("\n▶️ Starting playback...");
    *start_signal.lock().unwrap_or_else(|e| e.into_inner()) = true;

    // Wait for all tracks to complete
    for handle in handles {
        match handle.join() {
            Ok(result) => result?,
            Err(_) => return Err("A playback thread panicked".into()),
        }
    }

    println!("✨ Playback complete!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_commands_at_time() {
        let commands = vec![
            RumbleCommand {
                frequency: 100.0,
                amplitude: 1.0,
                wait_before: Duration::from_millis(100),
            },
            RumbleCommand {
                frequency: 200.0,
                amplitude: 0.5,
                wait_before: Duration::from_millis(200),
            },
            RumbleCommand {
                frequency: 300.0,
                amplitude: 0.7,
                wait_before: Duration::from_millis(300),
            },
        ];

        assert_eq!(find_commands_at_time(&commands, Duration::ZERO), 0);
        assert_eq!(
            find_commands_at_time(&commands, Duration::from_millis(50)),
            0
        );
        assert_eq!(
            find_commands_at_time(&commands, Duration::from_millis(100)),
            1
        );
        assert_eq!(
            find_commands_at_time(&commands, Duration::from_millis(300)),
            2
        );
        assert_eq!(
            find_commands_at_time(&commands, Duration::from_millis(600)),
            3
        );
    }

    #[test]
    fn test_is_note_off() {
        let note_on = RumbleCommand {
            frequency: 100.0,
            amplitude: 1.0,
            wait_before: Duration::from_millis(100),
        };

        let note_off = RumbleCommand {
            frequency: 100.0,
            amplitude: 0.0,
            wait_before: Duration::from_millis(100),
        };

        // Test with no previous command
        assert!(!is_note_off(&note_on, None));
        assert!(!is_note_off(&note_off, None));

        // Test note off after note on
        assert!(is_note_off(&note_off, Some(&note_on)));

        // Test note on after note off
        assert!(!is_note_off(&note_on, Some(&note_off)));

        // Test note on after note on
        assert!(!is_note_off(&note_on, Some(&note_on)));

        // Test note off after note off
        assert!(!is_note_off(&note_off, Some(&note_off)));
    }
}
