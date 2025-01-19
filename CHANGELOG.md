# Changelog

All notable changes to Musical-Joycons will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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