//! Individual JoyCon device control.
//!
//! This module provides the [`JoyCon`] struct for controlling individual
//! Nintendo JoyCon or Pro Controller devices.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hidapi::HidDevice;

use super::interface::JoyconInterface;
use super::types::{
    Command, DeviceInfo, JoyConError, JoyConType, Subcommand, JOYCON_CHARGING_GRIP, JOYCON_L_BT,
    JOYCON_R_BT, PRO_CONTROLLER,
};
use crate::midi::RumbleTrack;

/// Represents a connected Nintendo JoyCon or Pro Controller.
///
/// This struct provides methods for controlling the rumble motor of a JoyCon,
/// including playing individual frequencies, scales, and complete MIDI tracks.
///
/// # Creating a JoyCon
///
/// JoyCons are typically created through [`JoyConManager`](super::JoyConManager),
/// which handles device discovery and connection:
///
/// ```no_run
/// use musical_joycons::joycon::JoyConManager;
///
/// let manager = JoyConManager::new()?;
/// let joycons = manager.connect_and_initialize_joycons()?;
/// # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
/// ```
///
/// # Rumble Control
///
/// The JoyCon rumble motor can produce frequencies from approximately 40 Hz to 1252 Hz.
/// Frequencies outside this range are automatically octave-shifted:
///
/// ```no_run
/// # use musical_joycons::joycon::JoyConManager;
/// # let manager = JoyConManager::new()?;
/// # let mut joycons = manager.connect_and_initialize_joycons()?;
/// # let joycon = joycons.first_mut().unwrap();
/// // Play middle A (440 Hz) at 70% amplitude
/// joycon.rumble(440.0, 0.7)?;
///
/// // Higher frequencies work too (will be octave-shifted if needed)
/// joycon.rumble(880.0, 0.5)?;
///
/// // Stop rumbling
/// joycon.rumble(0.0, 0.0)?;
/// # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
/// ```
///
/// # Thread Safety
///
/// `JoyCon` is not `Sync` due to the underlying HID device handle.
/// For multi-threaded access, wrap in `Arc<Mutex<JoyCon>>`.
pub struct JoyCon {
    /// HID device handle for communication
    handle: Option<HidDevice>,
    /// Type of JoyCon (Left, Right, Pro Controller, etc.)
    device_type: JoyConType,
    /// Packet counter for HID communication (wraps at 255)
    timing_byte: u8,
}

impl JoyCon {
    /// Creates a new JoyCon instance from device information.
    ///
    /// This is typically called by [`JoyConManager`](super::JoyConManager) during
    /// device discovery. The device handle must be set separately via `set_handle`.
    ///
    /// # Arguments
    ///
    /// * `device_info` - Information about the HID device including product ID
    ///
    /// # Returns
    ///
    /// A new `JoyCon` instance without an active connection, or an error if
    /// the device type cannot be determined from the product ID.
    ///
    /// # Errors
    ///
    /// Returns [`JoyConError::InvalidDevice`] if:
    /// - The product ID is not recognized as a JoyCon or Pro Controller
    /// - The charging grip interface number is invalid
    pub fn new(device_info: &DeviceInfo) -> Result<Self, JoyConError> {
        let device_type = match device_info.product_id {
            JOYCON_L_BT => JoyConType::Left,
            JOYCON_R_BT => JoyConType::Right,
            PRO_CONTROLLER => JoyConType::ProController,
            JOYCON_CHARGING_GRIP => match device_info.interface_number {
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

    /// Sends a rumble command to the JoyCon with the specified frequency and amplitude.
    ///
    /// This is the primary method for producing sound through the JoyCon's rumble motor.
    /// The frequency is automatically octave-shifted to fit within the hardware's
    /// supported range (approximately 40 Hz to 1252 Hz).
    ///
    /// # Arguments
    ///
    /// * `frequency` - The desired frequency in Hz. Use `0.0` to stop rumbling.
    ///   Values outside the 40-1252 Hz range will be octave-shifted.
    /// * `amplitude` - The intensity from `0.0` (silent) to `1.0` (maximum).
    ///   Values outside this range are clamped.
    ///
    /// # Musical Reference
    ///
    /// Common musical frequencies:
    /// - A4 (concert pitch): 440.0 Hz
    /// - Middle C (C4): 261.63 Hz
    /// - C5: 523.25 Hz
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use musical_joycons::joycon::JoyConManager;
    /// # let manager = JoyConManager::new()?;
    /// # let mut joycons = manager.connect_and_initialize_joycons()?;
    /// # let joycon = joycons.first_mut().unwrap();
    /// // Play A4 at half volume
    /// joycon.rumble(440.0, 0.5)?;
    ///
    /// // Stop rumbling
    /// joycon.rumble(0.0, 0.0)?;
    /// # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`JoyConError::NotConnected`] if the device handle is not set.
    /// Returns [`JoyConError::HidError`] if the HID write operation fails.
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

    /// Enables the rumble motor on this JoyCon.
    ///
    /// This must be called before any rumble commands will have effect.
    /// It is automatically called by [`initialize_device`](Self::initialize_device).
    ///
    /// # Errors
    ///
    /// Returns an error if the HID command cannot be sent.
    pub fn enable_rumble(&mut self) -> Result<(), JoyConError> {
        JoyconInterface::send_command(
            self,
            Command::Rumble,
            Some(Subcommand::EnableVibration),
            &[0x01],
        )
    }

    /// Plays a C major scale from C5 to C6 as a device test.
    ///
    /// This is useful for verifying that the JoyCon rumble motor is working
    /// correctly after connection. Each note plays for 50ms with a 5ms pause.
    ///
    /// # Notes Played
    ///
    /// C5 (524 Hz) → D5 → E5 → F5 → G5 → A5 → B5 → C6 (1046 Hz)
    ///
    /// # Errors
    ///
    /// Returns an error if any rumble command fails.
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

    /// Plays a complete rumble track on this JoyCon.
    ///
    /// This method executes all commands in the track sequentially,
    /// respecting the timing specified in each command's `wait_before` field.
    ///
    /// # Arguments
    ///
    /// * `track` - The [`RumbleTrack`] containing the sequence of rumble commands
    ///
    /// # Blocking
    ///
    /// This method blocks the current thread for the duration of the track.
    /// For non-blocking playback, run in a separate thread.
    ///
    /// # Errors
    ///
    /// Returns an error if any rumble command fails to send.
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

    /// Plays a rumble track with synchronized start across multiple JoyCons.
    ///
    /// This method waits for the `start_signal` to become `true` before
    /// beginning playback, allowing multiple JoyCons to start playing
    /// at the same moment.
    ///
    /// # Arguments
    ///
    /// * `track` - The [`RumbleTrack`] to play
    /// * `start_signal` - A shared mutex that signals when to start playback
    ///
    /// # Synchronization
    ///
    /// The method polls the signal every 1ms until it becomes `true`,
    /// then immediately begins playback.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::{Arc, Mutex};
    /// use std::thread;
    /// # use musical_joycons::joycon::JoyConManager;
    /// # use musical_joycons::midi::RumbleTrack;
    ///
    /// # fn example(track1: RumbleTrack, track2: RumbleTrack) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// let manager = JoyConManager::new()?;
    /// let mut joycons = manager.connect_and_initialize_joycons()?;
    ///
    /// let signal = Arc::new(Mutex::new(false));
    ///
    /// // Spawn threads for each JoyCon (simplified example)
    /// // ... then trigger synchronized start:
    /// *signal.lock().unwrap_or_else(|e| e.into_inner()) = true;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if playback fails.
    pub fn play_synchronized(
        &mut self,
        track: RumbleTrack,
        start_signal: Arc<Mutex<bool>>,
    ) -> Result<(), JoyConError> {
        while !*start_signal.lock().unwrap_or_else(|e| e.into_inner()) {
            thread::sleep(Duration::from_millis(1));
        }

        self.play_rumble_track(track)
    }

    /// Initializes the JoyCon for rumble playback.
    ///
    /// This method:
    /// 1. Enables the rumble motor via HID command
    /// 2. Plays a test scale to verify functionality
    ///
    /// Call this after connecting to a JoyCon and before sending rumble commands.
    /// [`JoyConManager::connect_and_initialize_joycons`](super::JoyConManager::connect_and_initialize_joycons)
    /// calls this automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if enabling rumble or playing the scale fails.
    pub fn initialize_device(&mut self) -> Result<(), JoyConError> {
        self.enable_rumble()?;
        self.play_scale()
    }

    /// Returns a reference to the underlying HID device handle.
    ///
    /// Returns `None` if the device has not been connected yet.
    pub fn get_handle(&self) -> Option<&HidDevice> {
        self.handle.as_ref()
    }

    /// Sets the HID device handle for this JoyCon.
    ///
    /// This is called internally by [`JoyConManager`](super::JoyConManager)
    /// after successfully opening the device.
    pub(crate) fn set_handle(&mut self, device: HidDevice) {
        self.handle = Some(device);
    }

    /// Returns the type of this JoyCon device.
    ///
    /// # Returns
    ///
    /// - [`JoyConType::Left`] for a left JoyCon
    /// - [`JoyConType::Right`] for a right JoyCon
    /// - [`JoyConType::ProController`] for a Pro Controller
    /// - [`JoyConType::Other`] for unrecognized devices
    pub fn get_type(&self) -> JoyConType {
        self.device_type
    }

    /// Returns the current timing byte value.
    ///
    /// The timing byte is a packet counter used in HID communication
    /// to ensure proper command sequencing.
    pub fn get_timing_byte(&self) -> u8 {
        self.timing_byte
    }

    /// Increments the timing byte, wrapping at 255.
    ///
    /// Called internally after each HID command is sent.
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
            vendor_id: 0x057E,
            product_id,
            usage_page: 0,
            interface_number: 0,
            serial: String::new(),
        }
    }

    #[test]
    fn test_joycon_creation() {
        let left_info = create_test_device_info(JOYCON_L_BT);
        let right_info = create_test_device_info(JOYCON_R_BT);
        let pro_info = create_test_device_info(PRO_CONTROLLER);

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
        let mut joycon = JoyCon::new(&create_test_device_info(JOYCON_L_BT)).unwrap();

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
        let mut joycon = JoyCon::new(&create_test_device_info(JOYCON_L_BT)).unwrap();

        // Should fail without device handle
        assert!(joycon.rumble(440.0, 1.5).is_err());
        assert!(joycon.rumble(2504.0, 0.5).is_err());
        assert!(joycon.rumble(0.25, 0.5).is_err());
    }
}
