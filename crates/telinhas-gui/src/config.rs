//! Configuração persistida entre execuções e o atalho de início automático.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use telinhas_core::render::Position;
use telinhas_core::sensors::Metric;

use crate::app::APP_ID;

/// Nome do arquivo de fundo guardado na pasta de configuração (o caminho
/// original pode sumir; no Flatpak ele é temporário).
const BACKGROUND_STEM: &str = "background";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Arquivo de fundo dentro de [`dir`], por exemplo `background.png`.
    pub background_file: Option<String>,
    /// Nome original do arquivo, só para exibir.
    pub background_name: Option<String>,
    /// Se a imagem estava sendo mostrada na tela.
    pub showing: bool,
    /// Iniciar em segundo plano com a sessão.
    pub autostart: bool,
    /// Métricas desenhadas sobre a imagem.
    #[serde(deserialize_with = "known_metrics")]
    pub metrics: Vec<Metric>,
    /// Onde desenhar as métricas.
    pub metrics_position: Position,
}

/// Ignora métricas que deixaram de existir (por exemplo, "clock"), em vez de
/// descartar a configuração inteira.
fn known_metrics<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Metric>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// `$XDG_CONFIG_HOME/telinhas` (ou `~/.config/telinhas`).
pub fn dir() -> PathBuf {
    xdg_config_home().join("telinhas")
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

impl Config {
    /// Lê a configuração; arquivo ausente ou inválido vira a padrão.
    pub fn load() -> Self {
        std::fs::read(dir().join("config.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let dir = dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("config.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("config.json"))
    }

    pub fn background_path(&self) -> Option<PathBuf> {
        self.background_file.as_ref().map(|f| dir().join(f))
    }

    /// Guarda uma cópia do fundo escolhido e registra o nome dela.
    pub fn store_background(&mut self, bytes: &[u8], original: &Path) -> io::Result<()> {
        let ext = original
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "img".into());
        let file = format!("{BACKGROUND_STEM}.{ext}");
        let dir = dir();
        std::fs::create_dir_all(&dir)?;
        // Remove cópias antigas com outra extensão.
        if let Some(old) = self.background_file.as_ref().filter(|old| **old != file) {
            let _ = std::fs::remove_file(dir.join(old));
        }
        std::fs::write(dir.join(&file), bytes)?;
        self.background_file = Some(file);
        self.background_name = original
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        Ok(())
    }
}

/// Liga ou desliga o início automático com a sessão (XDG autostart).
pub fn set_autostart(enable: bool) -> io::Result<()> {
    let path = xdg_config_home()
        .join("autostart")
        .join(format!("{APP_ID}.desktop"));
    if !enable {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    // Na AppImage, o executável real fica num ponto de montagem temporário;
    // `$APPIMAGE` aponta para o arquivo que o usuário guardou.
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .map_or_else(std::env::current_exe, Ok)?;
    std::fs::create_dir_all(path.parent().expect("caminho tem pasta"))?;
    std::fs::write(
        &path,
        format!(
            "[Desktop Entry]\nType=Application\nName=Telinhas\n\
             Comment=Keeps the image on your mini screens\n\
             Exec=\"{}\" --background\nIcon={APP_ID}\nTerminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exe.display()
        ),
    )
}

/// Está rodando dentro do Flatpak?
pub fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// No Flatpak, o início automático é pedido ao portal Background (o app não
/// enxerga `~/.config/autostart`). Devolve se o pedido foi concedido.
pub async fn set_autostart_portal(enable: bool) -> Result<bool, String> {
    use ashpd::desktop::background::Background;
    let response = Background::request()
        .reason("Keep the image on your mini screens after you log in")
        .auto_start(enable)
        .command(["telinhas-gui", "--background"])
        .send()
        .await
        .and_then(|r| r.response())
        .map_err(|e| e.to_string())?;
    Ok(response.auto_start() == enable)
}
