//! MIDI playback coordination across multiple JoyCons with runtime L/R swap.
//!
//! The playback system uses a pre-computed [`PlaybackPlan`] combined with a
//! [`JoyConBinding`] that maps the primary and secondary parts to Left/Right
//! Joy-Cons. The binding can be changed at runtime via keyboard controls.
//!
//! # Runtime Controls
//!
//! During playback the following keyboard shortcuts are active:
//!
//! | Key | Action |
//! |-----|--------|
//! | `S` | Swap L/R assignment |
//! | `1` | Cycle to next primary candidate |
//! | `2` | Cycle to next secondary candidate |
//! | `Q` | Quit playback |

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};

use crate::joycon::{JoyConManager, JoyConType};

use super::rumble::{parse_midi_to_rumble, RumbleCommand};
use super::scoring::PartSelection;

/// Maps the primary and secondary parts to physical Joy-Con sides.
///
/// Playback threads read this through a shared `Arc<Mutex<…>>` on every
/// command. Swap and cycle operations only touch the binding — they never
/// alter the underlying note timelines.
#[derive(Debug, Clone)]
pub struct JoyConBinding {
    pub primary_part_idx: usize,
    pub secondary_part_idx: usize,
    /// `true` → Right Joy-Con plays primary, Left plays secondary.
    pub primary_on_right: bool,

    pub primary_candidates: Vec<usize>,
    pub secondary_candidates: Vec<usize>,
    primary_candidate_pos: usize,
    secondary_candidate_pos: usize,
}

impl JoyConBinding {
    pub fn new(selection: &PartSelection) -> Self {
        Self {
            primary_part_idx: selection.primary,
            secondary_part_idx: selection.secondary,
            primary_on_right: true,
            primary_candidates: selection.primary_candidates.clone(),
            secondary_candidates: selection.secondary_candidates.clone(),
            primary_candidate_pos: 0,
            secondary_candidate_pos: 0,
        }
    }

    /// Returns the rumble-track index that should play on the given Joy-Con side.
    pub fn track_for_side(&self, side: JoyConSide) -> usize {
        match (side, self.primary_on_right) {
            (JoyConSide::Right, true) | (JoyConSide::Left, false) => self.primary_part_idx,
            (JoyConSide::Left, true) | (JoyConSide::Right, false) => self.secondary_part_idx,
        }
    }

    pub fn swap(&mut self) {
        self.primary_on_right = !self.primary_on_right;
        let side_str = if self.primary_on_right {
            "Right"
        } else {
            "Left"
        };
        println!("🔄 Swapped — primary now on {side_str} Joy-Con");
    }

    pub fn cycle_primary(&mut self) {
        if self.primary_candidates.len() <= 1 {
            return;
        }
        self.primary_candidate_pos =
            (self.primary_candidate_pos + 1) % self.primary_candidates.len();
        self.primary_part_idx = self.primary_candidates[self.primary_candidate_pos];
        println!(
            "🔁 Primary → part {} (candidate {}/{})",
            self.primary_part_idx,
            self.primary_candidate_pos + 1,
            self.primary_candidates.len()
        );
    }

    pub fn cycle_secondary(&mut self) {
        if self.secondary_candidates.len() <= 1 {
            return;
        }
        self.secondary_candidate_pos =
            (self.secondary_candidate_pos + 1) % self.secondary_candidates.len();
        self.secondary_part_idx = self.secondary_candidates[self.secondary_candidate_pos];
        println!(
            "🔁 Secondary → part {} (candidate {}/{})",
            self.secondary_part_idx,
            self.secondary_candidate_pos + 1,
            self.secondary_candidates.len()
        );
    }
}

/// Logical side of a Joy-Con for binding purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoyConSide {
    Left,
    Right,
}

impl JoyConSide {
    pub fn from_joycon_type(jt: JoyConType) -> Self {
        match jt {
            JoyConType::Left => Self::Left,
            JoyConType::Right => Self::Right,
            // Pro Controllers default to "Right" (primary side).
            _ => Self::Right,
        }
    }
}

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

fn is_note_off(cmd: &RumbleCommand, prev_cmd: Option<&RumbleCommand>) -> bool {
    match prev_cmd {
        Some(prev) => prev.amplitude > 0.0 && cmd.amplitude == 0.0,
        None => false,
    }
}

/// Spawns a thread that reads keyboard events and mutates the binding.
fn spawn_input_thread(
    binding: Arc<Mutex<JoyConBinding>>,
    quit: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        // Enable raw mode so key-presses arrive immediately.
        let raw_ok = crossterm::terminal::enable_raw_mode().is_ok();

        while !quit.load(Ordering::Relaxed) {
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                })) = event::read()
                {
                    match code {
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            if let Ok(mut b) = binding.lock() {
                                b.swap();
                            }
                        }
                        KeyCode::Char('1') => {
                            if let Ok(mut b) = binding.lock() {
                                b.cycle_primary();
                            }
                        }
                        KeyCode::Char('2') => {
                            if let Ok(mut b) = binding.lock() {
                                b.cycle_secondary();
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            println!("\n⏹  Quitting playback…");
                            quit.store(true, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }
            }
        }

        if raw_ok {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    })
}

/// Plays a MIDI file through connected JoyCons with runtime L/R swap support.
///
/// # Runtime Controls
///
/// While the song is playing press:
/// - **S** to swap which Joy-Con plays the primary (melody) part
/// - **1** to cycle to the next primary candidate
/// - **2** to cycle to the next secondary candidate
/// - **Q** or **Esc** to stop playback
pub fn play_midi_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let manager = JoyConManager::new()?;
    let joycons = manager.connect_and_initialize_joycons()?;
    let num_joycons = joycons.len();

    println!("🎵 Loading MIDI file: {:?}", path);
    let midi_data = std::fs::read(&path)?;

    let (tracks, plan, selection) = parse_midi_to_rumble(&midi_data, num_joycons)?;

    println!("\nAvailable parts (rumble tracks): {}", tracks.len());
    for (idx, track) in tracks.iter().enumerate() {
        println!(
            "  Part {} : {} notes, {} cmds, dur={:.1}s, type={:?}, name={:?}",
            idx,
            track.metrics.note_count,
            track.commands.len(),
            track.total_duration.as_secs_f32(),
            track.metrics.track_type,
            track.metrics.track_name.as_deref().unwrap_or("-")
        );
    }

    let binding = Arc::new(Mutex::new(JoyConBinding::new(&selection)));
    let quit = Arc::new(AtomicBool::new(false));
    let start_signal = Arc::new(Mutex::new(false));

    // Spawn the keyboard input thread.
    let input_handle = spawn_input_thread(Arc::clone(&binding), Arc::clone(&quit));

    let mut handles: Vec<thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>> =
        Vec::new();

    for (joycon_idx, mut joycon) in joycons.into_iter().enumerate() {
        let joycon_signal = Arc::clone(&start_signal);
        let joycon_tracks = tracks.clone();
        let joycon_plan = plan.clone();
        let joycon_binding = Arc::clone(&binding);
        let joycon_quit = Arc::clone(&quit);
        let side = JoyConSide::from_joycon_type(joycon.get_type());

        handles.push(thread::spawn(move || {
            while !*joycon_signal.lock().unwrap_or_else(|e| e.into_inner()) {
                thread::sleep(Duration::from_millis(1));
            }

            let playback_start = Instant::now();
            let mut last_write = playback_start;

            // Resolve which track this Joy-Con should start on.
            let mut current_track_idx = joycon_binding
                .lock()
                .map(|b| b.track_for_side(side))
                .unwrap_or(0);
            let mut command_index = 0;
            let mut scheduled_time = Duration::ZERO;
            let mut pending_track_switch: Option<usize> = None;
            let mut next_section_time = joycon_plan.next_section_time(Duration::ZERO);

            println!(
                "🎮 JoyCon {} ({:?}) starting on part {}",
                joycon_idx + 1,
                side,
                current_track_idx
            );

            loop {
                if joycon_quit.load(Ordering::Relaxed) {
                    break;
                }

                let current_time = playback_start.elapsed();

                // Check if the binding changed (swap / cycle).
                let desired_track = joycon_binding
                    .lock()
                    .map(|b| b.track_for_side(side))
                    .unwrap_or(current_track_idx);

                if desired_track != current_track_idx && desired_track < joycon_tracks.len() {
                    current_track_idx = desired_track;
                    command_index = find_commands_at_time(
                        &joycon_tracks[current_track_idx].commands,
                        current_time,
                    );
                    pending_track_switch = None;
                }

                let track = &joycon_tracks[current_track_idx];

                if command_index >= track.commands.len() {
                    let mut found_next = false;
                    let mut scan_time = next_section_time;
                    while let Some(boundary) = scan_time {
                        let candidate = joycon_plan.track_for(joycon_idx, boundary);
                        if candidate != current_track_idx {
                            let ci = find_commands_at_time(
                                &joycon_tracks[candidate].commands,
                                current_time,
                            );
                            if ci < joycon_tracks[candidate].commands.len() {
                                current_track_idx = candidate;
                                command_index = ci;
                                next_section_time = joycon_plan.next_section_time(boundary);
                                found_next = true;
                                break;
                            }
                        }
                        scan_time = joycon_plan.next_section_time(boundary);
                    }
                    if !found_next {
                        break;
                    }
                    continue;
                }

                // Section boundary crossing.
                if let Some(boundary) = next_section_time {
                    if current_time >= boundary {
                        let new_track = joycon_plan.track_for(joycon_idx, current_time);
                        if new_track != current_track_idx {
                            pending_track_switch = Some(new_track);
                        }
                        next_section_time = joycon_plan.next_section_time(current_time);
                    }
                }

                let cmd = &track.commands[command_index];
                let prev_cmd = if command_index > 0 {
                    Some(&track.commands[command_index - 1])
                } else {
                    None
                };

                // Execute pending switch at note-off boundary.
                if let Some(new_track_idx) = pending_track_switch {
                    if is_note_off(cmd, prev_cmd) {
                        current_track_idx = new_track_idx;
                        command_index = find_commands_at_time(
                            &joycon_tracks[current_track_idx].commands,
                            current_time,
                        );
                        pending_track_switch = None;
                        continue;
                    }
                }

                // Sleep until the scheduled fire time, absorbing any prior
                // oversleep or HID-I/O overhead automatically.
                if !cmd.wait_before.is_zero() {
                    scheduled_time += cmd.wait_before;
                    let target = playback_start + scheduled_time;
                    let now = Instant::now();
                    if target > now {
                        thread::sleep(target - now);
                    }
                }

                // Coalesce consecutive zero-wait commands (same-tick events).
                // The JoyCon plays one frequency at a time, so only the last
                // state at each tick is audible.
                while command_index + 1 < track.commands.len()
                    && track.commands[command_index + 1].wait_before.is_zero()
                {
                    command_index += 1;
                }
                let cmd = &track.commands[command_index];

                if cmd.amplitude > 0.0 {
                    println!(
                        "🎮 JoyCon {} playing: freq={:.1}, amp={:.2}, t={:.2}s",
                        joycon_idx + 1,
                        cmd.frequency,
                        cmd.amplitude,
                        playback_start.elapsed().as_secs_f32()
                    );
                }

                // Throttle HID writes to avoid overwhelming the USB pipe.
                const MIN_HID_INTERVAL: Duration = Duration::from_millis(2);
                let since_last = last_write.elapsed();
                if since_last < MIN_HID_INTERVAL {
                    thread::sleep(MIN_HID_INTERVAL - since_last);
                }

                joycon.rumble(cmd.frequency, cmd.amplitude)?;
                last_write = Instant::now();
                command_index += 1;
            }

            println!("🎮 JoyCon {} stopping", joycon_idx + 1);
            joycon.rumble(0.0, 0.0)?;
            Ok(())
        }));
    }

    println!("\n▶️  Starting playback…");
    println!("    S = swap L/R  |  1 = cycle primary  |  2 = cycle secondary  |  Q = quit\n");
    *start_signal.lock().unwrap_or_else(|e| e.into_inner()) = true;

    for handle in handles {
        match handle.join() {
            Ok(result) => result?,
            Err(_) => return Err("A playback thread panicked".into()),
        }
    }

    // Signal the input thread to stop and wait for it.
    quit.store(true, Ordering::Relaxed);
    let _ = input_handle.join();

    println!("✨ Playback complete!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::scoring::PartSelection;

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

        assert!(!is_note_off(&note_on, None));
        assert!(!is_note_off(&note_off, None));
        assert!(is_note_off(&note_off, Some(&note_on)));
        assert!(!is_note_off(&note_on, Some(&note_off)));
        assert!(!is_note_off(&note_on, Some(&note_on)));
        assert!(!is_note_off(&note_off, Some(&note_off)));
    }

    #[test]
    fn test_binding_swap() {
        let sel = PartSelection {
            primary: 0,
            secondary: 1,
            primary_candidates: vec![0, 2],
            secondary_candidates: vec![1, 3],
        };
        let mut binding = JoyConBinding::new(&sel);

        assert!(binding.primary_on_right);
        assert_eq!(binding.track_for_side(JoyConSide::Right), 0);
        assert_eq!(binding.track_for_side(JoyConSide::Left), 1);

        binding.swap();

        assert!(!binding.primary_on_right);
        assert_eq!(binding.track_for_side(JoyConSide::Right), 1);
        assert_eq!(binding.track_for_side(JoyConSide::Left), 0);
    }

    #[test]
    fn test_binding_cycle_primary() {
        let sel = PartSelection {
            primary: 0,
            secondary: 1,
            primary_candidates: vec![0, 2, 4],
            secondary_candidates: vec![1],
        };
        let mut binding = JoyConBinding::new(&sel);

        binding.cycle_primary();
        assert_eq!(binding.primary_part_idx, 2);

        binding.cycle_primary();
        assert_eq!(binding.primary_part_idx, 4);

        binding.cycle_primary();
        assert_eq!(binding.primary_part_idx, 0); // wraps around
    }

    #[test]
    fn test_binding_cycle_secondary() {
        let sel = PartSelection {
            primary: 0,
            secondary: 1,
            primary_candidates: vec![0],
            secondary_candidates: vec![1, 3],
        };
        let mut binding = JoyConBinding::new(&sel);

        binding.cycle_secondary();
        assert_eq!(binding.secondary_part_idx, 3);

        binding.cycle_secondary();
        assert_eq!(binding.secondary_part_idx, 1); // wraps
    }
}
