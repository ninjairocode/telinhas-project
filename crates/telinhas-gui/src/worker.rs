//! Thread do dispositivo: dona do HID (bloqueante), conversa com a interface
//! por canais e mantém o quadro na tela reenviando-o antes do timeout dela.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cosmic::iced::futures::{self, SinkExt, Stream};
use cosmic::iced::{stream, Subscription};
use telinhas_core::devices::{Bus, DeviceInfo};
use telinhas_core::{Capabilities, Error, Screen, ScreenStatus};

/// Intervalo entre buscas pela tela quando ela não está conectada (ou
/// verificações de que continua conectada, quando ociosa).
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Reenvio do quadro para telas que não pedem um intervalo próprio.
const DEFAULT_RESEND: Duration = Duration::from_secs(2);

/// Pedidos da interface para a thread.
#[derive(Debug, Clone)]
pub enum Command {
    /// Mostra este JPEG (já no tamanho da tela) até `Stop`.
    Show(Arc<Vec<u8>>),
    /// Para de enviar quadros; a tela volta à própria imagem sozinha.
    Stop,
    SetBrightness(u8),
    SetRotation(u16),
}

/// Notícias da thread para a interface.
#[derive(Debug, Clone)]
pub enum Event {
    /// A thread começou; use este canal para mandar comandos.
    Ready(Sender<Command>),
    Connected {
        info: &'static DeviceInfo,
        caps: Capabilities,
        status: ScreenStatus,
    },
    /// A tela está conectada, mas não foi possível abri-la (regra udev).
    NoAccess(String),
    Disconnected(String),
    StatusChanged(ScreenStatus),
    Error(String),
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(events)
}

fn events() -> impl Stream<Item = Event> {
    stream::channel(
        16,
        |mut out: futures::channel::mpsc::Sender<Event>| async move {
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel();
            std::thread::spawn(move || run(cmd_rx, ev_tx));
            let _ = out.send(Event::Ready(cmd_tx)).await;
            while let Some(ev) = ev_rx.recv().await {
                if out.send(ev).await.is_err() {
                    break;
                }
            }
        },
    )
}

type Events = tokio::sync::mpsc::UnboundedSender<Event>;

/// Por que uma sessão com a tela terminou.
enum End {
    /// A interface fechou o canal de comandos: encerrar a thread.
    Closed,
    /// A tela sumiu ou parou de responder: voltar a procurar.
    Lost(String),
}

fn run(cmds: Receiver<Command>, ev: Events) {
    let mut bus = match Bus::new() {
        Ok(bus) => bus,
        Err(e) => {
            let _ = ev.send(Event::Error(format!("não foi possível iniciar o HID: {e}")));
            return;
        }
    };
    let mut image: Option<Arc<Vec<u8>>> = None;
    let mut last_no_access = None;

    loop {
        let found = match bus.rescan() {
            Ok(found) => found.first().copied(),
            Err(e) => {
                let _ = ev.send(Event::Error(e.to_string()));
                None
            }
        };
        if let Some(info) = found {
            match bus.open(info) {
                Ok(mut screen) => {
                    last_no_access = None;
                    match session(screen.as_mut(), &mut bus, &cmds, &ev, &mut image) {
                        End::Closed => return,
                        End::Lost(why) => {
                            let _ = ev.send(Event::Disconnected(why));
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if last_no_access.as_ref() != Some(&msg) {
                        let _ = ev.send(Event::NoAccess(msg.clone()));
                        last_no_access = Some(msg);
                    }
                }
            }
        }
        // Enquanto procura, só guarda qual imagem mostrar quando a tela voltar.
        let deadline = Instant::now() + SCAN_INTERVAL;
        loop {
            match cmds.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Command::Show(jpeg)) => image = Some(jpeg),
                Ok(Command::Stop) => image = None,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

fn session(
    screen: &mut dyn Screen,
    bus: &mut Bus,
    cmds: &Receiver<Command>,
    ev: &Events,
    image: &mut Option<Arc<Vec<u8>>>,
) -> End {
    let info = screen.info();
    let resend = info.keepalive.unwrap_or(DEFAULT_RESEND);
    match screen.connect() {
        Ok(status) => {
            let caps = screen.capabilities();
            let _ = ev.send(Event::Connected { info, caps, status });
        }
        Err(e) => return End::Lost(e.to_string()),
    }

    let mut next_frame = Instant::now();
    loop {
        if let Some(jpeg) = image.as_deref() {
            if Instant::now() >= next_frame {
                if let Err(e) = screen.show_frame(jpeg) {
                    return End::Lost(e.to_string());
                }
                next_frame = Instant::now() + resend;
            }
        }
        let wait = if image.is_some() {
            next_frame.saturating_duration_since(Instant::now())
        } else {
            SCAN_INTERVAL
        };

        let result = match cmds.recv_timeout(wait) {
            Ok(Command::Show(jpeg)) => {
                *image = Some(jpeg);
                next_frame = Instant::now();
                Ok(())
            }
            Ok(Command::Stop) => {
                *image = None;
                Ok(())
            }
            Ok(Command::SetBrightness(v)) => screen
                .set_brightness(v)
                .and_then(|()| report_status(screen, ev)),
            Ok(Command::SetRotation(d)) => screen
                .set_rotation(d)
                .and_then(|()| report_status(screen, ev)),
            Err(RecvTimeoutError::Timeout) if image.is_none() => {
                // Ocioso: confere se a tela continua ali.
                match bus.rescan() {
                    Ok(found) if found.iter().any(|d| std::ptr::eq(*d, info)) => Ok(()),
                    Ok(_) => return End::Lost("tela desconectada".into()),
                    Err(e) => Err(e),
                }
            }
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => return End::Closed,
        };

        match result {
            Ok(()) => {}
            // Falha de transporte ou silêncio: a tela provavelmente sumiu.
            Err(e @ (Error::Hid(_) | Error::NoResponse(_))) => return End::Lost(e.to_string()),
            Err(e) => {
                let _ = ev.send(Event::Error(e.to_string()));
            }
        }
    }
}

/// Relê o estado (o `conn` devolve brilho e rotação gravados) e avisa a interface.
fn report_status(screen: &mut dyn Screen, ev: &Events) -> telinhas_core::Result<()> {
    let status = screen.connect()?;
    let _ = ev.send(Event::StatusChanged(status));
    Ok(())
}
