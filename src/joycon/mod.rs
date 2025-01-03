
pub mod joycon;
pub mod types;
pub mod bluetooth;

pub use types::{Command, DeviceInfo, JoyCon, JoyConError, JoyConType, RumbleState, Subcommand};

pub use bluetooth::*;

pub const VENDOR_ID: u16 = 0x057E;
pub const JOYCON_L_BT: u16 = 0x2006;
pub const JOYCON_R_BT: u16 = 0x2007;
pub const PRO_CONTROLLER: u16 = 0x2009;
pub const JOYCON_CHARGING_GRIP: u16 = 0x200e;
