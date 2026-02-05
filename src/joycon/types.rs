//! JoyCon-specific types and constants.

/// Nintendo vendor ID
pub(crate) const VENDOR_ID: u16 = 0x057e;

/// Product IDs for various Nintendo controllers
pub(crate) const JOYCON_L_BT: u16 = 0x2006;
pub(crate) const JOYCON_R_BT: u16 = 0x2007;
pub(crate) const PRO_CONTROLLER: u16 = 0x2009;
pub(crate) const JOYCON_CHARGING_GRIP: u16 = 0x200e;

/// HID commands for JoyCon communication.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(crate) enum Command {
    Rumble = 0x10,
    SubCommand = 0x01,
}

/// HID subcommands for JoyCon configuration.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(crate) enum Subcommand {
    EnableVibration = 0x48,
    EnableImu = 0x40,
    SetInputMode = 0x03,
    GetDeviceInfo = 0x02,
}

/// Identifies the type of JoyCon device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoyConType {
    /// Left JoyCon
    Left,
    /// Right JoyCon
    Right,
    /// Nintendo Pro Controller
    ProController,
    /// Unknown or unsupported device with the given product ID
    Other(u16),
}

/// Information about a connected HID device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Product ID of the device
    pub product_id: u16,
    /// Interface number (used for charging grip detection)
    pub interface_number: i32,
    /// Serial number of the device
    pub serial: String,
    /// HID device path (reserved for future use)
    #[allow(dead_code)]
    pub(crate) path: String,
    /// Vendor ID (reserved for future use)
    #[allow(dead_code)]
    pub(crate) vendor_id: i32,
    /// USB usage page (reserved for future use)
    #[allow(dead_code)]
    pub(crate) usage_page: i32,
}

/// Errors that can occur during JoyCon operations.
#[derive(Debug, thiserror::Error)]
pub enum JoyConError {
    /// Device is not connected or has been disconnected.
    #[error("Device not connected")]
    NotConnected,

    /// Device configuration is invalid or unsupported.
    #[error("Invalid device configuration: {0}")]
    InvalidDevice(&'static str),

    /// An error occurred during HID communication.
    #[error("HID error: {0}")]
    HidError(String),

    /// Invalid rumble frequency or amplitude parameters.
    #[error("Invalid rumble parameters: {0}")]
    InvalidRumble(&'static str),
}
