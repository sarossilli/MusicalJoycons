# Changelog

All notable changes to Musical-Joycons will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] - 2026-02-26
### Added
- ARM64 (aarch64) Linux build target for use on devices like the Nintendo Switch running L4T Ubuntu.
- Native ARM64 GitHub Actions runner (`ubuntu-24.04-arm`) for building and testing release binaries.

## [0.1.2] - 2026-02-24
### Added
- Part-based MIDI normalization: notes grouped by `(channel, program)` into musical Parts, replacing raw track-based analysis.
- Dual scoring models: separate Primary (melody-likeness) and Secondary (accompaniment) scorers with weighted feature vectors.
- Rich feature computation per Part: monophony ratio, chordiness, stepwise motion, pitch/velocity percentiles, instrument priors, and active ratio.
- Runtime L/R swap and part cycling: press S to swap Joy-Con sides, 1/2 to cycle primary/secondary candidates during playback.
- Keyboard input thread using `crossterm` for non-blocking runtime controls.

### Changed
- Frequency mapping now octave-shifts notes into the 200–1252 Hz range so low-pitched MIDI parts produce audible rumble.
- `active_ratio` computed against full song duration instead of per-part span, preventing sparse fill-parts from appearing artificially active.
- Primary/secondary scoring boosted `active_ratio` and `total_note_time` weights; added hard penalties for parts covering less than 10–25% of the song.
- Playback timing uses `Instant`-based wall-clock scheduling instead of accumulated `thread::sleep` durations, eliminating drift between Joy-Cons.
- `ticks_to_duration` upgraded from `f32`/`u64` microsecond truncation to full `f64` precision.
- Skyline algorithm now operates on a constrained candidate pool from the scoring models rather than all raw MIDI tracks.
- HID writes coalesced for same-tick commands with a 2ms minimum interval to prevent USB pipeline overflow.

### Fixed
- Songs with many sparse monophonic parts no longer select nearly-silent fill tracks as primary/secondary.
- Joy-Con timing desync caused by HID I/O overhead and Windows sleep granularity compounding over the song.
- Windows HID "overlapped I/O" crash when rumble commands were sent too rapidly.

## [0.1.1] - 2025-01-05
### Added
- Track Merging and Ordering:
    - Added the ability to merge and order MIDI tracks based on their score to enhance the overall musical experience.
    - Introduced new methods and functions to handle track scoring and switching.

### Changed
- Refactored MIDI Playback:
    - Moved MIDI playing code from main.rs to a new dedicated module playback.rs for better code organization and readability.
    - Refactored JoyCon::rumble method to clamp amplitude values between 0 and 1.
    - Updated DeviceInfo struct with new fields (path, vendor_id, usage_page).

### Removed
    - Removed deprecated code in main.rs related to track selection and JoyCon synchronization.
    - Removed user ability to select tracks in favor of auto-selection.

### Fixed
- Unit Tests:
    - Added unit tests to ensure the correctness and reliability of the new features and refactored code.


## [0.1.0] - 2025-01-05
### Added
- Initial release
- Core JoyCon communication layer
- Basic MIDI parsing and transformation
- Support for haptic feedback
- Multi-platform support (Windows, Linux, MacOS)
- Dual JoyCon support with synchronized playback