# Thermaltake screen protocol (264a:2348)

## Confirmed on hardware (2026-10-17)

The TH360 V3 Ultra speaks the **BY protocol**, the same one publicly documented
for the 6" panel `264a:2347`
([deniswsrosa/thermaltake-6inch-lcd-linux](https://github.com/deniswsrosa/thermaltake-6inch-lcd-linux)).
Telinhas has its own implementation in `crates/telinhas-core/src/protocol/by.rs`.

- Frame: `5A | length u16 BE (text + 5) | text | sum8 | 5A`, one per 1024-byte
  report (zeros after the final `5A`). sum8 = sum of the length and text bytes,
  mod 256. Between the delimiters, `5A` → `5B 01` and `5B` → `5B 02`.
- HTTP-like text, CRLF line endings:
  `POST conn 1` / `SeqNumber=112` / `ContentType=json` / `ContentLength=0` /
  empty line / JSON body.
- Reply to `conn` (`telinhas probe`): `1 200`, `AckNumber=113`,
  `ContentType=json`, `ContentLength=222` and the body
  `{"OS":"Linux","version":{"app":"V1.0.3","firmware":"V1.0.3","sdk":"V1.2.7","hardware":"V1.0"},"space":…,"brightness":100,"degree":0,"sn":"…","osdState":0,"mode":0,"logo":1,"timeout":5,"bootFinish":1}`.
  The controller runs embedded Linux. `timeout` (seconds) is how long the
  screen waits without frames before going back to its own picture.
- `POST realtimeDisplay 1` with `{"enable": true}` replies `200`.
- Live frames (`telinhas show`): a 720x720 JPEG in reports **without** the
  `5A` frame, each `5C 03 FD | counter 0–59 | total u16 BE | index u16 BE | 01
  | zeros up to byte 23 | 1000 bytes of JPEG` (last slice zero-padded), no
  pause between slices and no reply. Tested with a 14,805-byte red JPEG (15
  slices) resent every 2 s: the screen turned fully red. Limit of 0xC7FFF
  bytes per JPEG (from the 6" panel, not tested here).
- `POST brightness 1` with `{"value": 30}` and `POST rotate 1` with
  `{"degree": 90}` reply `200`, have a visible effect and persist: the next
  `conn` returns the new `brightness`/`degree`. (The brightness frame is 90 =
  `0x5A` bytes long, so its length field is escaped as `5B 01`; this confirms
  the escaping.)

Not tested yet: rotations 180/270, maximum frame rate.

## Static analysis of the official installer

Source: `TT LCD Screen_1.4.0.exe` (NSIS 3.04 with embedded 7z; Electron + Vue
app).

The main process (`out/main/index.jsc`) contains the headers `SeqNumber`,
`AckNumber`, `ContentLength`, `ContentType`, `FileName`, `FileBlockId`,
`FileSize`, `ContentRange`, `Counter`, `Option`; the methods `GET`, `POST`,
`STATE`, `DELETE`; and the commands `conn`, `realtimeDisplay`,
`displayInSleep`, `brightness`, `power`, `config`, `sysinfoDisplay`,
`fanLCDSet`, `waterBlockScreen`, `mediaDelete`, `recovery`, `factoryTest`,
`transport`, `transported`. **Do not send the last four** (storage writes,
recovery, firmware).

## Read directly from the files

- USB: VID `0x264a`, PID `0x2348`, "ThermalTake USB Device", USB 2.0.
- One HID interface, interrupt endpoints IN `0x81` and OUT `0x01`,
  `wMaxPacketSize` 1024, `bInterval` 4.
- HID report descriptor (36 bytes):
  `06 00 ff 09 01 a1 01 09 01 15 00 26 ff 00 75 08 96 00 04 81 02 09 01 15 00 26 ff 00 75 08 96 00 04 91 02 c0`
  = vendor usage page `0xFF00`; one input and one output report, 1024 bytes
  each, **no Report ID**.
- The app also supports Android-based screens over ADB (600x1024, 720x1480;
  Google VID `0x18D1`) and a serial port; **the TH360 shows up as HID**, not
  ADB.
- The app knows the `720x720` resolution.

## Where the protocol lives in the official app

- The UI (renderer) is minified JavaScript and does not contain the protocol.
- The main process is **V8 bytecode** (`out/main/index.jsc`); its strings show
  the flow: commands, a command queue with timeout/retry, `AckNumber`,
  checksum, "Protocol version not match", firmware updates from a `.zip` over
  HID or serial, JPEG frames.
- Transport lives in a native **Rust (napi-rs)** addon:
  `resources/app.asar` → `/node_modules/node-by-addons/node-by-addons.win32-x64-msvc.node`
  (20 MB, v1.16.0), using the `hidapi` 2.6.6 crate. Internal modules:
  `node_by_addons::devices::hid` (`DeviceInfo`, `HHidDevice`),
  `node_by_addons::protocol::low::frame_tlv` and
  `node_by_addons::protocol::file` (`meta.rs`, `mod.rs`). The TLV and "file"
  layers are probably used for file uploads (`transport`), which Telinhas does
  not implement.
- `BYMonitor64.dll` is a renamed `HWiNFO64.dll` (sensors); `MediaStation.dll`
  only processes media. Neither talks to the screen.

## If more details are needed

For what the public BY protocol does not cover (the addon's TLV layer, file
uploads), two options:

1. **USB capture** of the official app (Windows VM with the screen passed
   through, plus `usbmon` on the Linux host, filtering endpoint `0x01`), one
   isolated action at a time.
2. **Disassemble the addon in Ghidra** (import the `.node` as a PE x64).
   Starting points (image base `0x180000000`):
   - `frame_tlv` strings: `0x180e6bf40` (.rs path), messages at
     `0x180e6c0c1`–`0x180e6c146`
   - TLV log call (`tracing` macro): `0x180059f12`
   - HID write wrapper ("size can not be 0"): `0x180c9777d`
   - "Failed to create data frame tlv": `0x1802e65f0`, `0x1802e6cb9`

## Rules

- Never send unknown data to the device (especially anything resembling
  firmware).
- Implement protocols from scratch; do not copy Thermaltake code or binaries.
