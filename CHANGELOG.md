# Changelog

All notable changes to Telinhas are listed here. Versions follow
[Semantic Versioning](https://semver.org); while the version is `0.x`, any
release may change behavior.

## [0.1.0] - 2026-10-17

First public release. Early stage: one supported device.

### Added

- Support for the Thermaltake TH360 V3 Ultra ARGB Sync LCD (`264a:2348`),
  through the Thermaltake "BY" protocol: show an image, brightness, rotation.
- Graphical interface for COSMIC (`telinhas-gui`): image picker with preview,
  show/stop, brightness and rotation controls, automatic reconnection, and
  help installing the udev rule.
- Command-line tool (`telinhas`): `list`, `show`, `set`, `prepare`, `probe`,
  `listen`, `udev-rules`.
- AppImage built by GitHub Actions, with update information for AppImageUpdate.

[0.1.0]: https://github.com/ninjairocode/telinhas-project/releases/tag/v0.1.0
