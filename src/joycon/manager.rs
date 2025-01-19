use super::joycon::JoyCon;
use super::types::{DeviceInfo, JoyConError};
use hidapi::HidApi;
use std::time::Duration;

pub struct JoyConManager {
    api: HidApi,
}

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);

impl JoyConManager {
    pub fn new() -> Result<Self, JoyConError> {
        let api = HidApi::new().map_err(|e| JoyConError::HidError(e.to_string()))?;
        Ok(Self { api })
    }

    pub fn scan_for_devices(&self) -> Result<Vec<JoyCon>, JoyConError> {
        let mut joycons = Vec::new();

        for device_info in self.api.device_list() {
            if device_info.vendor_id() == crate::joycon::types::VENDOR_ID {
                match device_info.product_id() {
                    id @ (crate::joycon::types::JOYCON_L_BT
                    | crate::joycon::types::JOYCON_R_BT
                    | crate::joycon::types::PRO_CONTROLLER) => {
                        println!(
                            "Found device: VID={:04x} PID={:04x}",
                            device_info.vendor_id(),
                            device_info.product_id()
                        );

                        if let Ok(device) = self.api.open_path(device_info.path()) {
                            let device_info = DeviceInfo {
                                product_id: id,
                                interface_number: device_info.interface_number(),
                                serial: device_info
                                    .serial_number()
                                    .unwrap_or("Unknown")
                                    .to_string(),
                                path: device_info.path().to_string_lossy().into_owned(),
                                vendor_id: device_info.vendor_id() as i32,
                                usage_page: device_info.usage_page() as i32,
                            };

                            if let Ok(mut joycon) = JoyCon::new(&device_info) {
                                joycon.set_handle(device);
                                joycons.push(joycon);
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }

        Ok(joycons)
    }

    pub fn connect_and_initialize_joycons(&self) -> Result<Vec<JoyCon>, JoyConError> {
        let mut tries = 0;

        println!("🔍 Scanning for JoyCons...");

        while tries < MAX_RETRIES {
            match self.scan_for_devices() {
                Ok(mut joycons) => {
                    if !joycons.is_empty() {
                        self.initialize_joycons(&mut joycons)?;
                        return Ok(joycons);
                    }
                    self.print_retry_message(tries);
                }
                Err(e) => {
                    println!("❌ Error scanning for devices: {}", e);
                    self.print_retry_message(tries);
                }
            }

            std::thread::sleep(RETRY_DELAY);
            tries += 1;
        }

        Err(JoyConError::NotConnected)
    }

    fn initialize_joycons(&self, joycons: &mut [JoyCon]) -> Result<(), JoyConError> {
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

    fn print_retry_message(&self, tries: u32) {
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
}
