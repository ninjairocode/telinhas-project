use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use telinhas_core::devices::{self, thermaltake};
use telinhas_core::imaging;
use telinhas_core::protocol::by;
use telinhas_core::transport::HidTransport;
use telinhas_core::Screen;

const UDEV_RULES: &str = include_str!("../../../packaging/99-telinhas.rules");

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
    /// Envia só o handshake `conn` do protocolo BY e mostra a resposta
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
        /// Intervalo entre reenvios do quadro, em milissegundos
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,
        /// Só mostra os bytes que seriam enviados, sem abrir o dispositivo
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
        /// Largura em pixels (padrão: a da TH360 V3 Ultra)
        #[arg(long, default_value_t = thermaltake::TH360_V3_ULTRA.width)]
        width: u32,
        /// Altura em pixels (padrão: a da TH360 V3 Ultra)
        #[arg(long, default_value_t = thermaltake::TH360_V3_ULTRA.height)]
        height: u32,
        /// Qualidade JPEG (1–100)
        #[arg(long, default_value_t = imaging::DEFAULT_QUALITY)]
        quality: u8,
    },
    /// Imprime a regra udev necessária para acessar o dispositivo sem root
    UdevRules,
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
            print!("{UDEV_RULES}");
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

            let api = HidApi::new()?;
            let Some(dev) = devices::detect(&api).into_iter().next() else {
                eprintln!("Nenhuma tela conhecida encontrada.");
                std::process::exit(1);
            };
            let t = HidTransport::open(&api, dev.vid, dev.pid)?;
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
            let info = thermaltake::TH360_V3_ULTRA;
            let jpeg = match (&input, color) {
                (Some(path), _) => imaging::prepare_jpeg(
                    &std::fs::read(path)?,
                    info.width,
                    info.height,
                    imaging::DEFAULT_QUALITY,
                )?,
                (None, Some(rgb)) => {
                    imaging::solid_jpeg(info.width, info.height, rgb, imaging::DEFAULT_QUALITY)?
                }
                (None, None) => unreachable!("clap exige imagem ou --color"),
            };
            let slices = by::image_slices(&jpeg, 0)?;
            let realtime = by::encode_frame(&by::request(
                "realtimeDisplay",
                by::DEFAULT_SEQ,
                r#"{"enable": true}"#,
            ))?;
            println!(
                "Comando realtimeDisplay ({} bytes): {:02x?}",
                realtime.len(),
                realtime
            );
            println!(
                "JPEG {}x{}: {} bytes em {} fatia(s); cabeçalho da 1ª: {:02x?}",
                info.width,
                info.height,
                jpeg.len(),
                slices.len(),
                &slices[0][..by::SLICE_HEADER_LEN]
            );
            if dry_run {
                return Ok(());
            }

            let api = HidApi::new()?;
            let Some(dev) = devices::detect(&api).into_iter().next() else {
                eprintln!("Nenhuma tela conhecida encontrada.");
                std::process::exit(1);
            };
            let mut lcd =
                thermaltake::ThermaltakeLcd::new(HidTransport::open(&api, dev.vid, dev.pid)?);
            let hello = lcd.connect()?;
            println!("Conectado: {}", hello.body);
            let end = Instant::now() + Duration::from_secs(seconds);
            let mut n = 0u32;
            while Instant::now() < end {
                lcd.show_jpeg(&jpeg)?;
                n += 1;
                println!("Quadro #{n} enviado.");
                std::thread::sleep(Duration::from_millis(interval_ms));
            }
            println!("Pronto; a tela volta à própria imagem após o timeout dela.");
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
            let api = HidApi::new()?;
            let found = devices::detect(&api);
            if found.is_empty() {
                println!("Nenhuma tela conhecida encontrada.");
                println!("Modelos suportados:");
                for k in devices::KNOWN_DEVICES {
                    println!("  {:04x}:{:04x}  {}", k.vid, k.pid, k.name);
                }
                return Ok(());
            }
            for k in found {
                print!(
                    "{:04x}:{:04x}  {} ({}x{})  ",
                    k.vid, k.pid, k.name, k.width, k.height
                );
                match HidTransport::open(&api, k.vid, k.pid) {
                    Ok(_) => println!("[acesso OK]"),
                    Err(e) => {
                        println!("[sem acesso: {e}]");
                        println!("  dica: rode `telinhas udev-rules` e siga as instruções.");
                    }
                }
            }
        }
        Cmd::Listen { seconds } => {
            let api = HidApi::new()?;
            let found = devices::detect(&api);
            let Some(dev) = found.first() else {
                eprintln!("Nenhuma tela conhecida encontrada.");
                std::process::exit(1);
            };
            let t = HidTransport::open(&api, dev.vid, dev.pid)?;
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
