use musical_joycons::joycon::{JoyCon, JoyConError, JoyConManager, JoyConType};
use musical_joycons::midi::rubmle::parse_midi_to_rumble;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Print welcome message
    println!("🎮 Musical JoyCons - MIDI Player");
    println!("===============================");

    // Get MIDI file path from user
    print!("Enter the path to your MIDI file: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let path = PathBuf::from(input.trim());

    if !path.exists() {
        println!("❌ File not found: {:?}", path);
        return Ok(());
    }

    play_midi_file(path)?;

    println!("\nPress Enter to exit...");
    input.clear();
    io::stdin().read_line(&mut input)?;

    Ok(())
}

fn play_midi_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Read and parse the MIDI file
    println!("🎵 Loading MIDI file: {:?}", path);
    let midi_data = std::fs::read(path)?;
    let (primary_track, secondary_track) = parse_midi_to_rumble(&midi_data)?;

    println!(
        "✨ Primary track duration: {:?}",
        primary_track.total_duration
    );
    if let Some(ref secondary) = secondary_track {
        println!(
            "✨ Secondary track duration: {:?}",
            secondary.total_duration
        );
    }

    // Connect to JoyCons
    let mut joycons = connect_to_joycons()?;
    let mut right_joycon = None;
    let mut left_joycon = None;

    // Sort JoyCons by type
    for joycon in joycons.drain(..) {
        match joycon.get_type() {
            JoyConType::Right => right_joycon = Some(joycon),
            JoyConType::Left => left_joycon = Some(joycon),
            _ => println!("⚠️ Unsupported controller type detected"),
        }
    } // Create synchronization signal with countdown
    let start_signal = Arc::new(Mutex::new(false));
    let mut handles = Vec::new();

    // Assign tracks to JoyCons
    if let Some(mut right) = right_joycon {
        println!("🎮 Right JoyCon will play primary track");
        let right_track = primary_track.clone();
        let right_signal = Arc::clone(&start_signal);
        handles.push(thread::spawn(move || {
            right.play_synchronized(right_track, right_signal)
        }));
    }

    if let Some(mut left) = left_joycon {
        if let Some(left_track) = secondary_track {
            println!("🎮 Left JoyCon will play secondary track");
            let left_signal = Arc::clone(&start_signal);
            handles.push(thread::spawn(move || {
                left.play_synchronized(left_track, left_signal)
            }));
        }
    }
    *start_signal.lock().unwrap() = true;

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
                if !joycons.is_empty() {
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
    println!("❌ No JoyCons found. Are they in pairing mode?");
    println!("   - Press the sync button on your JoyCon");
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
