use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use cosmic::app::CosmicFlags;
use cosmic::dialog::file_chooser::{self, FileFilter};
use cosmic::iced::{window, Alignment, Length, Size, Subscription};
use cosmic::prelude::*;
use cosmic::widget::{self, image::Handle, settings};
use telinhas_core::devices::{self, DeviceInfo, KNOWN_DEVICES};
use telinhas_core::{imaging, Capabilities, ScreenStatus};

use crate::config::{self, Config};
use crate::worker::{self, Command, Event};

pub const APP_ID: &str = "io.github.ninjairocode.Telinhas";
const PREVIEW_SIZE: f32 = 360.0;
const WINDOW_SIZE: Size = Size::new(560.0, 820.0);
const WINDOW_MIN: Size = Size::new(420.0, 560.0);

/// Flags de inicialização; exigidas pela instância única do libcosmic.
#[derive(Debug, Clone, Default)]
pub struct Flags;

impl CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
}

enum Device {
    Searching,
    NoAccess(String),
    Connected {
        info: &'static DeviceInfo,
        caps: Capabilities,
        status: ScreenStatus,
        /// Rótulos do seletor de rotação ("90°"), na ordem de `caps.rotations`.
        rotation_labels: Vec<String>,
    },
}

struct Picture {
    name: String,
    preview: Handle,
    jpeg: Arc<Vec<u8>>,
}

pub struct App {
    core: cosmic::Core,
    cmds: Option<Sender<Command>>,
    device: Device,
    picture: Option<Picture>,
    loading: bool,
    showing: bool,
    /// O quadro atual já foi entregue ao worker.
    sent: bool,
    config: Config,
    /// Valor do slider enquanto arrasta; só é enviado ao soltar.
    brightness: u8,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Worker(Event),
    PickImage,
    Picked(PathBuf),
    PickCancelled,
    /// Imagem pronta; `source` traz o arquivo escolhido agora (para guardar
    /// uma cópia), ou `None` quando veio da configuração salva.
    Prepared {
        name: String,
        prepared: Arc<imaging::Prepared>,
        source: Option<(PathBuf, Arc<Vec<u8>>)>,
    },
    ToggleShow,
    ToggleAutostart(bool),
    WindowClosed(window::Id),
    Quit,
    BrightnessDrag(u8),
    BrightnessRelease,
    Rotation(usize),
    CopyUdevCommand,
    Error(String),
    DismissError,
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Flags) -> (Self, Task<cosmic::Action<Message>>) {
        let config = Config::load();
        let mut app = App {
            core,
            cmds: None,
            device: Device::Searching,
            picture: None,
            loading: false,
            showing: config.showing,
            sent: false,
            brightness: 100,
            error: None,
            config,
        };
        let mut tasks = Vec::new();
        if let Some(id) = app.core.main_window_id() {
            tasks.push(app.set_window_title("Telinhas".into(), id));
        }
        // Restaura o fundo salvo (e volta a mostrá-lo, se estava mostrando).
        if let Some(path) = app.config.background_path().filter(|p| p.exists()) {
            let name = app.config.background_name.clone().unwrap_or_default();
            tasks.push(app.load(path, name, false));
        } else {
            app.showing = false;
        }
        (app, Task::batch(tasks))
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        vec![widget::button::text("Sair").on_press(Message::Quit).into()]
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::WindowClosed(id))
    }

    /// Abrir o Telinhas de novo (pelo menu) com ele já rodando reabre a janela.
    fn dbus_activation(
        &mut self,
        _msg: cosmic::dbus_activation::Message,
    ) -> Task<cosmic::Action<Message>> {
        if self.core.main_window_id().is_some() {
            return Task::none();
        }
        let (id, open) = window::open(window::Settings {
            size: WINDOW_SIZE,
            min_size: Some(WINDOW_MIN),
            decorations: false,
            transparent: true,
            resizable: true,
            resize_border: 8,
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: APP_ID.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        self.core.set_main_window_id(Some(id));
        let title = self.set_window_title("Telinhas".into(), id);
        Task::batch([open.discard(), title])
    }

    fn subscription(&self) -> Subscription<Message> {
        worker::subscription().map(Message::Worker)
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Worker(event) => self.on_worker(event),
            Message::PickImage => {
                return cosmic::task::future(async {
                    let mut filter = FileFilter::new("Imagens");
                    for ext in ["jpg", "jpeg", "png", "webp", "JPG", "JPEG", "PNG", "WEBP"] {
                        filter = filter.glob(&format!("*.{ext}"));
                    }
                    let dialog = file_chooser::open::Dialog::new()
                        .title("Escolher imagem")
                        .filter(filter);
                    match dialog.open_file().await {
                        Ok(resp) => match resp.url().to_file_path() {
                            Ok(path) => Message::Picked(path),
                            Err(()) => {
                                Message::Error(format!("não é um arquivo local: {}", resp.url()))
                            }
                        },
                        Err(file_chooser::Error::Cancelled) => Message::PickCancelled,
                        Err(e) => Message::Error(format!("seletor de arquivos: {e}")),
                    }
                });
            }
            Message::PickCancelled => {}
            Message::Picked(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return self.load(path, name, true);
            }
            Message::Prepared {
                name,
                prepared,
                source,
            } => {
                self.loading = false;
                if let Some((path, bytes)) = source {
                    if let Err(e) = self.config.store_background(&bytes, &path) {
                        self.error = Some(format!("não foi possível guardar a imagem: {e}"));
                    }
                    self.save_config();
                }
                self.picture = Some(Picture {
                    name,
                    preview: Handle::from_rgba(
                        prepared.width,
                        prepared.height,
                        prepared.rgba.clone(),
                    ),
                    jpeg: Arc::new(prepared.jpeg.clone()),
                });
                // Imagem nova (ou restaurada): entrega ao worker se estiver mostrando.
                self.sent = false;
                self.sync_worker();
            }
            Message::ToggleShow => {
                self.showing = !self.showing && self.picture.is_some();
                self.sent = false;
                self.sync_worker();
                self.config.showing = self.showing;
                self.save_config();
            }
            Message::ToggleAutostart(enable) => match config::set_autostart(enable) {
                Ok(()) => {
                    self.config.autostart = enable;
                    self.save_config();
                }
                Err(e) => self.error = Some(format!("início automático: {e}")),
            },
            Message::WindowClosed(id) => {
                if self.core.main_window_id() == Some(id) {
                    self.core.set_main_window_id(None);
                }
            }
            Message::Quit => return cosmic::iced::exit(),
            Message::BrightnessDrag(v) => self.brightness = v,
            Message::BrightnessRelease => self.send(Command::SetBrightness(self.brightness)),
            Message::Rotation(i) => {
                if let Device::Connected { caps, .. } = &self.device {
                    if let Some(&d) = caps.rotations.get(i) {
                        self.send(Command::SetRotation(d));
                    }
                }
            }
            Message::CopyUdevCommand => {
                return cosmic::iced::clipboard::write(udev_command());
            }
            Message::Error(e) => {
                self.loading = false;
                self.error = Some(e);
            }
            Message::DismissError => self.error = None,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let connected = matches!(self.device, Device::Connected { .. });

        let mut content = widget::column::with_capacity(4)
            .spacing(spacing.space_m)
            .push(self.device_section());

        if let Some(e) = &self.error {
            content = content.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center)
                    .push(widget::text::body(e.as_str()).width(Length::Fill))
                    .push(widget::button::text("Fechar").on_press(Message::DismissError)),
            );
        }

        let preview: Element<_> = match &self.picture {
            Some(p) => widget::image(p.preview.clone())
                .width(PREVIEW_SIZE)
                .height(PREVIEW_SIZE)
                .into(),
            None => widget::container(widget::text::body(if self.loading {
                "Preparando…"
            } else {
                "Nenhuma imagem escolhida"
            }))
            .center(PREVIEW_SIZE)
            .class(cosmic::theme::Container::Card)
            .into(),
        };
        let show_label = if self.showing {
            "Parar"
        } else {
            "Mostrar na tela"
        };
        let image_section = settings::section()
            .title("Imagem")
            .add(
                widget::container(preview)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .add(
                settings::item::builder(
                    self.picture
                        .as_ref()
                        .map_or("Recortada para 720×720", |p| p.name.as_str()),
                )
                .control(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xs)
                        .push(
                            widget::button::standard("Escolher imagem…")
                                .on_press_maybe((!self.loading).then_some(Message::PickImage)),
                        )
                        .push(
                            widget::button::suggested(show_label).on_press_maybe(
                                (connected && (self.showing || self.picture.is_some()))
                                    .then_some(Message::ToggleShow),
                            ),
                        ),
                ),
            );
        content = content.push(image_section);

        if let Device::Connected {
            caps,
            status,
            rotation_labels,
            ..
        } = &self.device
        {
            let mut adjust = settings::section().title("Ajustes");
            if caps.brightness {
                adjust = adjust.add(
                    settings::item::builder(format!("Brilho: {}%", self.brightness)).control(
                        widget::slider(0..=100, self.brightness, Message::BrightnessDrag)
                            .on_release(Message::BrightnessRelease)
                            .width(220),
                    ),
                );
            }
            if !caps.rotations.is_empty() {
                let selected = status
                    .rotation
                    .and_then(|r| caps.rotations.iter().position(|&d| d == r));
                adjust = adjust.add(settings::item::builder("Rotação").control(widget::dropdown(
                    rotation_labels.as_slice(),
                    selected,
                    Message::Rotation,
                )));
            }
            if caps.brightness || !caps.rotations.is_empty() {
                content = content.push(adjust);
            }
        }

        content = content.push(
            settings::section()
                .title("Segundo plano")
                .add(
                    settings::item::builder("Iniciar com o sistema")
                        .description("Restaura a imagem na tela ao entrar na sessão")
                        .toggler(self.config.autostart, Message::ToggleAutostart),
                )
                .add(widget::text::caption(
                    "Fechar esta janela não para a tela: o Telinhas continua rodando. \
                     Abra-o pelo menu para ver a janela de novo; use \"Sair\" para encerrar.",
                )),
        );

        widget::scrollable(
            widget::container(content.max_width(640))
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(spacing.space_m),
        )
        .into()
    }
}

impl App {
    fn send(&mut self, cmd: Command) {
        if let Some(tx) = &self.cmds {
            if tx.send(cmd).is_err() {
                self.error = Some("a thread do dispositivo parou".into());
            }
        }
    }

    /// Prepara um arquivo de imagem fora da thread da interface.
    fn load(
        &mut self,
        path: PathBuf,
        name: String,
        keep_source: bool,
    ) -> Task<cosmic::Action<Message>> {
        self.loading = true;
        let info = self.screen_info();
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                let bytes = std::fs::read(&path)?;
                let prepared =
                    imaging::prepare(&bytes, info.width, info.height, jpeg_quality(info))?;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>((path, bytes, prepared))
            })
            .await;
            match result {
                Ok(Ok((path, bytes, prepared))) => Message::Prepared {
                    name,
                    prepared: Arc::new(prepared),
                    source: keep_source.then(|| (path, Arc::new(bytes))),
                },
                Ok(Err(e)) => Message::Error(format!("{name}: {e}")),
                Err(e) => Message::Error(e.to_string()),
            }
        })
    }

    /// Entrega ao worker o que a interface quer na tela (uma vez por mudança).
    fn sync_worker(&mut self) {
        if self.sent || self.cmds.is_none() {
            return;
        }
        let cmd = match (&self.picture, self.showing) {
            (Some(p), true) => Command::Show(p.jpeg.clone()),
            _ => Command::Stop,
        };
        self.send(cmd);
        self.sent = true;
    }

    fn save_config(&mut self) {
        if let Err(e) = self.config.save() {
            self.error = Some(format!("não foi possível salvar a configuração: {e}"));
        }
    }

    fn screen_info(&self) -> &'static DeviceInfo {
        match &self.device {
            Device::Connected { info, .. } => info,
            _ => &KNOWN_DEVICES[0],
        }
    }

    fn on_worker(&mut self, event: Event) {
        match event {
            Event::Ready(tx) => {
                self.cmds = Some(tx);
                self.sync_worker();
            }
            Event::Connected { info, caps, status } => {
                self.brightness = status.brightness.unwrap_or(self.brightness);
                let rotation_labels = caps.rotations.iter().map(|d| format!("{d}°")).collect();
                self.device = Device::Connected {
                    info,
                    caps,
                    status,
                    rotation_labels,
                };
            }
            Event::StatusChanged(new) => {
                self.brightness = new.brightness.unwrap_or(self.brightness);
                if let Device::Connected { status, .. } = &mut self.device {
                    *status = new;
                }
            }
            Event::NoAccess(why) => self.device = Device::NoAccess(why),
            Event::Disconnected(why) => {
                self.device = Device::Searching;
                self.error = Some(format!("Conexão perdida: {why}"));
            }
            Event::Error(e) => self.error = Some(e),
        }
    }

    fn device_section(&self) -> Element<'_, Message> {
        let section = settings::section().title("Tela");
        match &self.device {
            Device::Searching => section
                .add(
                    settings::item::builder("Procurando a tela…")
                        .control(widget::text::caption("Conecte o cabo USB do visor")),
                )
                .into(),
            Device::NoAccess(why) => section
                .add(
                    settings::item::builder("Sem permissão para acessar a tela").control(
                        widget::button::standard("Copiar comando")
                            .on_press(Message::CopyUdevCommand),
                    ),
                )
                .add(widget::text::body(format!(
                    "Rode no terminal o comando copiado (instala a regra udev) e reconecte o \
                     USB.\nDetalhe: {why}"
                )))
                .into(),
            Device::Connected { info, status, .. } => {
                let mut section = section.add(
                    settings::item::builder(info.name).control(widget::text::body("Conectada")),
                );
                if let Some(fw) = &status.firmware {
                    section = section.add(
                        settings::item::builder("Firmware")
                            .control(widget::text::body(fw.as_str())),
                    );
                }
                section.into()
            }
        }
    }
}

/// Comando de terminal que instala a regra udev e a recarrega.
fn udev_command() -> String {
    format!(
        "sudo tee /etc/udev/rules.d/99-telinhas.rules >/dev/null <<'EOF'\n{}EOF\n\
         sudo udevadm control --reload && sudo udevadm trigger\n",
        devices::udev_rules()
    )
}

fn jpeg_quality(info: &DeviceInfo) -> u8 {
    let devices::FrameFormat::Jpeg { quality } = info.frame;
    quality
}
