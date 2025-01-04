use hidapi::HidError;
use mockall::mock;
use mockall::predicate::*;
use musical_joycons::joycon::types::{DeviceInfo, JOYCON_L_BT, JOYCON_R_BT};
use musical_joycons::joycon::{JoyCon, JoyConError, JoyConManager, JoyConType};

// First, define a trait that represents the behavior we want to mock
pub trait HidDeviceTraitMock: Send {
    fn write(&self, data: &[u8]) -> Result<usize, HidError>;
    fn read(&mut self, data: &mut [u8]) -> Result<usize, HidError>;
}

// Create a mock for our trait
mock! {
    pub HidDeviceMock {}

    impl HidDeviceTraitMock for HidDeviceMock {
        fn write(&self, data: &[u8]) -> Result<usize, HidError>;
        fn read(&mut self, data: &mut [u8]) -> Result<usize, HidError>;
    }
}

#[test]
fn test_joycon_new() {
    let device_info = DeviceInfo {
        product_id: JOYCON_L_BT,
        interface_number: 0,
        serial: "TEST001".to_string(),
    };

    let joycon = JoyCon::new(&device_info).unwrap();
    assert_eq!(joycon.get_type(), JoyConType::Left);

    let device_info = DeviceInfo {
        product_id: JOYCON_R_BT,
        interface_number: 0,
        serial: "TEST002".to_string(),
    };

    let joycon = JoyCon::new(&device_info).unwrap();
    assert_eq!(joycon.get_type(), JoyConType::Right);
}

#[test]
fn test_invalid_device() {
    let device_info = DeviceInfo {
        product_id: 0x0000, // Invalid product ID
        interface_number: 0,
        serial: "TEST003".to_string(),
    };

    let result = JoyCon::new(&device_info);
    assert!(matches!(result, Err(JoyConError::InvalidDevice(_))));
}

#[tokio::test]
async fn test_rumble_parameters() {
    let device_info = DeviceInfo {
        product_id: JOYCON_L_BT,
        interface_number: 0,
        serial: "TEST005".to_string(),
    };

    let mut joycon = JoyCon::new(&device_info).unwrap();

    // Test invalid frequency
    let result = joycon.rumble(1300.0, 0.5);
    assert!(matches!(result, Err(JoyConError::InvalidRumble(_))));

    // Test invalid amplitude
    let result = joycon.rumble(440.0, 1.5);
    assert!(matches!(result, Err(JoyConError::InvalidRumble(_))));
}


// Helper function to create a mock HID device for testing
fn setup_mock_device() -> MockHidDeviceMock {
    let mut mock_device = MockHidDeviceMock::new();
    mock_device.expect_write().returning(|data| Ok(data.len()));
    mock_device
}
