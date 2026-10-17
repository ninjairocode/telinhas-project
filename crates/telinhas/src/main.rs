use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use telinhas_core::devices;
use telinhas_core::transport::HidTransport;

const UDEV_RULES: &str = include_str!("../../../packaging/99-telinhas.rules");

#[derive(Parser)]
#[command(name = "telinhas", version, about = "Controle mini telas de PC no Linux")]
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
    /// Imprime a regra udev necessária para acessar o dispositivo sem root
    UdevRules,
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
