use musical_joycons::joycon::scan_for_devices;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_secs(5);

    let mut tries = 0;
    let mut joycons = Vec::new();

    println!("🔍 Scanning for JoyCons...");

    while tries < MAX_RETRIES {
        match scan_for_devices() {
            Ok(discovered) => {
                if !discovered.is_empty() {
                    joycons = discovered;
                    break;
                }
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
            Err(e) => {
                println!("❌ Error scanning for devices: {}", e);
                println!(
                    "Retrying in {} seconds... (Attempt {}/{})",
                    RETRY_DELAY.as_secs(),
                    tries + 1,
                    MAX_RETRIES
                );
            }
        }

        std::thread::sleep(RETRY_DELAY);
        tries += 1;
    }

    if joycons.is_empty() {
        println!(
            "❌ No JoyCons found after {} attempts. Please check your hardware and try again.",
            MAX_RETRIES
        );
        std::process::exit(1);
    }

    println!("✅ Found {} JoyCon(s)!", joycons.len());

    for (i, mut joycon) in joycons.into_iter().enumerate() {
        println!("🎮 Initializing JoyCon {}", i + 1);
        match joycon.initialize_device() {
            Ok(_) => println!("✅ JoyCon {} initialized successfully", i + 1),
            Err(e) => println!("❌ Failed to initialize JoyCon {}: {}", i + 1, e),
        }

        if let Err(e) = joycon.pulse_rumble() {
            println!("❌ Rumble test failed for JoyCon {}: {}", i + 1, e);
        } else {
            println!("✨ Rumble test successful for JoyCon {}", i + 1);
        }
    }

    println!("🎵 Ready to make some music!");
    Ok(())
}
