//! JoyCon device discovery and management.

use std::time::Duration;

use hidapi::HidApi;

use super::device::JoyCon;
use super::types::{DeviceInfo, JoyConError, JOYCON_L_BT, JOYCON_R_BT, PRO_CONTROLLER, VENDOR_ID};

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Manages discovery and connection to JoyCon devices.
///
/// # Example
///
/// ```no_run
/// use musical_joycons::joycon::JoyConManager;
///
/// let manager = JoyConManager::new().expect("Failed to initialize HID API");
/// let joycons = manager.connect_and_initialize_joycons().expect("No JoyCons found");
/// println!("Found {} JoyCon(s)", joycons.len());
/// ```
pub struct JoyConManager {
    api: HidApi,
}

impl JoyConManager {
    /// Creates a new JoyConManager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the HID API cannot be initialized.
    pub fn new() -> Result<Self, JoyConError> {
        let api = HidApi::new().map_err(|e| JoyConError::HidError(e.to_string()))?;
        Ok(Self { api })
    }

    /// Scans for connected JoyCon devices.
    ///
    /// Returns a list of discovered and opened JoyCon devices.
    pub fn scan_for_devices(&self) -> Result<Vec<JoyCon>, JoyConError> {
        let mut joycons = Vec::new();

        for device_info in self.api.device_list() {
            if device_info.vendor_id() == VENDOR_ID {
                match device_info.product_id() {
                    id @ (JOYCON_L_BT | JOYCON_R_BT | PRO_CONTROLLER) => {
                        println!(
                            "Found device: VID={:04x} PID={:04x}",
                            device_info.vendor_id(),
                            device_info.product_id()
                        );

                        if let Ok(device) = self.api.open_path(device_info.path()) {
                            let info = DeviceInfo {
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

                            if let Ok(mut joycon) = JoyCon::new(&info) {
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

    /// Scans for JoyCons with retries, then initializes them.
    ///
    /// This method will retry scanning for devices up to 5 times with 5 second
    /// delays between attempts. Once devices are found, they are initialized
    /// and ready for use.
    ///
    /// # Errors
    ///
    /// Returns `JoyConError::NotConnected` if no devices are found after all retries.
    pub fn connect_and_initialize_joycons(&self) -> Result<Vec<JoyCon>, JoyConError> {
        let mut tries = 0;

        println!("Scanning for JoyCons...");

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
                    eprintln!("Error scanning for devices: {}", e);
                    self.print_retry_message(tries);
                }
            }

            std::thread::sleep(RETRY_DELAY);
            tries += 1;
        }

        Err(JoyConError::NotConnected)
    }

    fn initialize_joycons(&self, joycons: &mut [JoyCon]) -> Result<(), JoyConError> {
        println!("Found {} JoyCon(s)!", joycons.len());

        for (i, joycon) in joycons.iter_mut().enumerate() {
            println!("Initializing JoyCon {}", i + 1);
            match joycon.initialize_device() {
                Ok(()) => println!("JoyCon {} initialized successfully", i + 1),
                Err(e) => eprintln!("Failed to initialize JoyCon {}: {}", i + 1, e),
            }
        }

        Ok(())
    }

    fn print_retry_message(&self, tries: u32) {
        eprintln!("No JoyCons found. Are they connected to your PC?");
        eprintln!("  - Check your Bluetooth devices connected");
        eprintln!("  - Make sure the JoyCon is charged");
        println!(
            "Retrying in {} seconds... (Attempt {}/{})",
            RETRY_DELAY.as_secs(),
            tries + 1,
            MAX_RETRIES
        );
    }
}
