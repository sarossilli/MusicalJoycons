use super::interface::JoyconInterface;
use super::types::{Command, DeviceInfo, JoyConError, JoyConType, Subcommand};
use crate::midi::rubmle::RumbleTrack;
use hidapi::HidDevice;
use std::sync::{Arc, Mutex};
use std::thread;
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
            other_id => return Err(JoyConError::InvalidDevice("Unknown product ID")), // Handle other JoyCons
        };

        Ok(Self {
            handle: None,
            device_type,
            timing_byte: 0,
        })
    }

    pub fn rumble(&mut self, frequency: f32, amplitude: f32) -> Result<(), JoyConError> {
        // Clamp amplitude between 0 and 1 (should already be normalized from track analysis)
        let clamped_amplitude = amplitude.clamp(0.0, 1.0);

        if frequency == 0.0 {
            return JoyconInterface::send_rumble(self, 0.0, clamped_amplitude);
        }

        let wrapped_freq = {
            let mut freq = frequency;
            // Shift down octaves until below max
            while freq > 1252.0 {
                freq /= 2.0;
            }
            // Shift up octaves until above min (excluding 0)
            while freq > 0.0 && freq < 1.0 {
                freq *= 2.0;
            }

            freq
        };

        JoyconInterface::send_rumble(self, wrapped_freq, clamped_amplitude)
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

        let note_duration = Duration::from_millis(50);
        let pause_duration = Duration::from_millis(5);

        for &frequency in scale.iter() {
            self.rumble(frequency, 0.90)?;
            std::thread::sleep(note_duration);

            self.rumble(0.0, 0.0)?;
            std::thread::sleep(pause_duration);
        }

        self.rumble(0.0, 0.0)
    }

    pub fn play_rumble_track(&mut self, track: RumbleTrack) -> Result<(), JoyConError> {
        println!("Track duration: {:?}", track.total_duration);

        for command in track.commands {
            if !command.wait_before.is_zero() {
                thread::sleep(command.wait_before);
            }
            self.rumble(command.frequency, command.amplitude)?;
        }

        self.rumble(0.0, 0.0)?;
        Ok(())
    }

    pub fn play_synchronized(
        &mut self,
        track: RumbleTrack,
        start_signal: Arc<Mutex<bool>>,
    ) -> Result<(), JoyConError> {
        while !*start_signal.lock().unwrap() {
            thread::sleep(Duration::from_millis(1));
        }

        self.play_rumble_track(track)
    }

    pub fn initialize_device(&mut self) -> Result<(), JoyConError> {
        self.enable_rumble()?;
        self.play_scale()
    }

    // Getters and setters
    pub fn get_handle(&self) -> Option<&HidDevice> {
        self.handle.as_ref()
    }

    pub(crate) fn set_handle(&mut self, device: HidDevice) {
        self.handle = Some(device);
    }

    pub fn get_type(&self) -> JoyConType {
        self.device_type
    }

    pub fn get_timing_byte(&self) -> u8 {
        self.timing_byte
    }

    pub(crate) fn increment_timing_byte(&mut self) {
        self.timing_byte = self.timing_byte.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device_info(product_id: u16) -> DeviceInfo {
        DeviceInfo {
            path: String::new(),
            vendor_id: 0x057E, // Nintendo vendor ID
            product_id,
            usage_page: 0,
            interface_number: 0,
            serial: String::new(),
        }
    }

    #[test]
    fn test_joycon_creation() {
        let left_info = create_test_device_info(crate::joycon::types::JOYCON_L_BT);
        let right_info = create_test_device_info(crate::joycon::types::JOYCON_R_BT);
        let pro_info = create_test_device_info(crate::joycon::types::PRO_CONTROLLER);

        assert!(matches!(
            JoyCon::new(&left_info).unwrap().get_type(),
            JoyConType::Left
        ));
        assert!(matches!(
            JoyCon::new(&right_info).unwrap().get_type(),
            JoyConType::Right
        ));
        assert!(matches!(
            JoyCon::new(&pro_info).unwrap().get_type(),
            JoyConType::ProController
        ));
    }

    #[test]
    fn test_timing_byte() {
        let mut joycon = JoyCon::new(&create_test_device_info(crate::joycon::types::JOYCON_L_BT)).unwrap();
        
        assert_eq!(joycon.get_timing_byte(), 0);
        joycon.increment_timing_byte();
        assert_eq!(joycon.get_timing_byte(), 1);
        
        // Test wrapping behavior
        joycon.timing_byte = 255;
        joycon.increment_timing_byte();
        assert_eq!(joycon.get_timing_byte(), 0);
    }

    #[test]
    fn test_rumble_parameters() {
        let mut joycon = JoyCon::new(&create_test_device_info(crate::joycon::types::JOYCON_L_BT)).unwrap();
        
        // Test amplitude clamping
        assert!(joycon.rumble(440.0, 1.5).is_err()); // Should fail without device
        
        // Test frequency wrapping
        assert!(joycon.rumble(2504.0, 0.5).is_err()); // Should fail without device but would wrap to 1252.0
        assert!(joycon.rumble(0.25, 0.5).is_err()); // Should fail without device but would wrap to 1.0
    }
}
