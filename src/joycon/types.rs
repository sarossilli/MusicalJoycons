pub(crate) const VENDOR_ID: u16 = 0x057e;
pub(crate) const JOYCON_L_BT: u16 = 0x2006;
pub(crate) const JOYCON_R_BT: u16 = 0x2007;
pub(crate) const PRO_CONTROLLER: u16 = 0x2009;
pub(crate) const JOYCON_CHARGING_GRIP: u16 = 0x200e;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Command {
    Rumble = 0x10,
    SubCommand = 0x01,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Subcommand {
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

pub(crate) struct DeviceInfo {
    pub product_id: u16,
    pub interface_number: i32,
    pub serial: String,
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
