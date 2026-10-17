use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use cosmic::dialog::file_chooser::{self, FileFilter};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::prelude::*;
use cosmic::widget::{self, image::Handle, settings};
use telinhas_core::devices::{thermaltake, DeviceInfo};
use telinhas_core::imaging;
use telinhas_core::protocol::by::Status;

use crate::worker::{self, Command, Event};

const UDEV_RULES: &str = include_str!("../../../packaging/99-telinhas.rules");
const ROTATIONS: [&str; 4] = ["0°", "90°", "180°", "270°"];
const ROTATION_DEGREES: [u16; 4] = [0, 90, 180, 270];
const PREVIEW_SIZE: f32 = 360.0;

enum Device {
    Searching,
    NoAccess(String),
    Connected { info: DeviceInfo, status: Status },
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
    Prepared(String, Arc<imaging::Prepared>),
    ToggleShow,
    BrightnessDrag(u8),
    BrightnessRelease,
    Rotation(usize),
    CopyUdevCommand,
    Error(String),
    DismissError,
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.ninjairocode.Telinhas";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<cosmic::Action<Message>>) {
        let mut app = App {
            core,
            cmds: None,
            device: Device::Searching,
            picture: None,
            loading: false,
            showing: false,
            brightness: 100,
            error: None,
        };
        let task = match app.core.main_window_id() {
            Some(id) => app.set_window_title("Telinhas".into(), id),
            None => Task::none(),
        };
        (app, task)
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
                self.loading = true;
                let info = self.screen_info();
                return cosmic::task::future(async move {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let prepared = tokio::task::spawn_blocking(move || {
                        let bytes = std::fs::read(&path)?;
                        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(imaging::prepare(
                            &bytes,
                            info.width,
                            info.height,
                            imaging::DEFAULT_QUALITY,
                        )?)
                    })
                    .await;
                    match prepared {
                        Ok(Ok(p)) => Message::Prepared(name, Arc::new(p)),
                        Ok(Err(e)) => Message::Error(format!("{name}: {e}")),
                        Err(e) => Message::Error(e.to_string()),
                    }
                });
            }
            Message::Prepared(name, prepared) => {
                self.loading = false;
                let jpeg = Arc::new(prepared.jpeg.clone());
                self.picture = Some(Picture {
                    name,
                    preview: Handle::from_rgba(
                        prepared.width,
                        prepared.height,
                        prepared.rgba.clone(),
                    ),
                    jpeg: jpeg.clone(),
                });
                // Trocou a imagem enquanto mostrava: atualiza a tela.
                if self.showing {
                    self.send(Command::Show(jpeg));
                }
            }
            Message::ToggleShow => {
                if self.showing {
                    self.showing = false;
                    self.send(Command::Stop);
                } else if let Some(p) = &self.picture {
                    let jpeg = p.jpeg.clone();
                    self.showing = true;
                    self.send(Command::Show(jpeg));
                }
            }
            Message::BrightnessDrag(v) => self.brightness = v,
            Message::BrightnessRelease => self.send(Command::SetBrightness(self.brightness)),
            Message::Rotation(i) => self.send(Command::SetRotation(ROTATION_DEGREES[i])),
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

        if let Device::Connected { status, .. } = &self.device {
            let rotation = ROTATION_DEGREES.iter().position(|&d| d == status.degree);
            content = content.push(
                settings::section()
                    .title("Ajustes")
                    .add(
                        settings::item::builder(format!("Brilho: {}%", self.brightness)).control(
                            widget::slider(0..=100, self.brightness, Message::BrightnessDrag)
                                .on_release(Message::BrightnessRelease)
                                .width(220),
                        ),
                    )
                    .add(settings::item::builder("Rotação").control(widget::dropdown(
                        &ROTATIONS,
                        rotation,
                        Message::Rotation,
                    ))),
            );
        }

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

    fn screen_info(&self) -> DeviceInfo {
        match &self.device {
            Device::Connected { info, .. } => *info,
            _ => thermaltake::TH360_V3_ULTRA,
        }
    }

    fn on_worker(&mut self, event: Event) {
        match event {
            Event::Ready(tx) => self.cmds = Some(tx),
            Event::Connected { info, status } => {
                self.brightness = status.brightness;
                self.device = Device::Connected { info, status };
            }
            Event::StatusChanged(new) => {
                self.brightness = new.brightness;
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
            Device::Connected { info, status } => section
                .add(settings::item::builder(info.name).control(widget::text::body("Conectada")))
                .add(
                    settings::item::builder("Firmware")
                        .control(widget::text::body(status.version.firmware.as_str())),
                )
                .into(),
        }
    }
}

/// Comando de terminal que instala a regra udev e a recarrega.
fn udev_command() -> String {
    format!(
        "sudo tee /etc/udev/rules.d/99-telinhas.rules >/dev/null <<'EOF'\n{UDEV_RULES}EOF\n\
         sudo udevadm control --reload && sudo udevadm trigger\n"
    )
}
