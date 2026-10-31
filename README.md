<p align="center">
  <img src="packaging/icons/hicolor/256x256/apps/io.github.ninjairocode.Telinhas.png" alt="Telinhas logo" width="160">
</p>

<h1 align="center">Telinhas</h1>

<p align="center">
  Manage the mini screens of your PC on Linux, all in one place.<br>
  <a href="README.pt-BR.md">Leia em português</a>
</p>

> [!WARNING]
> **Early-stage project.** Telinhas currently supports **a single device**
> (see [Supported devices](#supported-devices)). Compatibility will grow with
> each update. Expect rough edges, and please report problems.

<p align="center">
  <img src="docs/screenshots/main-window.png" alt="Telinhas main window" width="420">
</p>

## What it is

PCs are getting more and more small screens: AIO cooler displays, case panels,
USB mini monitors. Each brand ships its own Windows-only app. Telinhas aims to
manage all of them from a single Linux app, written in Rust, with a native
interface for the [COSMIC](https://system76.com/cosmic) desktop.

**Today you can:**

- show an image or an animated GIF on the screen (cropped to its resolution);
- draw system metrics over it: CPU and GPU temperature, CPU and memory usage;
- adjust brightness and rotation;
- keep it running in the background and start it with the system;
- plug and unplug the screen while the app is running (it reconnects by itself).

**Planned:**

- support for more device models and brands;
- more metrics and layouts;
- video.

## Supported devices

| Device | USB ID | Status |
|---|---|---|
| Thermaltake TH360 V3 Ultra ARGB Sync (720×720 LCD) | `264a:2348` | Works (image, GIF, metrics, brightness, rotation) |

Other Thermaltake screens that use the same "BY" protocol may work with small
changes, but they have not been tested. If you own a different screen and want
to help, see [Contributing](#contributing).

## Install

Telinhas is distributed in two ways:

| Channel | Status | Updates |
|---|---|---|
| **AppImage** from [GitHub Releases](https://github.com/ninjairocode/telinhas-project/releases) | Available | Manual or with AppImageUpdate (see [Updating](#updating)) |
| **COSMIC Store** (Flatpak, via Flathub) | Coming soon | Automatic, through the store |

### AppImage

1. Download `Telinhas-x86_64.AppImage` from the
   [latest release](https://github.com/ninjairocode/telinhas-project/releases/latest).
2. Make it executable:
   ```
   chmod +x Telinhas-x86_64.AppImage
   ```
3. **Once:** allow access to the screen without root (udev rule), then unplug
   and plug the screen's USB cable back in:
   ```
   ./Telinhas-x86_64.AppImage udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules
   sudo udevadm control --reload && sudo udevadm trigger
   ```
   If you skip this step, the app shows the command for you to copy.
4. Open it with a double click, or run `./Telinhas-x86_64.AppImage`.

**Requirements:** a 64-bit (x86_64) Linux with a Wayland or X11 desktop and
working graphics drivers (Vulkan or OpenGL). The interface is designed for
COSMIC but also runs on other desktops. If the AppImage fails to start with a
FUSE error (common in containers), run it with `--appimage-extract-and-run`.

### COSMIC Store

Coming soon. The app will be published on Flathub, which is the source the
COSMIC Store uses. The udev rule above will still be needed, because Flatpak
apps cannot install it on the system by themselves.

## Usage

### Graphical interface

1. Click **Choose image…** and pick a JPEG, PNG, WebP or (animated) GIF file.
   A preview shows how it will be cropped.
2. Under **Metrics on screen**, tick the metrics you want and pick their
   **Position** (top, middle or bottom; middle works best on round screens).
   Without an image, metrics are shown over a dark background.
3. Click **Show on screen**.
4. Adjust **Brightness** (applied when you release the slider) and
   **Rotation**.

**Background mode:** closing the window does not stop the screen; Telinhas
keeps running. Open it again from the app menu to bring the window back, and
use **Quit** (top bar) to exit. Turn on **Start with the system** to restore
the image, metrics and settings every time you log in.

> [!NOTE]
> The screen only keeps a custom image **while Telinhas is running**: the app
> keeps sending it, and the screen goes back to its own picture about
> 5 seconds after that stops (for example, after **Quit**).

Settings are stored in `~/.config/telinhas/`.

### Command line

The AppImage also works as a command-line tool when given arguments
(`telinhas` when built from source):

```
./Telinhas-x86_64.AppImage list                         # find supported screens
./Telinhas-x86_64.AppImage show photo.png --seconds 30  # show an image
./Telinhas-x86_64.AppImage show anim.gif --metrics cpu-temp,ram-usage
./Telinhas-x86_64.AppImage show --color ff0000          # solid color
./Telinhas-x86_64.AppImage set --brightness 60 --rotate 90
./Telinhas-x86_64.AppImage prepare photo.png -o out.jpg # only convert, send nothing
./Telinhas-x86_64.AppImage probe                        # handshake, show versions
./Telinhas-x86_64.AppImage listen --seconds 10          # read-only debugging
```

`probe`, `show` and `set` send data to the device; add `--dry-run` to see what
would be sent without sending it. Metrics: `cpu-temp`, `cpu-usage`,
`gpu-temp`, `ram-usage`; position: `--metrics-position top|middle|bottom`.

## Updating

| Installed via | How to update |
|---|---|
| **AppImage** | Download the new `Telinhas-x86_64.AppImage` from [Releases](https://github.com/ninjairocode/telinhas-project/releases/latest) and replace the old file (run `chmod +x` again). Or use [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate): `AppImageUpdate Telinhas-x86_64.AppImage` downloads only what changed. |
| **COSMIC Store / Flatpak** | Updates arrive through the store. From a terminal: `flatpak update`. |
| **Source** | `git pull`, then `cargo build --release` again. |

The udev rule does not need to be reinstalled when updating, unless the release
notes say so (for example, when a new device is added).

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.

## Build from source

Requires [Rust](https://rustup.rs) 1.93 or newer and, on Ubuntu/Debian/Pop!_OS:

```
sudo apt install libudev-dev libxkbcommon-dev libwayland-dev pkg-config
git clone https://github.com/ninjairocode/telinhas-project.git
cd telinhas-project
cargo build --release              # target/release/telinhas-gui and telinhas
cargo build --release -p telinhas  # command line only, without libcosmic
```

The first build downloads and compiles [libcosmic](https://github.com/pop-os/libcosmic)
and takes a few minutes. Install the udev rule from
`packaging/99-telinhas.rules` as shown in [AppImage](#appimage).

### Project layout

| Path | Contents |
|---|---|
| `crates/telinhas-core` | Device drivers, transports, protocols, images/GIFs, sensors, metrics overlay |
| `crates/telinhas` | Command-line tool |
| `crates/telinhas-gui` | COSMIC interface (libcosmic) |
| `packaging/` | udev rule, desktop entry, AppStream metadata, icons |
| `packaging/flatpak/` | Flatpak manifest |
| `docs/PROTOCOL.md` | Notes on the Thermaltake "BY" protocol |
| `docs/ADDING_A_DEVICE.md` | How to add support for a new screen |

### Flatpak

```
sudo apt install flatpak-builder python3-aiohttp python3-tomlkit python3-yaml
curl -O https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
flatpak-builder --user --install --install-deps-from=flathub --force-clean \
  build-dir packaging/flatpak/io.github.ninjairocode.Telinhas.yml
flatpak run io.github.ninjairocode.Telinhas
```

The Noto Sans font in `crates/telinhas-core/assets/fonts/` is under the SIL
Open Font License 1.1 (`OFL.txt` next to it).

## Contributing

New devices are the most valuable contribution. Open an
[issue](https://github.com/ninjairocode/telinhas-project/issues) with the
screen model and the output of `lsusb`. Each device becomes a self-contained
driver in `crates/telinhas-core/src/devices/`; the step-by-step guide is in
[docs/ADDING_A_DEVICE.md](docs/ADDING_A_DEVICE.md).

## Support

- Bugs and requests: [GitHub Issues](https://github.com/ninjairocode/telinhas-project/issues)
- E-mail: [ninjairocode@outlook.com](mailto:ninjairocode@outlook.com)

## License and legal notice

Telinhas is free software under the [GNU GPL v3.0 or later](LICENSE).

Device protocols were implemented from scratch, for interoperability only. This
repository contains no code, images or binaries from hardware vendors. Product
names are trademarks of their respective owners; Telinhas is not affiliated
with or endorsed by them.
