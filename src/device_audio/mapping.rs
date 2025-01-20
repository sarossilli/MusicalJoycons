use super::fft::FrequencyPeak;
use std::time::{Duration, Instant};

pub struct RumbleMapping {
    last_update: Instant,
    smooth_frequency: f32,
    smooth_amplitude: f32,
}

impl RumbleMapping {
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            smooth_frequency: 0.0,
            smooth_amplitude: 0.0,
        }
    }

    pub fn map_frequencies(&mut self, peaks: &[FrequencyPeak]) -> (f32, f32) {
        const SMOOTHING_FACTOR: f32 = 0.15; // Smoother transitions
        const MIN_MAGNITUDE_THRESHOLD: f32 = 0.015;
        const AMPLITUDE_SCALING: f32 = 10.0;
        const MIN_RUMBLE_AMPLITUDE: f32 = 0.0; // Allow complete silence
        const MAX_RUMBLE_AMPLITUDE: f32 = 0.8; // Limit maximum intensity

        if peaks.is_empty() {
            self.smooth_amplitude *= (1.0 - SMOOTHING_FACTOR);
            return (self.smooth_frequency, self.smooth_amplitude);
        }

        // Find most prominent musical peak
        let strongest = peaks.iter()
            .max_by(|a, b| a.magnitude.partial_cmp(&b.magnitude).unwrap())
            .unwrap();

        // Dynamic smoothing based on magnitude
        let freq_smoothing = if strongest.magnitude > 0.1 {
            SMOOTHING_FACTOR * 1.5 // Faster response for strong signals
        } else {
            SMOOTHING_FACTOR
        };

        // Update frequency with smoothing
        self.smooth_frequency = self.smooth_frequency * (1.0 - freq_smoothing) 
            + strongest.frequency * freq_smoothing;

        // Calculate target amplitude with musical dynamics
        let target_amplitude = if strongest.magnitude > MIN_MAGNITUDE_THRESHOLD {
            ((strongest.magnitude * AMPLITUDE_SCALING).min(1.0) * MAX_RUMBLE_AMPLITUDE)
                .max(MIN_RUMBLE_AMPLITUDE)
        } else {
            MIN_RUMBLE_AMPLITUDE
        };

        // Smooth amplitude changes
        self.smooth_amplitude = self.smooth_amplitude * (1.0 - SMOOTHING_FACTOR) 
            + target_amplitude * SMOOTHING_FACTOR;

        // After getting the smooth_frequency, snap it to a musical note
        let analyzer = super::fft::FrequencyAnalyzer::new(48000, 1024);
        self.smooth_frequency = analyzer.snap_to_musical_note(self.smooth_frequency);

        (self.smooth_frequency, self.smooth_amplitude)
    }
}
