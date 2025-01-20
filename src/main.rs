use musical_joycons::device_audio::audio_input::AudioInput;
use musical_joycons::joycon::{JoyCon, JoyConManager};
use musical_joycons::midi::playback::play_midi_file;
use std::io::{self};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🎮 Musical JoyCons - MIDI Player & Audio Capture");
    println!("=================================================");
    println!("Choose an option:");
    println!("1. Play MIDI file");
    println!("2. Capture audio input");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: u32 = input.trim().parse()?;

    match choice {
        1 => {
            println!("🎶 Enter the path of your MIDI file:");
            let mut midi_path = String::new();
            io::stdin().read_line(&mut midi_path)?;
            let path = PathBuf::from(midi_path.trim().trim_matches('"'));

            if !path.exists() {
                println!("❌ File not found: {:?}", path);
                return Ok(());
            }

            // Play the MIDI file
            play_midi_file(path)?;
        }
        2 => {
            println!("🎤 Capturing audio...");
            
            // Initialize JoyCons
            let joycons = initialize_joycons()?;
            
            if joycons.is_empty() {
                println!("❌ No JoyCons found!");
                return Ok(());
            }

            println!("✅ Found {} JoyCons", joycons.len());

            // Create audio input with JoyCons
            let mut audio_input = AudioInput::new_with_joycons(joycons)?;
            audio_input.start()?;
        }
        _ => {
            println!("❌ Invalid choice.");
        }
    }

    Ok(())
}

fn initialize_joycons() -> Result<Vec<JoyCon>, Box<dyn std::error::Error + Send + Sync>> {
    let mut manager = JoyConManager::new()?;
    let devices = manager.scan_for_devices()?;
    let mut joycons = Vec::new();

    for device in devices {
        // The JoyCon is already initialized in scan_for_devices
        joycons.push(device);
    }

    if !joycons.is_empty() {
        println!("✅ Successfully connected to {} JoyCon(s)", joycons.len());
    }

    Ok(joycons)
}
