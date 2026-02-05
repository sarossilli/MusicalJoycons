//! JoyCon device management and communication.
//!
//! This module provides:
//! - Device discovery and connection management via [`JoyConManager`]
//! - Individual JoyCon control via [`JoyCon`]
//! - Device type identification and error handling

mod device;
mod interface;
mod manager;
mod types;

// Re-export public types
pub use self::device::JoyCon;
pub use self::manager::JoyConManager;
pub use self::types::{DeviceInfo, JoyConError, JoyConType};