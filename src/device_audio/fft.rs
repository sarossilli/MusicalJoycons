use rustfft::{FftPlanner, num_complex::Complex};
use std::collections::VecDeque;

const VOCAL_MIN_FREQ: f32 = 80.0;  // Typical lowest vocal note
const VOCAL_MAX_FREQ: f32 = 1200.0;  // Typical highest vocal/melody note
const PEAK_THRESHOLD: f32 = 0.015;  // Adjusted for better peak isolation
const MIN_AMPLITUDE: f32 = 0.01;

// Add new public frequency range constants
pub const BASS_MIN_FREQ: f32 = 40.0;   // Low bass
pub const BASS_MAX_FREQ: f32 = 250.0;  // Upper bass
pub const MELODY_MIN_FREQ: f32 = 250.0; // Lower melody range
pub const MELODY_MAX_FREQ: f32 = 1500.0; // Upper melody range

// Add musical note constants
const NOTE_FREQUENCIES: &[(f32, &str)] = &[
    (82.41, "E2"),    // Bass E
    (110.00, "A2"),
    (146.83, "D3"),
    (196.00, "G3"),
    (246.94, "B3"),
    (329.63, "E4"),   // Guitar/Vocal range
    (440.00, "A4"),
    (493.88, "B4"),
    (587.33, "D5"),
    (783.99, "G5"),
    (987.77, "B5"),
    (1174.66, "D6"),
];

pub struct FrequencyAnalyzer {
    sample_rate: u32,
    window_size: usize,
    planner: FftPlanner<f32>,
    prev_magnitudes: VecDeque<Vec<f32>>,
    noise_floor: Vec<f32>,
    frame_count: usize,
    peak_history: VecDeque<Vec<FrequencyPeak>>,
    last_stable_peak: Option<FrequencyPeak>,
}

#[derive(Debug, Clone)]
pub struct FrequencyPeak {
    pub frequency: f32,
    pub magnitude: f32,
}

impl FrequencyAnalyzer {
    pub fn new(sample_rate: u32, window_size: usize) -> Self {
        Self {
            sample_rate,
            window_size,
            planner: FftPlanner::new(),
            prev_magnitudes: VecDeque::with_capacity(5), // Keep last 5 frames for smoothing
            noise_floor: vec![0.0; window_size / 2],
            frame_count: 0,
            peak_history: VecDeque::with_capacity(10), // Keep track of peaks over time
            last_stable_peak: None,
        }
    }

    pub fn get_window_size(&self) -> usize {
        self.window_size
    }

    pub fn resize_window(&mut self, new_size: usize) {
        self.window_size = new_size;
    }

    pub fn analyze_frequencies(&mut self, samples: &[f32], num_peaks: usize) -> Vec<FrequencyPeak> {
        let max_amplitude = samples.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if max_amplitude < MIN_AMPLITUDE {
            return Vec::new();
        }

        // Convert stereo to mono and normalize
        let mono_samples: Vec<f32> = if samples.len() % 2 == 0 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk[1]) * 0.5)
                .collect()
        } else {
            samples.to_vec()
        };

        // Resize window if needed and pad/truncate samples to match
        let padded_samples = if mono_samples.len() != self.window_size {
            self.window_size = mono_samples.len();
            self.noise_floor = vec![0.0; self.window_size / 2];
            mono_samples
        } else {
            mono_samples
        };

        let windowed = self.apply_hanning_window(&padded_samples);
        let mut fft_input: Vec<Complex<f32>> = windowed.iter()
            .map(|&x| Complex::new(x, 0.0))
            .collect();

        let fft = self.planner.plan_fft_forward(self.window_size);
        fft.process(&mut fft_input);

        // Calculate magnitudes with better frequency resolution
        let mut peaks: Vec<FrequencyPeak> = fft_input
            .iter()
            .take(self.window_size / 2)
            .enumerate()
            .map(|(i, c)| {
                let freq = i as f32 * self.sample_rate as f32 / self.window_size as f32;
                let mag = c.norm() / (self.window_size as f32).sqrt();
                FrequencyPeak {
                    frequency: freq,
                    magnitude: mag,
                }
            })
            .filter(|peak| {
                // Filter for musical frequency range
                peak.frequency >= VOCAL_MIN_FREQ 
                && peak.frequency <= VOCAL_MAX_FREQ
                && peak.magnitude > PEAK_THRESHOLD
            })
            .collect();

        // Sort by magnitude
        peaks.sort_by(|a, b| b.magnitude.partial_cmp(&a.magnitude).unwrap());

        // Remove harmonics to isolate fundamental frequencies
        let mut filtered_peaks = Vec::new();
        for peak in peaks {
            if !filtered_peaks.iter().any(|existing: &FrequencyPeak| {
                self.is_harmonic_of(existing.frequency, peak.frequency)
            }) {
                filtered_peaks.push(peak);
            }
        }

        filtered_peaks.truncate(num_peaks);
        filtered_peaks
    }

    pub fn analyze_frequency_band(&mut self, samples: &[f32], min_freq: f32, max_freq: f32) -> Vec<FrequencyPeak> {
        let max_amplitude = samples.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if max_amplitude < MIN_AMPLITUDE {
            return Vec::new();
        }

        // Convert stereo to mono and normalize
        let mono_samples: Vec<f32> = if samples.len() % 2 == 0 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk[1]) * 0.5)
                .collect()
        } else {
            samples.to_vec()
        };

        // Resize window if needed
        if mono_samples.len() != self.window_size {
            self.window_size = mono_samples.len();
            self.noise_floor = vec![0.0; self.window_size / 2];
        }

        let windowed = self.apply_hanning_window(&mono_samples);
        let mut fft_input: Vec<Complex<f32>> = windowed.iter()
            .map(|&x| Complex::new(x, 0.0))
            .collect();

        let fft = self.planner.plan_fft_forward(self.window_size);
        fft.process(&mut fft_input);

        // Get peaks in specific frequency band
        let mut peaks: Vec<FrequencyPeak> = fft_input
            .iter()
            .take(self.window_size / 2)
            .enumerate()
            .map(|(i, c)| {
                let freq = i as f32 * self.sample_rate as f32 / self.window_size as f32;
                let mag = c.norm() / (self.window_size as f32).sqrt();
                FrequencyPeak {
                    frequency: freq,
                    magnitude: mag,
                }
            })
            .filter(|peak| {
                peak.frequency >= min_freq 
                && peak.frequency <= max_freq
                && peak.magnitude > PEAK_THRESHOLD
            })
            .collect();

        peaks.sort_by(|a, b| b.magnitude.partial_cmp(&a.magnitude).unwrap());
        peaks.truncate(3); // Keep top 3 peaks in this band

        // Snap frequencies to musical notes before returning
        peaks.iter_mut().for_each(|peak| {
            peak.frequency = self.snap_to_musical_note(peak.frequency);
        });

        peaks
    }

    fn update_noise_floor(&mut self, magnitudes: &[f32]) {
        if self.frame_count == 0 {
            self.noise_floor = magnitudes.to_vec();
        } else {
            for (floor, &current) in self.noise_floor.iter_mut().zip(magnitudes.iter()) {
                *floor = (*floor * 0.9 + current * 0.1).min(current);
            }
        }
    }

    fn apply_hanning_window(&self, samples: &[f32]) -> Vec<f32> {
        samples
            .iter()
            .enumerate()
            .map(|(i, &sample)| {
                let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 
                    / (self.window_size - 1) as f32).cos());
                sample * window
            })
            .collect()
    }

    fn is_harmonic_of(&self, f1: f32, f2: f32) -> bool {
        const HARMONIC_TOLERANCE: f32 = 0.05; // 5% tolerance
        let ratio = f1.max(f2) / f1.min(f2);
        let nearest_harmonic = ratio.round();
        (ratio - nearest_harmonic).abs() < HARMONIC_TOLERANCE && nearest_harmonic <= 4.0
    }

    fn find_stable_peaks(&self, current_peaks: &[FrequencyPeak]) -> Vec<FrequencyPeak> {
        let mut stable_peaks = Vec::new();
        
        for peak in current_peaks {
            // Check if this peak appears consistently in history
            let is_stable = self.peak_history.iter().all(|historical_peaks| {
                historical_peaks.iter().any(|hist_peak| {
                    // Check if the frequency is consistent within 3% and has significant magnitude
                    let freq_diff = (hist_peak.frequency - peak.frequency).abs();
                    freq_diff < peak.frequency * 0.03 && hist_peak.magnitude > PEAK_THRESHOLD
                })
            });

            if is_stable {
                stable_peaks.push(peak.clone());
            }
        }

        stable_peaks
    }

    pub fn get_dominant_frequency(&mut self, samples: &[f32]) -> Option<FrequencyPeak> {
        let peaks = self.analyze_frequencies(samples, 1);
        peaks.into_iter().next()
    }

    pub fn snap_to_musical_note(&self, frequency: f32) -> f32 {
        if frequency == 0.0 {
            return 0.0;
        }

        // Find the closest musical note
        NOTE_FREQUENCIES
            .iter()
            .min_by(|&&(freq1, _), &&(freq2, _)| {
                let diff1 = (freq1 - frequency).abs();
                let diff2 = (freq2 - frequency).abs();
                diff1.partial_cmp(&diff2).unwrap()
            })
            .map(|&(freq, note)| {
                // Optional: print the note name when in debug mode
                #[cfg(debug_assertions)]
                println!("Snapped {:.1} Hz to {}: {:.1} Hz", frequency, note, freq);
                freq
            })
            .unwrap_or(frequency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frequency_detection() {
        let sample_rate = 44100;
        let window_size = 1024;
        let mut analyzer = FrequencyAnalyzer::new(sample_rate, window_size);

        // Generate a test signal at 440Hz (A4 note)
        let test_frequency = 440.0;
        let samples: Vec<f32> = (0..window_size)
            .map(|i| {
                (2.0 * std::f32::consts::PI * test_frequency * i as f32 / sample_rate as f32).sin()
            })
            .collect();

        if let Some(peak) = analyzer.get_dominant_frequency(&samples) {
            // Allow for some FFT bin size error
            assert!((peak.frequency - test_frequency).abs() < 5.0,
                "Expected frequency around 440Hz, got {}Hz", peak.frequency);
        } else {
            panic!("No frequency detected");
        }
    }

    #[test]
    fn test_multiple_peaks() {
        let sample_rate = 44100;
        let window_size = 1024;
        let mut analyzer = FrequencyAnalyzer::new(sample_rate, window_size);

        // Generate a test signal with two frequencies: 440Hz and 880Hz
        let samples: Vec<f32> = (0..window_size)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() +
                0.3 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
            })
            .collect();

        let peaks = analyzer.analyze_frequencies(&samples, 2);
        assert_eq!(peaks.len(), 2, "Should detect two frequency peaks");
    }
}
