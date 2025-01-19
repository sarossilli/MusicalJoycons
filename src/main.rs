use musical_joycons::joycon::{JoyCon, JoyConError, JoyConManager};
use musical_joycons::midi::rubmle::{parse_midi_to_rumble, RumbleCommand, TrackMergeController};
use std::io::{self};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const RANKING_WINDOW: Duration = Duration::from_millis(500);

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🎮 Musical JoyCons - MIDI Player");
    println!("===============================");

    println!("Drag and drop your MIDI file into this terminal and press Enter:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let path = PathBuf::from(input.trim().trim_matches('"'));

    if (!path.exists()) {
        println!("❌ File not found: {:?}", path);
        return Ok(());
    }

    play_midi_file(path)?;

    Ok(())
}

fn find_commands_at_time(commands: &[RumbleCommand], target_time: Duration) -> usize {
    let mut current_time = Duration::ZERO;
    for (idx, cmd) in commands.iter().enumerate() {
        if current_time >= target_time {
            return idx;
        }
        current_time += cmd.wait_before;
    }
    0
}

fn is_note_active(cmd: &RumbleCommand) -> bool {
    cmd.amplitude > 0.0
}

fn is_note_off(cmd: &RumbleCommand, prev_cmd: Option<&RumbleCommand>) -> bool {
    match prev_cmd {
        Some(prev) => prev.amplitude > 0.0 && cmd.amplitude == 0.0,
        None => false
    }
}

fn play_midi_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let joycons = connect_to_joycons()?;
    let num_joycons = joycons.len();

    println!("🎵 Loading MIDI file: {:?}", path);
    let midi_data = std::fs::read(&path)?;
    
    // Load ALL tracks by passing an empty vec
    let tracks = parse_midi_to_rumble(&midi_data, vec![])?;

    // Create initial assignments from the best tracks
    let mut track_scores: Vec<(usize, f32)> = tracks.iter()
        .enumerate()
        .filter(|(_, track)| !track.metrics.is_percussion && track.metrics.note_count > 0)
        .map(|(idx, track)| (idx, track.metrics.calculate_score()))
        .collect();

    if track_scores.is_empty() {
        return Err("No playable tracks found in MIDI file".into());
    }

    track_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    
    let initial_assignments: Vec<usize> = track_scores
        .iter()
        .take(num_joycons)
        .map(|(idx, _)| *idx)
        .collect();

    println!("Available tracks: {}", tracks.len());
    println!("Initial assignments: {:?}", initial_assignments);
    println!("Top track scores:");
    for (idx, score) in track_scores.iter().take(5) {
        println!("  Track {}: {:.2}", idx, score);
    }

    let start_signal = Arc::new(Mutex::new(false));
    let current_assignments = Arc::new(Mutex::new(initial_assignments.clone()));
    let active_tracks = Arc::new(Mutex::new(vec![true; tracks.len()]));
    let mut handles: Vec<std::thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>> = Vec::new();

    // Spawn ranking thread
    let ranking_signal = Arc::clone(&start_signal);
    let ranking_assignments = Arc::clone(&current_assignments);
    let ranking_active = Arc::clone(&active_tracks);
    let ranking_tracks = tracks.clone();

    thread::spawn(move || {
        while !*ranking_signal.lock().unwrap() {
            thread::sleep(Duration::from_millis(1));
        }

        let mut current_time = Duration::ZERO;
        
        loop {
            let window_start = current_time;
            let window_end = current_time + RANKING_WINDOW;

            // Score ALL tracks, not just the currently assigned ones
            let mut track_scores: Vec<(usize, f32, usize)> = ranking_tracks
                .iter()
                .enumerate()
                .filter(|(idx, track)| {
                    ranking_active.lock().unwrap()[*idx] && !track.metrics.is_percussion
                })
                .map(|(idx, track)| {
                    let (note_count, max_amp) = TrackMergeController::evaluate_track_section(
                        &track.commands,
                        current_time,
                        TrackMergeController::FUTURE_WINDOW_SIZE
                    );
                    let score = if max_amp >= 0.3 && note_count > 0 {
                        track.metrics.calculate_window_score(note_count, TrackMergeController::FUTURE_WINDOW_SIZE.as_secs_f32())
                    } else {
                        0.0
                    };
                    (idx, score, note_count)
                })
                .collect();

            // Sort by note count first, then by score
            track_scores.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| b.1.partial_cmp(&a.1).unwrap())
            });

            // Take the top N tracks for N JoyCons
            let mut new_assignments = vec![];
            for (idx, _, _) in track_scores.iter().take(5) {
                new_assignments.push(*idx);
            }

            // Update assignments if changed and there are active notes
            let mut assignments: std::sync::MutexGuard<'_, Vec<usize>> = ranking_assignments.lock().unwrap();
            if *assignments != new_assignments {
                let should_switch = track_scores.iter()
                    .take(num_joycons)
                    .any(|(_, _, notes)| *notes > 0);

                if should_switch {
                    println!("🔄 Reassigning tracks: {:?} (notes: {:?}, scores: {:?})", 
                        new_assignments,
                        track_scores.iter()
                            .take(num_joycons)
                            .map(|(_, _, notes)| notes)
                            .collect::<Vec<_>>(),
                        track_scores.iter()
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
            if ranking_active.lock().unwrap().iter().all(|&active| !active) {
                break;
            }
        }
    });

    let merge_controller = Arc::new(Mutex::new(TrackMergeController::new(tracks.clone(), num_joycons)));

    // Spawn JoyCon threads
    for (joycon_idx, mut joycon) in joycons.into_iter().enumerate() {
        let joycon_signal = Arc::clone(&start_signal);
        let joycon_assignments = Arc::clone(&current_assignments);
        let joycon_active = Arc::clone(&active_tracks);
        let joycon_tracks = tracks.clone();
        let initial_assignments = initial_assignments.clone();
        let merge_controller = Arc::clone(&merge_controller);

        handles.push(thread::spawn(move || {
            while !*joycon_signal.lock().unwrap() {
                thread::sleep(Duration::from_millis(1));
            }

            println!("🎮 JoyCon {} starting playback", joycon_idx + 1);
            let mut current_time = Duration::ZERO;
            let mut current_track_idx = initial_assignments[joycon_idx]; // Start with initial assignment
            let mut command_index = 0;
            let mut pending_track_switch: Option<usize> = None;
            let mut last_switch_time = Duration::ZERO;

            loop {
                let track = &joycon_tracks[current_track_idx];
                if command_index >= track.commands.len() {
                    println!("🎮 JoyCon {} finished track {}", joycon_idx + 1, current_track_idx);
                    joycon_active.lock().unwrap()[current_track_idx] = false;
                    break;
                }

                // Check for track reassignment
                let assignments = joycon_assignments.lock().unwrap();
                let assigned_track_idx = assignments[joycon_idx];
                drop(assignments);

                if assigned_track_idx != current_track_idx {
                    let should_switch = merge_controller.lock().unwrap()
                        .should_switch_tracks(joycon_idx, current_track_idx, assigned_track_idx, command_index);
                    
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
                        println!("🎮 JoyCon {} switching from track {} to {} at {:?} (note off)", 
                            joycon_idx + 1, current_track_idx, new_track_idx, current_time);
                        
                        current_track_idx = new_track_idx;
                        let new_index = find_commands_at_time(&joycon_tracks[current_track_idx].commands, current_time);
                        println!("  → New command index: {} (total commands: {})", 
                            new_index, joycon_tracks[current_track_idx].commands.len());
                        command_index = new_index;
                        pending_track_switch = None;
                        last_switch_time = current_time;
                        merge_controller.lock().unwrap().record_switch(joycon_idx);
                        continue; // Restart loop with new track
                    }
                }

                // Debug output for commands
                if cmd.amplitude > 0.0 {
                    println!("🎮 JoyCon {} playing note: freq={:.1}, amp={:.2}, wait={:?}", 
                        joycon_idx + 1, cmd.frequency, cmd.amplitude, cmd.wait_before);
                }

                if !cmd.wait_before.is_zero() {
                    thread::sleep(cmd.wait_before);
                    merge_controller.lock().unwrap().update_time(cmd.wait_before);
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
    *start_signal.lock().unwrap() = true;

    // Wait for all tracks to complete
    for handle in handles {
        handle.join().unwrap()?;
    }

    println!("✨ Playback complete!");
    Ok(())
}

fn connect_to_joycons() -> Result<Vec<JoyCon>, JoyConError> {
    let manager = JoyConManager::new()?;
    let mut tries = 0;

    println!("🔍 Scanning for JoyCons...");

    while tries < MAX_RETRIES {
        match manager.scan_for_devices() {
            Ok(mut joycons) => {
                if (!joycons.is_empty()) {
                    initialize_joycons(&mut joycons)?;
                    return Ok(joycons);
                }
                print_retry_message(tries);
            }
            Err(e) => {
                println!("❌ Error scanning for devices: {}", e);
                print_retry_message(tries);
            }
        }

        std::thread::sleep(RETRY_DELAY);
        tries += 1;
    }

    Err(JoyConError::NotConnected)
}

fn print_retry_message(tries: u32) {
    println!("❌ No JoyCons found. Are they connected to your PC?");
    println!("   - Check your Bluetooth devices connected");
    println!("   - Make sure the JoyCon is charged");
    println!(
        "Retrying in {} seconds... (Attempt {}/{})",
        RETRY_DELAY.as_secs(),
        tries + 1,
        MAX_RETRIES
    );
}

fn initialize_joycons(joycons: &mut [JoyCon]) -> Result<(), JoyConError> {
    println!("✅ Found {} JoyCon(s)!", joycons.len());

    for (i, joycon) in joycons.iter_mut().enumerate() {
        println!("🎮 Initializing JoyCon {}", i + 1);
        match joycon.initialize_device() {
            Ok(_) => {
                println!("✅ JoyCon {} initialized successfully", i + 1);
            }
            Err(e) => println!("❌ Failed to initialize JoyCon {}: {}", i + 1, e),
        }
    }

    Ok(())
}

fn is_track_active_at_time(commands: &[RumbleCommand], time: Duration) -> bool {
    let mut current_time = Duration::ZERO;
    for cmd in commands {
        if current_time >= time {
            return cmd.amplitude > 0.0;
        }
        current_time += cmd.wait_before;
    }
    false
}
