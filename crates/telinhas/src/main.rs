use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Parser, Subcommand};
use telinhas_core::devices::{self, Bus, Connection, DeviceInfo, FrameFormat, KNOWN_DEVICES};
use telinhas_core::imaging;
use telinhas_core::protocol::by;
use telinhas_core::render::{self, Position};
use telinhas_core::sensors::{Metric, Sensors};
use telinhas_core::transport::HidTransport;
use telinhas_core::Screen;

#[derive(Parser)]
#[command(name = "telinhas", version, about = "Control PC mini screens on Linux")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the supported screens that are connected
    List,
    /// Listen (read-only) to what the device sends
    Listen {
        /// How many seconds to listen
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Debugging: send only the `conn` handshake of the BY protocol
    /// (Thermaltake) and show the raw reply
    Probe {
        /// Only print the bytes that would be sent, without opening the device
        #[arg(long)]
        dry_run: bool,
    },
    /// Show an image (or a solid color) on the screen for a few seconds
    Show {
        /// Image (JPEG, PNG, WebP or animated GIF), cropped to the screen
        #[arg(required_unless_present = "color", conflicts_with = "color")]
        input: Option<PathBuf>,
        /// Solid color in hexadecimal, for example ff0000
        #[arg(long, value_parser = parse_rgb)]
        color: Option<[u8; 3]>,
        /// How many seconds to keep the image (the screen reverts by itself afterwards)
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Interval between frame resends, in milliseconds (default: what the
        /// model needs to keep the image)
        #[arg(long)]
        interval_ms: Option<u64>,
        /// Metrics drawn over the image, comma-separated:
        /// cpu-temp, cpu-usage, gpu-temp, ram-usage
        #[arg(long, value_delimiter = ',', value_parser = parse_metric)]
        metrics: Vec<Metric>,
        /// Where to draw the metrics
        #[arg(long, default_value = "bottom", value_parser = parse_position)]
        metrics_position: Position,
        /// Only prepare the frame and report what would be sent, without
        /// opening the device
        #[arg(long)]
        dry_run: bool,
    },
    /// Set the screen brightness and/or rotation
    Set {
        /// Brightness from 0 to 100
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: Option<u8>,
        /// Rotation in degrees
        #[arg(
            long,
            value_name = "DEGREES",
            value_parser = PossibleValuesParser::new(["0", "90", "180", "270"])
                .map(|s| s.parse::<u16>().unwrap())
        )]
        rotate: Option<u16>,
        /// Only print what would be changed, without opening the device
        #[arg(long)]
        dry_run: bool,
    },
    /// Convert an image to the JPEG the screen expects (sends nothing)
    Prepare {
        /// Input image (JPEG, PNG, WebP or GIF)
        input: PathBuf,
        /// Output JPEG file
        #[arg(short, long)]
        output: PathBuf,
        /// Width in pixels (default: the first supported model)
        #[arg(long, default_value_t = KNOWN_DEVICES[0].width)]
        width: u32,
        /// Height in pixels (default: the first supported model)
        #[arg(long, default_value_t = KNOWN_DEVICES[0].height)]
        height: u32,
        /// JPEG quality (1–100)
        #[arg(long, default_value_t = imaging::DEFAULT_QUALITY)]
        quality: u8,
        /// Metrics drawn over the image, comma-separated:
        /// cpu-temp, cpu-usage, gpu-temp, ram-usage
        #[arg(long, value_delimiter = ',', value_parser = parse_metric)]
        metrics: Vec<Metric>,
        /// Where to draw the metrics
        #[arg(long, default_value = "bottom", value_parser = parse_position)]
        metrics_position: Position,
    },
    /// Print the udev rule needed to access the device without root
    UdevRules,
}

/// Abre a primeira tela conhecida e faz o handshake.
fn open_screen() -> Result<Box<dyn Screen>, Box<dyn std::error::Error>> {
    let mut screen = Bus::new()?.open_first()?;
    let s = screen.connect()?;
    println!("Connected to {} ({})", screen.info().name, describe(&s));
    Ok(screen)
}

fn describe(s: &telinhas_core::ScreenStatus) -> String {
    let mut parts = Vec::new();
    if let Some(f) = &s.firmware {
        parts.push(format!("firmware {f}"));
    }
    if let Some(b) = s.brightness {
        parts.push(format!("brightness {b}"));
    }
    if let Some(r) = s.rotation {
        parts.push(format!("rotation {r}°"));
    }
    parts.join("; ")
}

fn usb_id(info: &DeviceInfo) -> String {
    match info.connection {
        Connection::UsbHid { vid, pid } => format!("{vid:04x}:{pid:04x}"),
    }
}

/// Primeira tela conectada usando o transporte HID (para os comandos de
/// depuração de baixo nível).
fn open_hid_raw() -> Result<(&'static DeviceInfo, HidTransport), Box<dyn std::error::Error>> {
    let bus = Bus::new()?;
    let info = bus
        .detect()
        .into_iter()
        .next()
        .ok_or(telinhas_core::Error::NoDevice)?;
    let Connection::UsbHid { vid, pid } = info.connection;
    Ok((info, HidTransport::open(bus.hid(), vid, pid)?))
}

fn parse_metric(s: &str) -> Result<Metric, String> {
    Metric::ALL
        .into_iter()
        .find(|m| metric_name(*m) == s)
        .ok_or_else(|| {
            let names: Vec<_> = Metric::ALL.into_iter().map(metric_name).collect();
            format!("use one of: {}", names.join(", "))
        })
}

fn parse_position(s: &str) -> Result<Position, String> {
    Position::ALL
        .into_iter()
        .find(|p| p.label().eq_ignore_ascii_case(s))
        .ok_or_else(|| "use top, middle or bottom".into())
}

fn metric_name(m: Metric) -> &'static str {
    match m {
        Metric::CpuTemp => "cpu-temp",
        Metric::CpuUsage => "cpu-usage",
        Metric::GpuTemp => "gpu-temp",
        Metric::RamUsage => "ram-usage",
    }
}

fn parse_rgb(s: &str) -> Result<[u8; 3], String> {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16)
        .ok()
        .filter(|_| s.len() == 6)
        .ok_or("use 6 hexadecimal digits, for example ff0000")?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::UdevRules => {
            print!("{}", devices::udev_rules());
            eprintln!(
                "\nInstall with:\n  telinhas udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules\n  sudo udevadm control --reload && sudo udevadm trigger\nThen plug the USB cable back in."
            );
        }
        Cmd::Probe { dry_run } => {
            let text = by::request("conn", by::DEFAULT_SEQ, "");
            let frame = by::encode_frame(&text)?;
            println!("Command: {:?}", String::from_utf8_lossy(&text));
            println!("Frame ({} bytes): {:02x?}", frame.len(), frame);
            if dry_run {
                return Ok(());
            }

            let (dev, t) = open_hid_raw()?;
            // Descarta relatórios antigos para não confundi-los com a resposta.
            while t.read_report(50)?.is_some() {}

            t.write_report(&frame)?;
            println!("Sent to {}. Waiting for a reply (3 s)...", dev.name);
            let end = Instant::now() + Duration::from_secs(3);
            while Instant::now() < end {
                let Some(r) = t.read_report(200)? else {
                    continue;
                };
                println!("Report ({} bytes): {:02x?}", r.len(), &r[..r.len().min(64)]);
                match by::decode_frame(&r).and_then(|p| by::Response::parse(&p)) {
                    Ok(resp) => {
                        println!("Status: {}", resp.status_line);
                        for (k, v) in &resp.headers {
                            println!("  {k} = {v}");
                        }
                        if !resp.body.is_empty() {
                            println!("Body: {}", resp.body);
                        }
                        return Ok(());
                    }
                    Err(e) => println!("  (not a BY reply: {e})"),
                }
            }
            println!("No BY reply within 3 s.");
        }
        Cmd::Show {
            input,
            color,
            seconds,
            interval_ms,
            metrics,
            metrics_position,
            dry_run,
        } => {
            // Sem dispositivo (--dry-run), prepara para o primeiro modelo.
            let mut screen = if dry_run { None } else { Some(open_screen()?) };
            let info = screen.as_ref().map_or(&KNOWN_DEVICES[0], |s| s.info());
            let FrameFormat::Jpeg { quality } = info.frame;
            let frames = match (&input, color) {
                (Some(path), _) => {
                    imaging::prepare(&std::fs::read(path)?, info.width, info.height, quality)?
                        .frames
                }
                (None, Some(rgb)) => vec![imaging::PreparedFrame {
                    jpeg: imaging::solid_jpeg(info.width, info.height, rgb, quality)?,
                    delay: None,
                }],
                (None, None) => unreachable!("clap requires an image or --color"),
            };
            let total: usize = frames.iter().map(|f| f.jpeg.len()).sum();
            println!(
                "{} JPEG frame(s) {}x{} for {}: {} bytes in total",
                frames.len(),
                info.width,
                info.height,
                info.name,
                total
            );
            let Some(screen) = screen.as_mut() else {
                return Ok(());
            };

            let mut interval = interval_ms
                .map(Duration::from_millis)
                .or(info.keepalive)
                .unwrap_or(Duration::from_secs(2));
            let mut sensors = Sensors::new();
            if !metrics.is_empty() {
                // Atualiza as métricas a cada segundo.
                interval = interval.min(Duration::from_secs(1));
            }
            let start = Instant::now();
            let end = start + Duration::from_secs(seconds);
            let (mut n, mut busy) = (0u32, Duration::ZERO);
            for frame in frames.iter().cycle() {
                if Instant::now() >= end {
                    break;
                }
                let t = Instant::now();
                if metrics.is_empty() {
                    screen.show_frame(&frame.jpeg)?;
                } else {
                    let readings = sensors.sample(&metrics);
                    screen.show_frame(&render::compose_jpeg(
                        &frame.jpeg,
                        &readings,
                        metrics_position,
                        quality,
                    )?)?;
                }
                busy += t.elapsed();
                n += 1;
                std::thread::sleep(frame.delay.unwrap_or(interval).saturating_sub(t.elapsed()));
            }
            let secs = start.elapsed().as_secs_f64();
            println!(
                "{n} frames in {secs:.1} s ({:.1} frames/s; {:.1} ms average send time)",
                f64::from(n) / secs,
                busy.as_secs_f64() * 1000.0 / f64::from(n.max(1))
            );
            println!("Done; the screen reverts to its own picture after its timeout.");
        }
        Cmd::Set {
            brightness,
            rotate,
            dry_run,
        } => {
            if brightness.is_none() && rotate.is_none() {
                eprintln!("Nothing to change; use --brightness and/or --rotate.");
                std::process::exit(2);
            }
            if dry_run {
                if let Some(v) = brightness {
                    println!("Would set brightness to {v}.");
                }
                if let Some(d) = rotate {
                    println!("Would set rotation to {d}°.");
                }
                return Ok(());
            }

            let mut screen = open_screen()?;
            if let Some(v) = brightness {
                screen.set_brightness(v)?;
                println!("Brightness set to {v}.");
            }
            if let Some(d) = rotate {
                screen.set_rotation(d)?;
                println!("Rotation set to {d}°.");
            }
            println!("Current state: {}", describe(&screen.connect()?));
        }
        Cmd::Prepare {
            input,
            output,
            width,
            height,
            quality,
            metrics,
            metrics_position,
        } => {
            let mut jpeg = imaging::prepare_jpeg(&std::fs::read(&input)?, width, height, quality)?;
            if !metrics.is_empty() {
                let mut sensors = Sensors::new();
                std::thread::sleep(Duration::from_millis(500)); // base para o uso de CPU
                jpeg = render::compose_jpeg(
                    &jpeg,
                    &sensors.sample(&metrics),
                    metrics_position,
                    quality,
                )?;
            }
            std::fs::write(&output, &jpeg)?;
            println!(
                "{} ({width}x{height}, {} KiB)",
                output.display(),
                jpeg.len().div_ceil(1024)
            );
        }
        Cmd::List => {
            let bus = Bus::new()?;
            let found = bus.detect();
            if found.is_empty() {
                println!("No supported screen found.");
                println!("Supported models:");
                for k in KNOWN_DEVICES {
                    println!("  {}  {}", usb_id(k), k.name);
                }
                return Ok(());
            }
            for k in found {
                print!("{}  {} ({}x{})  ", usb_id(k), k.name, k.width, k.height);
                match bus.open(k) {
                    Ok(_) => println!("[access OK]"),
                    Err(e) => {
                        println!("[no access: {e}]");
                        println!("  hint: run `telinhas udev-rules` and follow the instructions.");
                    }
                }
            }
        }
        Cmd::Listen { seconds } => {
            let (dev, t) = open_hid_raw()?;
            println!("Listening to {} for {seconds} s (read-only)...", dev.name);
            let end = Instant::now() + Duration::from_secs(seconds);
            let mut n = 0u32;
            while Instant::now() < end {
                if let Some(r) = t.read_report(200)? {
                    n += 1;
                    println!("#{n} {} bytes: {:02x?}", r.len(), &r[..r.len().min(64)]);
                }
            }
            println!("{n} report(s) received.");
        }
    }
    Ok(())
}
