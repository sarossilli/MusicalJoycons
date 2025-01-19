use musical_joycons::midi::playback::play_midi_file;
use std::io::{self};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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