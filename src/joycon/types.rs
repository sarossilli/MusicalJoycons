use hidapi::HidDevice;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Rumble = 0x10,
    SubCommand = 0x01,
}

#[derive(Debug, Clone, Copy)]
pub enum Subcommand {
    EnableVibration = 0x48,
    EnableImu = 0x40,
    SetInputMode = 0x03,
    GetDeviceInfo = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoyConType {
    Left,
    Right,
    ProController,
}

pub struct DeviceInfo {
    pub product_id: u16,
    pub interface_number: i32,
    pub serial: String,
}

#[derive(Debug, Clone)]
pub struct RumbleState {
    pub high_freq: u16,
    pub high_freq_amp: u8,
    pub low_freq: u8,
    pub low_freq_amp: u16,
    pub last_update: Instant,
}

impl Default for RumbleState {
    fn default() -> Self {
        Self {
            high_freq: 0,
            high_freq_amp: 0,
            low_freq: 0,
            low_freq_amp: 0,
            last_update: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct JoyCon {
    pub handle: Option<HidDevice>,
    pub device_type: JoyConType,
    pub timing_byte: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum JoyConError {
    #[error("Device not connected")]
    NotConnected,

    #[error("Invalid device configuration: {0}")]
    InvalidDevice(&'static str),

    #[error("HID error: {0}")]
    HidError(String),

    #[error("Invalid rumble parameters: {0}")]
    InvalidRumble(&'static str),
}

pub const VENDOR_ID: u16 = 0x057e;
pub const JOYCON_L_BT: u16 = 0x2006;
pub const JOYCON_R_BT: u16 = 0x2007;
pub const PRO_CONTROLLER: u16 = 0x2009;
pub const JOYCON_CHARGING_GRIP: u16 = 0x200e;