mod app;
mod worker;

fn main() -> cosmic::iced::Result {
    // Com argumentos (`Telinhas.AppImage list`), repassa para a CLI instalada ao
    // lado deste binário; sem argumentos, abre a janela.
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() {
        std::process::exit(run_cli(&args));
    }

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(560.0, 820.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(420.0)
                .min_height(560.0),
        );
    cosmic::app::run::<app::App>(settings, ())
}

fn run_cli(args: &[std::ffi::OsString]) -> i32 {
    let cli = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("telinhas")))
        .filter(|p| p.exists());
    let Some(cli) = cli else {
        eprintln!("telinhas-gui não aceita argumentos; use o comando `telinhas`.");
        return 2;
    };
    match std::process::Command::new(&cli).args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("não foi possível executar {}: {e}", cli.display());
            1
        }
    }
}
