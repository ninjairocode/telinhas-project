# Changelog

All notable changes to Telinhas are listed here. Versions follow
[Semantic Versioning](https://semver.org); while the version is `0.x`, any
release may change behavior.

## [0.2.2] - 2026-11-10

### Fixed

- The udev rule is now installed as `70-telinhas.rules`. As
  `99-telinhas.rules` it ran after systemd applies the `uaccess` permissions,
  so on some distributions (Fedora, for example) the screen stayed
  inaccessible without root. Remove the old file when updating.
- After the computer woke up from sleep, the screen sometimes stayed on its
  own picture until Telinhas was restarted. Telinhas now notices the wake-up
  and reconnects to the screen by itself.

### Changed

- Distribution is AppImage-only for now; the Flatpak manifest and the Flatpak
  code paths were removed.
- Documentation no longer ties the project to a specific distribution or
  desktop, and states how it is made: architected by a human, coded by AI.

## [0.2.1] - 2026-10-31

### Changed

- Faster JPEG encoding: drawing metrics over each GIF frame takes about half
  the CPU time, so 20 frames/s GIFs keep their speed with all metrics on.

### Fixed

- Flatpak: "Start with the system" now works on desktops without the
  Background portal, such as COSMIC.

## [0.2.0] - 2026-10-31

### Added

- Animated GIFs on the screen (up to 20 frames per second).
- System metrics drawn over the image or GIF: CPU temperature, CPU usage,
  GPU temperature and memory usage, chosen with checkboxes, at the top,
  middle or bottom of the screen (middle suits round screens). Without an
  image, they are shown over a dark background.
- Background mode: closing the window keeps the image on the screen; opening
  Telinhas again brings the window back; "Quit" exits. Option to start with
  the system, restoring the last image, metrics and settings.
- Command line: `show` plays GIFs and accepts `--metrics` and
  `--metrics-position`; `prepare` accepts them too.
- Flatpak manifest (`packaging/flatpak/`), towards the COSMIC Store.
- Guide for adding new devices (`docs/ADDING_A_DEVICE.md`).

### Changed

- The interface and the command line are now in English.
- Internal driver structure: every device implements one common interface,
  so new screens can be added without touching the app.

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

[0.2.2]: https://github.com/ninjairocode/telinhas-project/releases/tag/v0.2.2
[0.2.1]: https://github.com/ninjairocode/telinhas-project/releases/tag/v0.2.1
[0.2.0]: https://github.com/ninjairocode/telinhas-project/releases/tag/v0.2.0
[0.1.0]: https://github.com/ninjairocode/telinhas-project/releases/tag/v0.1.0
