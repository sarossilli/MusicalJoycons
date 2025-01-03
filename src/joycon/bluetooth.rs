use super::types::{Command, DeviceInfo, JoyCon, JoyConError, JoyConType, Subcommand};
use hidapi::HidApi;

pub fn scan_for_devices() -> Result<Vec<JoyCon>, JoyConError> {
    let api = HidApi::new().map_err(|e| JoyConError::HidError(e.to_string()))?;
    let mut joycons = Vec::new();

    for device_info in api.device_list() {
        if device_info.vendor_id() == super::types::VENDOR_ID {
            match device_info.product_id() {
                id @ (super::types::JOYCON_L_BT
                | super::types::JOYCON_R_BT
                | super::types::PRO_CONTROLLER) => {
                    println!(
                        "Found device: VID={:04x} PID={:04x}",
                        device_info.vendor_id(),
                        device_info.product_id()
                    );
                    println!(
                        "Usage: {:04x}:{:04x}",
                        device_info.usage_page(),
                        device_info.usage()
                    );

                    if let Ok(device) = api.open_path(device_info.path()) {
                        let device_info = DeviceInfo {
                            product_id: id,
                            interface_number: device_info.interface_number(),
                            serial: device_info.serial_number().unwrap_or("Unknown").to_string(),
                        };

                        if let Ok(mut joycon) = JoyCon::new(&device_info) {
                            joycon.handle = Some(device);
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

pub fn send_subcommand(
    joycon: &mut JoyCon,
    command: Command,
    subcommand: Subcommand,
    data: &[u8],
) -> Result<(), JoyConError> {
    let mut buf = [0u8; 49];
    buf[0] = command as u8;
    buf[1] = joycon.timing_byte;

    // Standard rumble data for neutral state
    let rumble_base = [0x00, 0x01, 0x40, 0x40, 0x00, 0x01, 0x40, 0x40];
    buf[2..10].copy_from_slice(&rumble_base);

    buf[10] = subcommand as u8;
    if !data.is_empty() {
        let end = (11 + data.len()).min(49);
        buf[11..end].copy_from_slice(&data[..end - 11]);
    }

    println!("Sending command packet (len={})", buf.len());
    for i in 0..buf.len() {
        print!("{:02x} ", buf[i]);
        if (i + 1) % 16 == 0 {
            println!();
        }
    }
    println!();

    let handle = joycon.handle.as_ref().ok_or(JoyConError::NotConnected)?;

    // Try a direct write first
    match handle.write(&buf) {
        Ok(written) => {
            println!("Write result: {}/{} bytes", written, buf.len());
            if written != buf.len() {
                return Err(JoyConError::HidError(format!(
                    "Incomplete write: {} of {} bytes",
                    written,
                    buf.len()
                )));
            }
        }
        Err(e) => {
            println!("Write error: {:?}", e);
            return Err(JoyConError::HidError(e.to_string()));
        }
    }

    joycon.timing_byte = joycon.timing_byte.wrapping_add(1);
    Ok(())
}

pub fn enable_rumble(joycon: &mut JoyCon) -> Result<(), JoyConError> {
    send_subcommand(
        joycon,
        Command::Rumble,
        Subcommand::EnableVibration,
        &[0x01],
    )
}

pub fn encode_and_send_rumble(
    joycon: &mut JoyCon,
    frequency: f32,
    amplitude: f32,
) -> Result<(), JoyConError> {
    let mut buf = [0u8; 49];
    buf[0] = Command::Rumble as u8;
    buf[1] = joycon.timing_byte;

    // First calculate frequency bytes
    let encoded_hex_freq = if frequency > 0.0 {
        ((frequency.clamp(0.0, 1252.0) / 10.0).log2() * 32.0).round() as u8
    } else {
        0
    };

    let hf = encoded_hex_freq.saturating_sub(0x60).saturating_mul(4);
    let lf = encoded_hex_freq.saturating_sub(0x40);

    // Then calculate amplitude bytes
    let encoded_hex_amp = if amplitude > 0.23 {
        ((amplitude.clamp(0.0, 1.0) * 8.7).log2() * 32.0).round() as u8
    } else if amplitude > 0.12 {
        ((amplitude.clamp(0.0, 1.0) * 17.0).log2() * 16.0).round() as u8
    } else {
        ((amplitude.clamp(0.0, 1.0) * 17.0).log2() * 16.0).round() as u8
    };

    // Convert to Joy-Con amplitude ranges
    let hf_amp = encoded_hex_amp.saturating_mul(2);
    let lf_amp = encoded_hex_amp.saturating_div(2).saturating_add(0x40);

    // Pack data according to the correct format
    let offset = match joycon.device_type {
        JoyConType::Right => 6, // Right JoyCon data starts at byte 6
        _ => 2,                 // Left JoyCon/Pro Controller data starts at byte 2
    };

    buf[offset] = hf as u8;
    buf[offset + 1] = hf_amp;
    buf[offset + 2] = lf;
    buf[offset + 3] = lf_amp;

    // Fill in the other side with neutral data
    let other_offset = if offset == 2 { 6 } else { 2 };
    buf[other_offset..other_offset + 4].copy_from_slice(&[0x00, 0x01, 0x40, 0x40]);

    println!("Sending rumble packet (len={})", buf.len());
    for i in 0..buf.len() {
        print!("{:02x} ", buf[i]);
        if (i + 1) % 16 == 0 {
            println!();
        }
    }
    println!();

    let handle = joycon.handle.as_ref().ok_or(JoyConError::NotConnected)?;
    match handle.write(&buf) {
        Ok(written) => {
            println!("Write result: {}/{} bytes", written, buf.len());
            if written != buf.len() {
                return Err(JoyConError::HidError(format!(
                    "Incomplete write: {} of {} bytes",
                    written,
                    buf.len()
                )));
            }
        }
        Err(e) => {
            println!("Write error: {:?}", e);
            return Err(JoyConError::HidError(e.to_string()));
        }
    }

    joycon.timing_byte = joycon.timing_byte.wrapping_add(1);
    Ok(())
}
