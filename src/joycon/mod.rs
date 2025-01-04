mod interface;
pub mod types;

pub use self::manager::JoyConManager;
pub use self::joycon::JoyCon;
pub use self::types::{JoyConError, JoyConType};

mod manager;
mod joycon;

#[cfg(test)]
mod tests;