use super::joycon::JoyCon;
use super::types::{DeviceInfo, JoyConError};
use hidapi::HidApi;

pub struct JoyConManager {
    api: HidApi,
}

impl JoyConManager {
    pub fn new() -> Result<Self, JoyConError> {
        let api = HidApi::new().map_err(|e| JoyConError::HidError(e.to_string()))?;
        Ok(Self { api })
    }

    pub fn scan_for_devices(&self) -> Result<Vec<JoyCon>, JoyConError> {
        let mut joycons = Vec::new();

        for device_info in self.api.device_list() {
            if device_info.vendor_id() == crate::joycon::types::VENDOR_ID {
                match device_info.product_id() {
                    id @ (crate::joycon::types::JOYCON_L_BT
                    | crate::joycon::types::JOYCON_R_BT
                    | crate::joycon::types::PRO_CONTROLLER) => {
                        println!(
                            "Found device: VID={:04x} PID={:04x}",
                            device_info.vendor_id(),
                            device_info.product_id()
                        );

                        if let Ok(device) = self.api.open_path(device_info.path()) {
                            let device_info = DeviceInfo {
                                product_id: id,
                                interface_number: device_info.interface_number(),
                                serial: device_info
                                    .serial_number()
                                    .unwrap_or("Unknown")
                                    .to_string(),
                            };

                            if let Ok(mut joycon) = JoyCon::new(&device_info) {
                                joycon.set_handle(device);
                                joycons.push(joycon);
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }

        Ok(joycons)
    }
}
