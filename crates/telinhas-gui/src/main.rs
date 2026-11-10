mod app;
mod config;
mod worker;

use std::ffi::OsString;
use std::path::PathBuf;

/// Temas de ícones que têm os botões da janela (fechar, minimizar...), em
/// ordem de preferência. O libcosmic usa "Cosmic" por padrão, que só existe no
/// desktop COSMIC; sem ele, os botões aparecem vazios.
const ICON_THEMES: [&str; 3] = ["Cosmic", "Adwaita", "breeze"];

fn main() -> cosmic::iced::Result {
    // `--background`: inicia sem janela (início automático com a sessão).
    // Outros argumentos (`Telinhas.AppImage list`) vão para a CLI instalada ao
    // lado deste binário; sem argumentos, abre a janela.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let background = args.first().is_some_and(|a| a == "--background");
    if !args.is_empty() && !background {
        std::process::exit(run_cli(&args));
    }

    let mut settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(560.0, 820.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(420.0)
                .min_height(560.0),
        )
        // Fechar a janela não encerra: a imagem continua na tela.
        .exit_on_close(false)
        .no_main_window(background);
    if let Some(theme) = ICON_THEMES.into_iter().find(|t| icon_theme_installed(t)) {
        settings = settings.default_icon_theme(theme);
    }
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

/// O tema de ícones está instalado? Procura em `~/.icons` e em `icons/` dentro
/// dos diretórios de dados XDG, como fazem os desktops.
fn icon_theme_installed(name: &str) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let icon_dirs = data_home
        .into_iter()
        .chain(std::env::split_paths(&data_dirs))
        .map(|d| d.join("icons"))
        .chain(home.map(|h| h.join(".icons")));
    icon_dirs
        .map(|d| d.join(name).join("index.theme"))
        .any(|f| f.exists())
}
