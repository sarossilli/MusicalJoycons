use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use std::error::Error;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Instant;

pub struct AudioInput {
    stream: Option<Stream>,
    buffer_count: Arc<AtomicUsize>,
    start_time: Instant,
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
            buffer_size: cpal::BufferSize::Fixed(512), // Smaller buffer size for lower latency
        };

        let buffer_count = Arc::new(AtomicUsize::new(0));
        let start_time = Instant::now();

        let buffer_count_clone = Arc::clone(&buffer_count);
        let stream = selected_device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let count = buffer_count_clone.fetch_add(1, Ordering::SeqCst);
                let avg_amplitude: f32 = data.iter().map(|&x| x.abs()).sum::<f32>() / data.len() as f32;
                if count % 10 == 0 {  // More frequent updates (every 10th buffer instead of 100th)
                    println!("Buffer #{}: {} samples, avg amplitude: {:.6}", 
                        count, data.len(), avg_amplitude);
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
}
