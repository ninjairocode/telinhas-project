use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use cosmic::app::CosmicFlags;
use cosmic::dialog::file_chooser::{self, FileFilter};
use cosmic::iced::{window, Alignment, Length, Size, Subscription};
use cosmic::prelude::*;
use cosmic::widget::{self, image::Handle, settings};
use telinhas_core::devices::{self, DeviceInfo, KNOWN_DEVICES};
use telinhas_core::render::Position;
use telinhas_core::sensors::{Metric, Reading, Sensors};
use telinhas_core::{imaging, render, Capabilities, ScreenStatus};

use crate::config::{self, Config};
use crate::worker::{self, Command, Event};

pub const APP_ID: &str = "io.github.ninjairocode.Telinhas";
const PREVIEW_SIZE: f32 = 360.0;
const WINDOW_SIZE: Size = Size::new(560.0, 820.0);
const WINDOW_MIN: Size = Size::new(420.0, 560.0);
/// Fundo atrás das métricas quando nenhuma imagem foi escolhida.
const SOLID_BACKGROUND: [u8; 3] = [16, 24, 48];
/// Rótulos do seletor de posição, na ordem de `Position::ALL`.
const POSITION_LABELS: [&str; 3] = ["Top", "Middle", "Bottom"];

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

/// O que vai para a tela: quadros prontos e o primeiro em RGBA, base da prévia.
struct Picture {
    name: String,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    frames: Arc<Vec<imaging::PreparedFrame>>,
}

impl Picture {
    fn new(name: String, prepared: &imaging::Prepared) -> Self {
        Self {
            name,
            rgba: prepared.rgba.clone(),
            width: prepared.width,
            height: prepared.height,
            frames: Arc::new(prepared.frames.clone()),
        }
    }
}

pub struct App {
    core: cosmic::Core,
    cmds: Option<Sender<Command>>,
    device: Device,
    picture: Option<Picture>,
    /// Fundo sólido, usado quando só há métricas.
    solid: Option<Picture>,
    /// Prévia com as métricas desenhadas.
    preview: Option<Handle>,
    sensors: Sensors,
    available: Vec<Metric>,
    readings: Vec<Reading>,
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
    ToggleMetric(Metric, bool),
    MetricsPosition(usize),
    /// Relê as métricas para a prévia.
    Tick,
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
        let info = &KNOWN_DEVICES[0];
        let solid = imaging::solid(
            info.width,
            info.height,
            SOLID_BACKGROUND,
            jpeg_quality(info),
        )
        .ok()
        .map(|p| Picture::new(String::new(), &p));
        let mut sensors = Sensors::new();
        let available = sensors.available();
        let readings = sensors.sample(&config.metrics);
        let mut app = App {
            core,
            cmds: None,
            device: Device::Searching,
            picture: None,
            solid,
            preview: None,
            sensors,
            available,
            readings,
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
            app.showing &= app.shown().is_some();
        }
        app.refresh_preview();
        (app, Task::batch(tasks))
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        vec![widget::button::text("Quit").on_press(Message::Quit).into()]
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
        let mut subs = vec![worker::subscription().map(Message::Worker)];
        // Prévia ao vivo só com a janela aberta (o worker cuida da tela).
        if !self.config.metrics.is_empty() && self.core.main_window_id().is_some() {
            subs.push(cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Worker(event) => self.on_worker(event),
            Message::PickImage => {
                return cosmic::task::future(async {
                    let mut filter = FileFilter::new("Images");
                    for ext in [
                        "jpg", "jpeg", "png", "webp", "gif", "JPG", "JPEG", "PNG", "WEBP", "GIF",
                    ] {
                        filter = filter.glob(&format!("*.{ext}"));
                    }
                    let dialog = file_chooser::open::Dialog::new()
                        .title("Choose an image")
                        .filter(filter);
                    match dialog.open_file().await {
                        Ok(resp) => match resp.url().to_file_path() {
                            Ok(path) => Message::Picked(path),
                            Err(()) => Message::Error(format!("not a local file: {}", resp.url())),
                        },
                        Err(file_chooser::Error::Cancelled) => Message::PickCancelled,
                        Err(e) => Message::Error(format!("file chooser: {e}")),
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
                        self.error = Some(format!("could not store the image: {e}"));
                    }
                    self.save_config();
                }
                let name = if prepared.is_animated() {
                    format!("{name} (animated, {} frames)", prepared.frames.len())
                } else {
                    name
                };
                self.picture = Some(Picture::new(name, &prepared));
                self.refresh_preview();
                // Imagem nova (ou restaurada): entrega ao worker se estiver mostrando.
                self.sent = false;
                self.sync_worker();
            }
            Message::ToggleShow => {
                self.showing = !self.showing && self.shown().is_some();
                self.sent = false;
                self.sync_worker();
                self.config.showing = self.showing;
                self.save_config();
            }
            Message::ToggleMetric(metric, on) => {
                let mut metrics = std::mem::take(&mut self.config.metrics);
                metrics.retain(|m| *m != metric);
                if on {
                    metrics.push(metric);
                }
                // Mantém a ordem de Metric::ALL, independente da ordem dos cliques.
                self.config.metrics = Metric::ALL
                    .into_iter()
                    .filter(|m| metrics.contains(m))
                    .collect();
                self.send_overlay();
                // Sem imagem, o fundo sólido aparece ou some junto com as métricas.
                if self.picture.is_none() {
                    self.showing &= self.shown().is_some();
                    self.config.showing = self.showing;
                    self.sent = false;
                    self.sync_worker();
                }
                self.save_config();
                self.readings = self.sensors.sample(&self.config.metrics);
                self.refresh_preview();
            }
            Message::MetricsPosition(i) => {
                if let Some(&p) = Position::ALL.get(i) {
                    self.config.metrics_position = p;
                    self.send_overlay();
                    self.save_config();
                    self.refresh_preview();
                }
            }
            Message::Tick => {
                self.readings = self.sensors.sample(&self.config.metrics);
                self.refresh_preview();
            }
            Message::ToggleAutostart(enable) => match config::set_autostart(enable) {
                Ok(()) => {
                    self.config.autostart = enable;
                    self.save_config();
                }
                Err(e) => self.error = Some(format!("start with the system: {e}")),
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
                    .push(widget::button::text("Dismiss").on_press(Message::DismissError)),
            );
        }

        let preview: Element<_> = match &self.preview {
            Some(p) => widget::image(p.clone())
                .width(PREVIEW_SIZE)
                .height(PREVIEW_SIZE)
                .into(),
            None => widget::container(widget::text::body(if self.loading {
                "Preparing…"
            } else {
                "No image selected"
            }))
            .center(PREVIEW_SIZE)
            .class(cosmic::theme::Container::Card)
            .into(),
        };
        let show_label = if self.showing {
            "Stop"
        } else {
            "Show on screen"
        };
        let image_section = settings::section()
            .title("Image")
            .add(
                widget::container(preview)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .add(
                settings::item::builder(
                    self.picture
                        .as_ref()
                        .map_or("Image or GIF, cropped to the screen", |p| p.name.as_str()),
                )
                .control(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xs)
                        .push(
                            widget::button::standard("Choose image…")
                                .on_press_maybe((!self.loading).then_some(Message::PickImage)),
                        )
                        .push(
                            widget::button::suggested(show_label).on_press_maybe(
                                (connected && (self.showing || self.shown().is_some()))
                                    .then_some(Message::ToggleShow),
                            ),
                        ),
                ),
            );
        content = content.push(image_section);

        let mut metrics = settings::section().title("Metrics on screen");
        for &m in &self.available {
            metrics = metrics.add(
                settings::item::builder(m.description())
                    .checkbox(self.config.metrics.contains(&m), move |on| {
                        Message::ToggleMetric(m, on)
                    }),
            );
        }
        if !self.config.metrics.is_empty() {
            let selected = Position::ALL
                .iter()
                .position(|p| *p == self.config.metrics_position);
            metrics = metrics.add(
                settings::item::builder("Position").control(widget::dropdown(
                    &POSITION_LABELS,
                    selected,
                    Message::MetricsPosition,
                )),
            );
        }
        content = content.push(metrics);

        if let Device::Connected {
            caps,
            status,
            rotation_labels,
            ..
        } = &self.device
        {
            let mut adjust = settings::section().title("Adjustments");
            if caps.brightness {
                adjust = adjust.add(
                    settings::item::builder(format!("Brightness: {}%", self.brightness)).control(
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
                adjust = adjust.add(
                    settings::item::builder("Rotation").control(widget::dropdown(
                        rotation_labels.as_slice(),
                        selected,
                        Message::Rotation,
                    )),
                );
            }
            if caps.brightness || !caps.rotations.is_empty() {
                content = content.push(adjust);
            }
        }

        content = content.push(
            settings::section()
                .title("Background")
                .add(
                    settings::item::builder("Start with the system")
                        .description("Restores the image on the screen when you log in")
                        .toggler(self.config.autostart, Message::ToggleAutostart),
                )
                .add(widget::text::caption(
                    "Closing this window does not stop the screen: Telinhas keeps running. \
                     Open it from the menu to see the window again; use \"Quit\" to exit.",
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
                self.error = Some("the device thread stopped".into());
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

    /// O que iria para a tela: a imagem escolhida ou, só com métricas, o
    /// fundo sólido.
    fn shown(&self) -> Option<&Picture> {
        self.picture.as_ref().or(self
            .solid
            .as_ref()
            .filter(|_| !self.config.metrics.is_empty()))
    }

    /// Recompõe a prévia com as leituras atuais.
    fn refresh_preview(&mut self) {
        self.preview = self.shown().map(|p| {
            let rgba = render::overlay_rgba(
                p.rgba.clone(),
                p.width,
                p.height,
                &self.readings,
                self.config.metrics_position,
            );
            Handle::from_rgba(p.width, p.height, rgba)
        });
    }

    fn send_overlay(&mut self) {
        let cmd = Command::SetOverlay(self.config.metrics.clone(), self.config.metrics_position);
        self.send(cmd);
    }

    /// Entrega ao worker o que a interface quer na tela (uma vez por mudança).
    fn sync_worker(&mut self) {
        if self.sent || self.cmds.is_none() {
            return;
        }
        let cmd = match (self.shown(), self.showing) {
            (Some(p), true) => Command::Show(p.frames.clone()),
            _ => Command::Stop,
        };
        self.send(cmd);
        self.sent = true;
    }

    fn save_config(&mut self) {
        if let Err(e) = self.config.save() {
            self.error = Some(format!("could not save the settings: {e}"));
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
                self.send_overlay();
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
                self.error = Some(format!("Connection lost: {why}"));
            }
            Event::Error(e) => self.error = Some(e),
        }
    }

    fn device_section(&self) -> Element<'_, Message> {
        let section = settings::section().title("Screen");
        match &self.device {
            Device::Searching => section
                .add(
                    settings::item::builder("Looking for the screen…")
                        .control(widget::text::caption("Plug in the screen's USB cable")),
                )
                .into(),
            Device::NoAccess(why) => section
                .add(
                    settings::item::builder("No permission to access the screen").control(
                        widget::button::standard("Copy command").on_press(Message::CopyUdevCommand),
                    ),
                )
                .add(widget::text::body(format!(
                    "Run the copied command in a terminal (it installs the udev rule), then \
                     plug the USB cable back in.\nDetails: {why}"
                )))
                .into(),
            Device::Connected { info, status, .. } => {
                let mut section = section.add(
                    settings::item::builder(info.name).control(widget::text::body("Connected")),
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
