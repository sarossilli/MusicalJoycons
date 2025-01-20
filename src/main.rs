use musical_joycons::device_audio::audio_input::AudioInput;
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

            // Create a new instance of AudioInput
            let mut audio_input = AudioInput::new()?;

            // Start capturing audio
            audio_input.start()?;

            // Capture audio and process it in the callback
            audio_input.capture(|buffer| {
                // Print out the captured audio data (sample preview)
                println!("Captured audio buffer with {} samples.", buffer.len());
                // You can add further processing here if needed (e.g., FFT)
            });

            println!("Audio capture started. Press Enter to stop.");
            let mut stop = String::new();
            io::stdin().read_line(&mut stop)?;
        }
        _ => {
            println!("❌ Invalid choice.");
        }
    }

    Ok(())
}
