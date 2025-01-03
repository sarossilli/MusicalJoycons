# Musical-Joycons (Rust Edition) 🦀
> *Breathing new life into vibrotactile MIDI experiences*

[<img src="https://raw.githubusercontent.com/Sarossilli/Musical-Joycons/master/readme/header.gif" alt="HEADER">]()

## Project Vision
Taking the novel concept of MIDI playback through Nintendo Switch JoyCon haptics and reimagining it in Rust. This isn't just a port - it's a complete architectural rethink leveraging Rust's robust type system and zero-cost abstractions.

I mainly want to do this to learn rust - but also I think the C++ version was poorly made durring my time in school. This should be a better version that is actually thought out.

## 🚧 Development Status
This is an active rewrite of the [original C++ implementation](https://github.com/sarossilli/Musical-Joycons). While we're making steady progress, expect significant architectural changes and evolving APIs.

## Core Features (In Development)
* **Haptic MIDI Playback**: Transform MIDI files into precise HD rumble patterns
* **Bluetooth Integration**: Native JoyCon connectivity using modern Rust async patterns
* **Multi-track Support**: Independent control over dual JoyCon haptics
* **Zero-overhead Abstractions**: Leveraging Rust's type system for performant, safe hardware interaction

## Technical Architecture
```
musical-joycons/
├── src/
│   ├── joycon/     # Bluetooth and haptic control
│   ├── audio/      # MIDI/Audio processing pipeline
│   └── config/     # Runtime configuration
```

## Development Setup
```bash
# Clone the repository
git clone 

# Build development version
cargo build

# Run tests
cargo test
```

## Dependencies
* `btleplug`: Rust-native Bluetooth LE connectivity
* Additional dependencies to be determined during development

## Roadmap
1. [ ] Core JoyCon communication layer
2. [ ] MIDI parsing and transformation
3. [ ] Haptic pattern generation
4. [ ] Configuration and UI layer

## Contributing
While the project is in active development, we welcome discussions and ideas. Feel free to open issues for feature suggestions or architectural feedback.

## License
*TBD*

## Acknowledgments
* Nintendo Switch Reverse Engineering community
* Rust Embedded community

---
*This README reflects the current development state and will evolve alongside the project.*