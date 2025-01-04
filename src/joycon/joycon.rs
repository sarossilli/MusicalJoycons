// src/joycon/joycon.rs
use super::interface::JoyconInterface;
use super::types::{Command, DeviceInfo, JoyConError, JoyConType, Subcommand};
use hidapi::HidDevice;
use std::time::Duration;

pub struct JoyCon {
    handle: Option<HidDevice>,
    device_type: JoyConType,
    timing_byte: u8,
}

impl JoyCon {
    pub fn new(device_info: &DeviceInfo) -> Result<Self, JoyConError> {
        let device_type = match device_info.product_id {
            crate::joycon::types::JOYCON_L_BT => JoyConType::Left,
            crate::joycon::types::JOYCON_R_BT => JoyConType::Right,
            crate::joycon::types::PRO_CONTROLLER => JoyConType::ProController,
            crate::joycon::types::JOYCON_CHARGING_GRIP => match device_info.interface_number {
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

    // Public interface
    pub fn rumble(&mut self, frequency: f32, amplitude: f32) -> Result<(), JoyConError> {
        if !(0.0..=1252.0).contains(&frequency) {
            return Err(JoyConError::InvalidRumble(
                "Frequency out of range (0-1252 Hz)",
            ));
        }
        if !(0.0..=1.0).contains(&amplitude) {
            return Err(JoyConError::InvalidRumble("Amplitude out of range (0-1.0)"));
        }

        JoyconInterface::send_rumble(self, frequency, amplitude)
    }

    pub fn enable_rumble(&mut self) -> Result<(), JoyConError> {
        JoyconInterface::send_command(
            self,
            Command::Rumble,
            Some(Subcommand::EnableVibration),
            &[0x01],
        )
    }
    pub fn play_scale(&mut self) -> Result<(), JoyConError> {
        // Define a major scale frequencies (approximate musical notes)
        // Starting from middle C (262 Hz) going up to the next C
        let scale = [
            524.0,  // C5
            588.0,  // D5
            660.0,  // E5
            698.0,  // F5
            784.0,  // G5
            880.0,  // A5
            988.0,  // B5
            1046.0, // C6
        ];

        // Duration for each note
        let note_duration = Duration::from_millis(500);
        // Short pause between notes
        let pause_duration = Duration::from_millis(500);

        for &frequency in scale.iter() {
            // Play the note
            self.rumble(frequency, 0.90)?;
            std::thread::sleep(note_duration);

            // Brief pause between notes
            self.rumble(0.0, 0.0)?;
            std::thread::sleep(pause_duration);
        }

        // Ensure rumble is off at the end
        self.rumble(0.0, 0.0)
    }

    pub fn initialize_device(&mut self) -> Result<(), JoyConError> {
        self.enable_rumble()?;
        self.play_scale()
    }

    // Getters and setters
    pub(crate) fn get_handle(&self) -> Option<&HidDevice> {
        self.handle.as_ref()
    }

    pub(crate) fn set_handle(&mut self, device: HidDevice) {
        self.handle = Some(device);
    }

    pub(crate) fn get_type(&self) -> JoyConType {
        self.device_type
    }

    pub(crate) fn get_timing_byte(&self) -> u8 {
        self.timing_byte
    }

    pub(crate) fn increment_timing_byte(&mut self) {
        self.timing_byte = self.timing_byte.wrapping_add(1);
    }
}
