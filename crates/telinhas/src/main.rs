use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Parser, Subcommand};
use telinhas_core::devices::{self, Bus, Connection, DeviceInfo, FrameFormat, KNOWN_DEVICES};
use telinhas_core::imaging;
use telinhas_core::protocol::by;
use telinhas_core::transport::HidTransport;
use telinhas_core::Screen;

#[derive(Parser)]
#[command(
    name = "telinhas",
    version,
    about = "Controle mini telas de PC no Linux"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Lista as telas conhecidas que estão conectadas
    List,
    /// Escuta (somente leitura) o que o dispositivo envia
    Listen {
        /// Quantos segundos escutar
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Depuração: envia só o handshake `conn` do protocolo BY (Thermaltake)
    /// e mostra a resposta crua
    Probe {
        /// Só mostra os bytes que seriam enviados, sem abrir o dispositivo
        #[arg(long)]
        dry_run: bool,
    },
    /// Mostra uma imagem (ou uma cor sólida) na tela por alguns segundos
    Show {
        /// Imagem (JPEG, PNG ou WebP); recortada e redimensionada para a tela
        #[arg(required_unless_present = "color", conflicts_with = "color")]
        input: Option<PathBuf>,
        /// Cor sólida em hexadecimal, por exemplo ff0000
        #[arg(long, value_parser = parse_rgb)]
        color: Option<[u8; 3]>,
        /// Por quantos segundos manter a imagem (a tela volta sozinha depois)
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Intervalo entre reenvios do quadro, em milissegundos (padrão: o
        /// que o modelo exige para manter a imagem)
        #[arg(long)]
        interval_ms: Option<u64>,
        /// Só prepara o quadro e mostra o que seria enviado, sem abrir o
        /// dispositivo
        #[arg(long)]
        dry_run: bool,
    },
    /// Ajusta brilho e/ou rotação da tela
    Set {
        /// Brilho de 0 a 100
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: Option<u8>,
        /// Rotação em graus
        #[arg(
            long,
            value_name = "GRAUS",
            value_parser = PossibleValuesParser::new(["0", "90", "180", "270"])
                .map(|s| s.parse::<u16>().unwrap())
        )]
        rotate: Option<u16>,
        /// Só mostra o que seria ajustado, sem abrir o dispositivo
        #[arg(long)]
        dry_run: bool,
    },
    /// Converte uma imagem para o JPEG que a tela espera (não envia nada)
    Prepare {
        /// Imagem de entrada (JPEG, PNG ou WebP)
        input: PathBuf,
        /// Arquivo JPEG de saída
        #[arg(short, long)]
        output: PathBuf,
        /// Largura em pixels (padrão: a do primeiro modelo suportado)
        #[arg(long, default_value_t = KNOWN_DEVICES[0].width)]
        width: u32,
        /// Altura em pixels (padrão: a do primeiro modelo suportado)
        #[arg(long, default_value_t = KNOWN_DEVICES[0].height)]
        height: u32,
        /// Qualidade JPEG (1–100)
        #[arg(long, default_value_t = imaging::DEFAULT_QUALITY)]
        quality: u8,
    },
    /// Imprime a regra udev necessária para acessar o dispositivo sem root
    UdevRules,
}

/// Abre a primeira tela conhecida e faz o handshake.
fn open_screen() -> Result<Box<dyn Screen>, Box<dyn std::error::Error>> {
    let mut screen = Bus::new()?.open_first()?;
    let s = screen.connect()?;
    println!("Conectado a {} ({})", screen.info().name, describe(&s));
    Ok(screen)
}

fn describe(s: &telinhas_core::ScreenStatus) -> String {
    let mut parts = Vec::new();
    if let Some(f) = &s.firmware {
        parts.push(format!("firmware {f}"));
    }
    if let Some(b) = s.brightness {
        parts.push(format!("brilho {b}"));
    }
    if let Some(r) = s.rotation {
        parts.push(format!("rotação {r}°"));
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

fn parse_rgb(s: &str) -> Result<[u8; 3], String> {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16)
        .ok()
        .filter(|_| s.len() == 6)
        .ok_or("use 6 dígitos hexadecimais, por exemplo ff0000")?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::UdevRules => {
            print!("{}", devices::udev_rules());
            eprintln!(
                "\nInstale com:\n  telinhas udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules\n  sudo udevadm control --reload && sudo udevadm trigger\nDepois reconecte o USB."
            );
        }
        Cmd::Probe { dry_run } => {
            let text = by::request("conn", by::DEFAULT_SEQ, "");
            let frame = by::encode_frame(&text)?;
            println!("Comando: {:?}", String::from_utf8_lossy(&text));
            println!("Quadro ({} bytes): {:02x?}", frame.len(), frame);
            if dry_run {
                return Ok(());
            }

            let (dev, t) = open_hid_raw()?;
            // Descarta relatórios antigos para não confundi-los com a resposta.
            while t.read_report(50)?.is_some() {}

            t.write_report(&frame)?;
            println!("Enviado a {}. Aguardando resposta (3s)...", dev.name);
            let end = Instant::now() + Duration::from_secs(3);
            while Instant::now() < end {
                let Some(r) = t.read_report(200)? else {
                    continue;
                };
                println!(
                    "Relatório ({} bytes): {:02x?}",
                    r.len(),
                    &r[..r.len().min(64)]
                );
                match by::decode_frame(&r).and_then(|p| by::Response::parse(&p)) {
                    Ok(resp) => {
                        println!("Status: {}", resp.status_line);
                        for (k, v) in &resp.headers {
                            println!("  {k} = {v}");
                        }
                        if !resp.body.is_empty() {
                            println!("Corpo: {}", resp.body);
                        }
                        return Ok(());
                    }
                    Err(e) => println!("  (não é uma resposta BY: {e})"),
                }
            }
            println!("Nenhuma resposta BY em 3s.");
        }
        Cmd::Show {
            input,
            color,
            seconds,
            interval_ms,
            dry_run,
        } => {
            // Sem dispositivo (--dry-run), prepara para o primeiro modelo.
            let mut screen = if dry_run { None } else { Some(open_screen()?) };
            let info = screen.as_ref().map_or(&KNOWN_DEVICES[0], |s| s.info());
            let FrameFormat::Jpeg { quality } = info.frame;
            let frame = match (&input, color) {
                (Some(path), _) => {
                    imaging::prepare_jpeg(&std::fs::read(path)?, info.width, info.height, quality)?
                }
                (None, Some(rgb)) => imaging::solid_jpeg(info.width, info.height, rgb, quality)?,
                (None, None) => unreachable!("clap exige imagem ou --color"),
            };
            println!(
                "Quadro JPEG {}x{} para {}: {} bytes",
                info.width,
                info.height,
                info.name,
                frame.len()
            );
            let Some(screen) = screen.as_mut() else {
                return Ok(());
            };

            let interval = interval_ms
                .map(Duration::from_millis)
                .or(info.keepalive)
                .unwrap_or(Duration::from_secs(2));
            let end = Instant::now() + Duration::from_secs(seconds);
            let mut n = 0u32;
            while Instant::now() < end {
                screen.show_frame(&frame)?;
                n += 1;
                println!("Quadro #{n} enviado.");
                std::thread::sleep(interval);
            }
            println!("Pronto; a tela volta à própria imagem após o timeout dela.");
        }
        Cmd::Set {
            brightness,
            rotate,
            dry_run,
        } => {
            if brightness.is_none() && rotate.is_none() {
                eprintln!("Nada a ajustar; use --brightness e/ou --rotate.");
                std::process::exit(2);
            }
            if dry_run {
                if let Some(v) = brightness {
                    println!("Ajustaria o brilho para {v}.");
                }
                if let Some(d) = rotate {
                    println!("Ajustaria a rotação para {d}°.");
                }
                return Ok(());
            }

            let mut screen = open_screen()?;
            if let Some(v) = brightness {
                screen.set_brightness(v)?;
                println!("Brilho ajustado para {v}.");
            }
            if let Some(d) = rotate {
                screen.set_rotation(d)?;
                println!("Rotação ajustada para {d}°.");
            }
            println!("Estado agora: {}", describe(&screen.connect()?));
        }
        Cmd::Prepare {
            input,
            output,
            width,
            height,
            quality,
        } => {
            let jpeg = imaging::prepare_jpeg(&std::fs::read(&input)?, width, height, quality)?;
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
                println!("Nenhuma tela conhecida encontrada.");
                println!("Modelos suportados:");
                for k in KNOWN_DEVICES {
                    println!("  {}  {}", usb_id(k), k.name);
                }
                return Ok(());
            }
            for k in found {
                print!("{}  {} ({}x{})  ", usb_id(k), k.name, k.width, k.height);
                match bus.open(k) {
                    Ok(_) => println!("[acesso OK]"),
                    Err(e) => {
                        println!("[sem acesso: {e}]");
                        println!("  dica: rode `telinhas udev-rules` e siga as instruções.");
                    }
                }
            }
        }
        Cmd::Listen { seconds } => {
            let (dev, t) = open_hid_raw()?;
            println!("Escutando {} por {seconds}s (somente leitura)...", dev.name);
            let end = Instant::now() + Duration::from_secs(seconds);
            let mut n = 0u32;
            while Instant::now() < end {
                if let Some(r) = t.read_report(200)? {
                    n += 1;
                    println!("#{n} {} bytes: {:02x?}", r.len(), &r[..r.len().min(64)]);
                }
            }
            println!("{n} relatório(s) recebido(s).");
        }
    }
    Ok(())
}
