# Adding a new device

Telinhas is built so that each screen model is a self-contained driver. The
command-line tool and the graphical interface only talk to the `Screen` trait,
so a new driver works in both without touching them.

## 1. Gather information (read-only)

1. Plug the screen in and run `lsusb` to get its USB ID (`vvvv:pppp`).
2. Find out how it shows up: HID (`/dev/hidraw*`), serial (`/dev/ttyACM*`,
   `/dev/ttyUSB*`), raw USB bulk endpoints, or ADB. `lsusb -v -d vvvv:pppp`
   lists its interfaces and endpoints.
3. Look for existing public documentation or open-source drivers for the same
   model or a sibling model. Many vendors reuse one protocol across products.
4. If nothing exists, capture the official app's traffic: a Windows VM with
   the screen passed through, and `usbmon` + Wireshark on the Linux host.
   Record one isolated action per capture (connect; send a solid red image;
   change brightness) so the captures can be compared.

Write down what you learn in `docs/` (see `docs/PROTOCOL.md` for the
Thermaltake example).

> [!CAUTION]
> Never send data whose meaning you do not know, and never send anything that
> looks like a firmware update, file upload or factory command. Test one new
> command at a time, starting with read-only ones such as a handshake.

## 2. Write the driver

Create `crates/telinhas-core/src/devices/<vendor>.rs`:

- A `pub const MODEL: DeviceInfo` with the name, connection (USB ID),
  resolution, frame format, `keepalive` (if the screen drops the image when
  frames stop) and the `open` function.
- A struct that implements `telinhas_core::Screen`:
  - `connect()` performs the handshake and returns a `ScreenStatus`;
  - `show_frame()` sends one frame in the format declared in `DeviceInfo`;
  - `capabilities()`, plus `set_brightness()` / `set_rotation()` when the
    screen supports them (the defaults return `Error::Unsupported`).
- Protocol framing that is not tied to one model goes in
  `crates/telinhas-core/src/protocol/`, with unit tests that compare against
  bytes from real captures.

Then register it:

- add `pub mod <vendor>;` and the `DeviceInfo` to `KNOWN_DEVICES` in
  `crates/telinhas-core/src/devices/mod.rs`;
- regenerate the udev rule:
  `cargo run -p telinhas -- udev-rules > packaging/70-telinhas.rules`
  (a test fails if you forget);
- add a `<modalias>` line to `packaging/io.github.ninjairocode.Telinhas.metainfo.xml`.

If the device needs a transport other than HID, add it to `devices::Bus` and a
new variant to `devices::Connection`.

## 3. Test

```
cargo test
cargo run -p telinhas -- list
cargo run -p telinhas -- show --color ff0000 --dry-run
cargo run -p telinhas -- show --color ff0000 --seconds 5
cargo run -p telinhas -- set --brightness 50
cargo run -p telinhas-gui
```

## 4. Contribute

Open a pull request with the driver, the protocol notes, and the devices you
tested on. If you cannot write the driver yourself, open an issue with the
model, the `lsusb -v` output and any captures. Never include vendor code,
firmware or binaries; implement the protocol from scratch.
