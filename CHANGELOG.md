# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

No unreleased changes yet!

## [0.2.2] - 2024-04-22

### Fixed
* Fixed an issue where the `embedded-io` traits were not implemented if custom buffers were used.

## [0.2.1] - 2024-03-06

### Added
* Added the following `embedded-io` traits to the `SerialPort` object: `Write`, `WriteReady`,
  `Read`, and `ReadReady`, and `ErrorType`
* The `CdcAcmClass` now exposes the IN and OUT endpoints for direct access via the `read_ep{_mut}`
  and `write_ep{_mut}` functions.

## [0.2.0] - 2023-11-13

### Added
- Support assigning interface name strings to the control and data interfaces
- Changed default baud rate from 8000 bps to 9600 bps

### Changed
- `usb-device` version bumped to 0.3.0

### Fixed
- [breaking] `Parity::Event` was fixed to `Parity::Even`

## [0.1.1] - 2020-10-03

## 0.1.0 - 2019-07-24

This is the initial release to crates.io.

[Unreleased]: https://github.com/rust-embedded-community/usbd-serial/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/rust-embedded-community/usbd-serial/releases/tag/v0.2.2
[0.2.1]: https://github.com/rust-embedded-community/usbd-serial/releases/tag/v0.2.1
[0.2.0]: https://github.com/rust-embedded-community/usbd-serial/releases/tag/v0.2.0
[0.1.1]: https://github.com/rust-embedded-community/usbd-serial/compare/v0.1.0...v0.1.1
