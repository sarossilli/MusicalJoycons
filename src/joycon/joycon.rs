use super::types::{DeviceInfo, JoyCon, JoyConError, JoyConType, RumbleState};
use std::time::Duration;

impl JoyCon {
    pub fn new(device_info: &DeviceInfo) -> Result<Self, JoyConError> {
        let device_type = match device_info.product_id {
            super::types::JOYCON_L_BT => JoyConType::Left,
            super::types::JOYCON_R_BT => JoyConType::Right,
            super::types::PRO_CONTROLLER => JoyConType::ProController,
            super::types::JOYCON_CHARGING_GRIP => match device_info.interface_number {
                0 | -1 => JoyConType::Right,
                1 => JoyConType::Left,
                _ => return Err(JoyConError::InvalidDevice("Unknown interface")),
            },
            _ => return Err(JoyConError::InvalidDevice("Unknown product ID")),
        };

        Ok(Self {
            handle: None,
            device_type,
            timing_byte: 0,
        })
    }

    // High-level rumble control functions
    pub fn set_rumble(&mut self, frequency: f32, amplitude: f32) -> Result<(), JoyConError> {
        if !(0.0..=1252.0).contains(&frequency) {
            return Err(JoyConError::InvalidRumble(
                "Frequency out of range (0-1252 Hz)",
            ));
        }
        if !(0.0..=1.0).contains(&amplitude) {
            return Err(JoyConError::InvalidRumble("Amplitude out of range (0-1.0)"));
        }

        // Update internal state
        self.encode_and_send_rumble(frequency, amplitude)
    }

    pub fn pulse_rumble(&mut self) -> Result<(), JoyConError> {
        // Test with known good values from the frequency/amplitude tables
        self.set_rumble(600.0, 0.99)?; // Low frequency rumble
        std::thread::sleep(Duration::from_millis(200));

        self.set_rumble(600.0, 0.0)?; // Mid frequency
        std::thread::sleep(Duration::from_millis(200));

        self.set_rumble(600.0, 0.99)?; // Higher frequency
        std::thread::sleep(Duration::from_millis(200));

        self.set_rumble(0.0, 0.0)?; // Stop
        Ok(())
    }

    pub fn initialize_device(&mut self) -> Result<(), JoyConError> {
        self.enable_rumble()?;
        self.pulse_rumble()?;
        Ok(())
    }

    // Helper functions that delegate to bluetooth.rs
    fn encode_and_send_rumble(
        &mut self,
        frequency: f32,
        amplitude: f32,
    ) -> Result<(), JoyConError> {
        super::bluetooth::encode_and_send_rumble(self, frequency, amplitude)
    }

    fn enable_rumble(&mut self) -> Result<(), JoyConError> {
        super::bluetooth::enable_rumble(self)
    }
}
