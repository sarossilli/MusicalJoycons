use musical_joycons::joycon::{JoyCon, JoyConError, JoyConManager};
use musical_joycons::midi::rubmle::parse_midi_to_rumble;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎮 Musical JoyCons - MIDI Player");
    println!("===============================");

    println!("Drag and drop your MIDI file into this terminal and press Enter:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let path = PathBuf::from(input.trim().trim_matches('"'));

    if !path.exists() {
        println!("❌ File not found: {:?}", path);
        return Ok(());
    }

    play_midi_file(path)?;

    Ok(())
}

fn get_user_track_selection(track_count: usize, prompt: &str) -> Option<usize> {
    println!("\n{}", prompt);
    println!("Enter track number or press Enter for automatic selection:");

    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;

    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    match input.parse::<usize>() {
        Ok(index) if index < track_count => Some(index),
        _ => {
            println!("❌ Invalid track number, using automatic selection");
            None
        }
    }
}

fn play_midi_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Connect to JoyCons
    let joycons = connect_to_joycons()?;

    println!("🎵 Loading MIDI file: {:?}", path);
    let midi_data = std::fs::read(&path)?;

    // First parse to show track information and get track count
    let tracks = parse_midi_to_rumble(&midi_data, vec![None; joycons.len()])?;

    // Get track count from output
    println!("\nWould you like to select specific tracks? y/n (Press Enter to skip)");
    println!("Note: Automatic selection will choose the best tracks for each JoyCon");

    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let track_selections: Vec<Option<usize>> = if input.trim().is_empty() {
        vec![None; joycons.len()]
    } else {
        (0..joycons.len())
            .map(|i| {
                get_user_track_selection(
                    tracks.len(),
                    &format!("Select track for JoyCon {}:", i + 1),
                )
            })
            .collect()
    };

    // Parse again with user selections
    let tracks = parse_midi_to_rumble(&midi_data, track_selections)?;

    // Create synchronization signal
    let start_signal = Arc::new(Mutex::new(false));
    let mut handles = Vec::new();

    // Assign tracks to JoyCons
    for (i, mut joycon) in joycons.into_iter().enumerate() {
        if let Some(track) = tracks.get(i) {
            println!("🎮 JoyCon {} will play track {}", i + 1, i + 1);
            let joycon_track = track.clone();
            let joycon_signal = Arc::clone(&start_signal);
            handles.push(thread::spawn(move || {
                joycon.play_synchronized(joycon_track, joycon_signal)
            }));
        }
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
