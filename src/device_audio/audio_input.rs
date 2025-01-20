use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use std::error::Error;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}, Mutex};
use std::time::Instant;
use super::fft::{FrequencyAnalyzer, BASS_MIN_FREQ, BASS_MAX_FREQ, MELODY_MIN_FREQ, MELODY_MAX_FREQ};
use super::mapping::RumbleMapping;
use crate::joycon::joycon::JoyCon;

pub struct AudioInput {
    stream: Option<Stream>,
    buffer_count: Arc<AtomicUsize>,
    start_time: Instant,
    analyzer: Arc<Mutex<FrequencyAnalyzer>>,
    rumble_mapping: Arc<Mutex<RumbleMapping>>,
    joycons: Arc<Mutex<Vec<JoyCon>>>,
}

impl AudioInput {
    // Create a new instance of AudioInput
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let host = cpal::default_host();
        let selected_device = host
            .default_output_device()
            .ok_or("No output device found")?;

        println!("Using device: {}", selected_device.name()?);

        let default_format = selected_device.default_output_config()?.config();
        println!("Sample rate: {}", default_format.sample_rate.0);
        println!("Channels: {}", default_format.channels);

        let stream_config = StreamConfig {
            channels: default_format.channels,
            sample_rate: default_format.sample_rate,
            buffer_size: cpal::BufferSize::Fixed(1024), // Force a stable buffer size
        };

        let buffer_count = Arc::new(AtomicUsize::new(0));
        let start_time = Instant::now();

        // Create frequency analyzer with initial window size
        let analyzer = Arc::new(Mutex::new(FrequencyAnalyzer::new(
            default_format.sample_rate.0,
            1024,
        )));

        let buffer_count_clone = Arc::clone(&buffer_count);
        let analyzer_clone: Arc<Mutex<FrequencyAnalyzer>> = Arc::clone(&analyzer);

        let stream = selected_device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let count = buffer_count_clone.fetch_add(1, Ordering::SeqCst);
                
                if let Ok(mut analyzer) = analyzer_clone.lock() {
                    // Initialize or update analyzer if buffer size changes
                    if analyzer.get_window_size() != data.len() {
                        analyzer.resize_window(data.len());
                    }

                    // Get top 3 frequencies
                    let peaks = analyzer.analyze_frequencies(data, 3);
                    if count % 10 == 0 {
                        println!("\n🎵 Buffer #{} ({} samples):", count, data.len());
                        for (i, peak) in peaks.iter().enumerate() {
                            println!("  #{}: {:.1} Hz (magnitude: {:.3})", 
                                i + 1, 
                                peak.frequency, 
                                peak.magnitude
                            );
                        }
                        // Add a visual representation of the magnitudes
                        println!("Relative magnitudes:");
                        for peak in &peaks {
                            let bars = (peak.magnitude * 40.0) as usize;
                            println!("  {:.1} Hz: {}",
                                peak.frequency,
                                "█".repeat(bars.min(40))
                            );
                        }
                    }
                }
            },
            move |err| eprintln!("Error in stream: {}", err),
            None,
        )?;

        // Return the struct containing the stream
        Ok(AudioInput {
            stream: Some(stream),
            buffer_count,
            start_time,
            analyzer,
            rumble_mapping: Arc::new(Mutex::new(RumbleMapping::new())),
            joycons: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn new_with_joycons(joycons: Vec<JoyCon>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let host = cpal::default_host();
        let selected_device = host
            .default_output_device()
            .ok_or("No output device found")?;

        println!("Using device: {}", selected_device.name()?);

        let default_format = selected_device.default_output_config()?.config();
        println!("Sample rate: {}", default_format.sample_rate.0);
        println!("Channels: {}", default_format.channels);

        let stream_config = StreamConfig {
            channels: default_format.channels,
            sample_rate: default_format.sample_rate,
            buffer_size: cpal::BufferSize::Fixed(1024), // Force a stable buffer size
        };

        let buffer_count = Arc::new(AtomicUsize::new(0));
        let start_time = Instant::now();

        // Create frequency analyzer with initial window size
        let window_size = 960; // Match the actual buffer size we're seeing
        let analyzer = Arc::new(Mutex::new(FrequencyAnalyzer::new(
            default_format.sample_rate.0,
            window_size,
        )));

        let buffer_count_clone = Arc::clone(&buffer_count);
        let analyzer_clone: Arc<Mutex<FrequencyAnalyzer>> = Arc::clone(&analyzer);

        let rumble_mapping = Arc::new(Mutex::new(RumbleMapping::new()));
        let joycons = Arc::new(Mutex::new(joycons));
        
        let rumble_mapping_clone = Arc::clone(&rumble_mapping);
        let joycons_clone: Arc<Mutex<Vec<JoyCon>>> = Arc::clone(&joycons);

        let stream = selected_device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let count = buffer_count_clone.fetch_add(1, Ordering::SeqCst);
                
                // Print raw audio stats occasionally
                if count % 30 == 0 {
                    let max_amplitude = data.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
                    println!("\n📊 Raw audio buffer stats:");
                    println!("   Size: {} samples, Max amplitude: {:.3}", data.len(), max_amplitude);
                }

                // First get the frequency analysis
                let peaks = if let Ok(mut analyzer) = analyzer_clone.lock() {
                    if analyzer.get_window_size() != data.len() {
                        analyzer.resize_window(data.len());
                    }
                    analyzer.analyze_frequencies(data, 3)
                } else {
                    return;
                };

                // Print frequency analysis results
                if count % 10 == 0 {
                    println!("\n🎵 Frequency analysis (buffer #{}):", count);
                    if peaks.is_empty() {
                        println!("   No significant frequencies detected");
                    } else {
                        for (i, peak) in peaks.iter().enumerate() {
                            println!("   Peak #{}: {:.1} Hz (magnitude: {:.3})", 
                                i + 1, 
                                peak.frequency, 
                                peak.magnitude
                            );
                        }
                    }
                }

                // Then try to apply the rumble
                if let Ok(mut mapping) = rumble_mapping_clone.lock() {
                    let (freq, amp) = mapping.map_frequencies(&peaks);
                    
                    // Debug print rumble parameters
                    if count % 10 == 0 {
                        println!("🎮 Rumble parameters:");
                        println!("   Frequency: {:.1} Hz", freq);
                        println!("   Amplitude: {:.3}", amp);
                    }

                    // Apply rumble to all connected JoyCons
                    if let Ok(mut joycons) = joycons_clone.lock() {
                        for (i, joycon) in joycons.iter_mut().enumerate() {
                            match joycon.rumble(freq, amp) {
                                Ok(_) => {
                                    if count % 30 == 0 {
                                        println!("✅ JoyCon {} rumble sent", i + 1);
                                    }
                                }
                                Err(e) => println!("❌ JoyCon {} rumble error: {}", i + 1, e),
                            }
                        }
                    } else {
                        println!("❌ Failed to lock JoyCons");
                    }
                } else {
                    println!("❌ Failed to lock rumble mapping");
                }

                if let Ok(mut analyzer) = analyzer_clone.lock() {
                    // Get different frequency bands
                    let bass_peaks = analyzer.analyze_frequency_band(data, BASS_MIN_FREQ, BASS_MAX_FREQ);
                    let melody_peaks = analyzer.analyze_frequency_band(data, MELODY_MIN_FREQ, MELODY_MAX_FREQ);

                    // Debug output
                    if count % 10 == 0 {
                        println!("\n🎵 Frequency Analysis:");
                        println!("Bass Peaks:");
                        for peak in &bass_peaks {
                            println!("   {:.1} Hz (magnitude: {:.3})", peak.frequency, peak.magnitude);
                        }
                        println!("Melody Peaks:");
                        for peak in &melody_peaks {
                            println!("   {:.1} Hz (magnitude: {:.3})", peak.frequency, peak.magnitude);
                        }
                    }

                    // Apply to JoyCons
                    if let Ok(mut joycons) = joycons_clone.lock() {
                        if let Some(left_joycon) = joycons.get_mut(0) {
                            if let Some(bass_peak) = bass_peaks.first() {
                                let amp = (bass_peak.magnitude * 12.0).min(0.8);
                                let _ = left_joycon.rumble(bass_peak.frequency, amp);
                            }
                        }
                        
                        if let Some(right_joycon) = joycons.get_mut(1) {
                            if let Some(melody_peak) = melody_peaks.first() {
                                let amp = (melody_peak.magnitude * 10.0).min(0.7);
                                let _ = right_joycon.rumble(melody_peak.frequency, amp);
                            }
                        }
                    }
                }
            },
            move |err| eprintln!("Error in stream: {}", err),
            None,
        )?;

        Ok(AudioInput {
            stream: Some(stream),
            buffer_count,
            start_time,
            analyzer,
            rumble_mapping,
            joycons,
        })
    }

    // Start the audio input stream
    pub fn start(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(stream) = &self.stream {
            // Start the stream
            stream.play()?;
            println!("Playing output audio... Press Enter to stop.");
            println!("Capturing started at {:?}", self.start_time);
            let _ = std::io::stdin().read_line(&mut String::new())?;
            
            let total_buffers = self.buffer_count.load(Ordering::SeqCst);
            let elapsed = self.start_time.elapsed();
            println!("Capture ended. Processed {} buffers in {:?}", total_buffers, elapsed);
        }
        Ok(())
    }

    pub fn capture<F>(&self, callback: F)
    where
        F: Fn(&[f32]) + Send + 'static, // Closure for handling the captured audio data
    {
        // Here we would add functionality for continuous audio capture.
        // This can either buffer and then call the callback, or be done through a separate thread.
    }

    pub fn get_current_frequencies(&self, num_peaks: usize) -> Option<Vec<(f32, f32)>> {
        if let Ok(mut analyzer) = self.analyzer.lock() {
            // You'll need to implement a way to get the current buffer
            // This is just a placeholder - you'll need to modify the structure
            // to store the latest buffer
            None
        } else {
            None
        }
    }
}
