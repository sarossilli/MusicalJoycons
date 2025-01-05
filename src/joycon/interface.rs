use super::types::{Command, JoyConError, JoyConType, Subcommand};
use super::JoyCon;
pub(crate) struct JoyconInterface;

impl JoyconInterface {
    pub fn send_command(
        joycon: &mut JoyCon,
        command: Command,
        subcommand: Option<Subcommand>,
        data: &[u8],
    ) -> Result<(), JoyConError> {
        let mut buf = [0u8; 49];
        buf[0] = command as u8;
        buf[1] = joycon.get_timing_byte();

        // Standard rumble data for neutral state
        let rumble_base = [0x00, 0x01, 0x40, 0x40, 0x00, 0x01, 0x40, 0x40];
        buf[2..10].copy_from_slice(&rumble_base);

        if let Some(subcmd) = subcommand {
            buf[10] = subcmd as u8;
            if !data.is_empty() {
                let end = (11 + data.len()).min(49);
                buf[11..end].copy_from_slice(&data[..end - 11]);
            }
        }

        Self::write_to_joycon(joycon, &buf)?;
        joycon.increment_timing_byte();
        Ok(())
    }

    pub fn send_rumble(
        joycon: &mut JoyCon,
        frequency: f32,
        amplitude: f32,
    ) -> Result<(), JoyConError> {
        let mut buf = [0u8; 49];
        buf[0] = Command::Rumble as u8;
        buf[1] = joycon.get_timing_byte();

        let frequency = frequency.clamp(0.0, 1252.0);
        let encoded_freq = if frequency > 0.0 {
            ((frequency / 10.0).log2() * 32.0).round() as u8
        } else {
            0
        };

        // Convert to HF/LF ranges according to formulas, using u16 for HF calculations
        let hf: u16 = if encoded_freq > 0x60 {
            let diff = u16::from(encoded_freq - 0x60);
            diff.saturating_mul(4)
        } else {
            0
        };
        let lf = encoded_freq.saturating_sub(0x40);

        // Then encode the amplitude according to the ranges
        let amplitude = amplitude.clamp(0.0, 1.0);
        let encoded_amp = if amplitude > 0.23 {
            ((amplitude * 8.7).log2() * 32.0).round() as u8
        } else if amplitude > 0.12 {
            ((amplitude * 17.0).log2() * 16.0).round() as u8
        } else {
            // For very low amplitudes, use the same formula as medium range
            ((amplitude * 17.0).log2() * 16.0).round() as u8
        };

        let hf_amp = encoded_amp.saturating_mul(2);
        let lf_amp = encoded_amp.saturating_div(2).saturating_add(0x40);

        // Pack data according to the correct format for each Joycon type
        let offset = match joycon.get_type() {
            JoyConType::Right => 6, // Right JoyCon data starts at byte 6
            _ => 2,                 // Left JoyCon/Pro Controller data starts at byte 2
        };

        buf[offset] = (hf & 0xFF) as u8; // Low byte of HF
        buf[offset + 1] = hf_amp; // High frequency amplitude
        buf[offset + 2] = lf; // Low frequency
        buf[offset + 3] = lf_amp; // Low frequency amplitude

        // Fill in the other side with neutral data
        let other_offset = if offset == 2 { 6 } else { 2 };
        buf[other_offset..other_offset + 4].copy_from_slice(&[0x00, 0x01, 0x40, 0x40]);
        Self::write_to_joycon(joycon, &buf)?;
        joycon.increment_timing_byte();
        Ok(())
    }

    fn write_to_joycon(joycon: &JoyCon, buf: &[u8]) -> Result<(), JoyConError> {
        let handle = joycon.get_handle().ok_or(JoyConError::NotConnected)?;

        match handle.write(buf) {
            Ok(written) => {
                if written != buf.len() {
                    return Err(JoyConError::HidError(format!(
                        "Incomplete write: {} of {} bytes",
                        written,
                        buf.len()
                    )));
                }
                Ok(())
            }
            Err(e) => Err(JoyConError::HidError(e.to_string())),
        }
    }
}
