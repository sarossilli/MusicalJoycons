//! JoyCon device management and communication.
//!
//! This module provides everything needed to discover, connect to, and control
//! Nintendo JoyCon controllers and Pro Controllers.
//!
//! # Overview
//!
//! The JoyCon module is organized around two main types:
//!
//! - [`JoyConManager`]: Handles device discovery and connection management
//! - [`JoyCon`]: Represents an individual connected device for rumble control
//!
//! # Device Discovery
//!
//! Use [`JoyConManager`] to find and connect to JoyCons:
//!
//! ```no_run
//! use musical_joycons::joycon::JoyConManager;
//!
//! let manager = JoyConManager::new()?;
//! let joycons = manager.connect_and_initialize_joycons()?;
//! println!("Found {} JoyCon(s)", joycons.len());
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! # Rumble Control
//!
//! Each [`JoyCon`] can be controlled independently:
//!
//! ```no_run
//! use musical_joycons::joycon::JoyConManager;
//! use std::time::Duration;
//! use std::thread;
//!
//! let manager = JoyConManager::new()?;
//! let mut joycons = manager.connect_and_initialize_joycons()?;
//!
//! if let Some(joycon) = joycons.first_mut() {
//!     // Play a 440 Hz tone at 50% amplitude
//!     joycon.rumble(440.0, 0.5)?;
//!     thread::sleep(Duration::from_millis(200));
//!     
//!     // Stop the rumble
//!     joycon.rumble(0.0, 0.0)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! # Supported Devices
//!
//! This module supports:
//! - Nintendo Switch Left JoyCon (Bluetooth)
//! - Nintendo Switch Right JoyCon (Bluetooth)
//! - Nintendo Switch Pro Controller
//! - JoyCons in Charging Grip (USB)
//!
//! # Error Handling
//!
//! All operations return [`Result`] types with [`JoyConError`] for error cases:
//!
//! - [`JoyConError::NotConnected`]: Device not found or disconnected
//! - [`JoyConError::InvalidDevice`]: Unsupported device configuration
//! - [`JoyConError::HidError`]: Low-level HID communication failure
//! - [`JoyConError::InvalidRumble`]: Invalid frequency/amplitude parameters

mod device;
mod interface;
mod manager;
mod types;

// Re-export public types
pub use self::device::JoyCon;
pub use self::manager::JoyConManager;
pub use self::types::{DeviceInfo, JoyConError, JoyConType};