#[cfg(test)]
mod tests {
    use crate::joycon::{
        types::{DeviceInfo, JOYCON_L_BT},
        JoyCon,
    };

    #[test]
    fn test_timing_byte_increment() {
        let device_info = DeviceInfo {
            product_id: JOYCON_L_BT,
            interface_number: 0,
            serial: String::from("TEST"),
            path: String::new(),
            vendor_id: 0x057E,
            usage_page: 0,
        };

        let mut joycon = JoyCon::new(&device_info).unwrap();
        assert_eq!(joycon.get_timing_byte(), 0);
        joycon.increment_timing_byte();
        assert_eq!(joycon.get_timing_byte(), 1);
    }
}
