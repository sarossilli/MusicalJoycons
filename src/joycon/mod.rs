mod interface;
pub mod types;

pub use self::manager::JoyConManager;
pub use self::joycon::JoyCon;
pub use self::types::{JoyConError, JoyConType};

pub mod manager;
pub mod joycon;

#[cfg(test)]
mod tests;