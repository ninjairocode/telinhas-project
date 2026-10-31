mod app;
mod config;
mod worker;

use std::ffi::OsString;

fn main() -> cosmic::iced::Result {
    // `--background`: inicia sem janela (início automático com a sessão).
    // Outros argumentos (`Telinhas.AppImage list`) vão para a CLI instalada ao
    // lado deste binário; sem argumentos, abre a janela.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let background = args.first().is_some_and(|a| a == "--background");
    if !args.is_empty() && !background {
        std::process::exit(run_cli(&args));
    }

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(560.0, 820.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(420.0)
                .min_height(560.0),
        )
        // Fechar a janela não encerra: a imagem continua na tela.
        .exit_on_close(false)
        .no_main_window(background);
    // Se já houver uma instância rodando, ela reabre a janela e esta termina.
    cosmic::app::run_single_instance::<app::App>(settings, app::Flags)
}

fn run_cli(args: &[OsString]) -> i32 {
    let cli = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("telinhas")))
        .filter(|p| p.exists());
    let Some(cli) = cli else {
        eprintln!("telinhas-gui takes no arguments; use the `telinhas` command.");
        return 2;
    };
    match std::process::Command::new(&cli).args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("could not run {}: {e}", cli.display());
            1
        }
    }
}
