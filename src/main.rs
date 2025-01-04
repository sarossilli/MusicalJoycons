use musical_joycons::joycon::{JoyCon, JoyConError, JoyConManager};
use std::time::Duration;

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let joycons = connect_to_joycons()?;
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

    println!("🎵 Ready to make some music!");
    Ok(())
}