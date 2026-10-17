//! Thread do dispositivo: dona do HID (bloqueante), conversa com a interface
//! por canais e mantém o quadro na tela reenviando-o antes do timeout dela.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cosmic::iced::futures::{self, SinkExt, Stream};
use cosmic::iced::{stream, Subscription};
use telinhas_core::devices::thermaltake::ThermaltakeLcd;
use telinhas_core::devices::{self, DeviceInfo};
use telinhas_core::hidapi::HidApi;
use telinhas_core::protocol::by::Status;
use telinhas_core::transport::HidTransport;
use telinhas_core::{Error, Screen};

/// Intervalo entre buscas pela tela quando ela não está conectada (ou
/// verificações de que continua conectada, quando ociosa).
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Intervalo entre reenvios do quadro; a tela desiste depois de 5 s.
const RESEND_INTERVAL: Duration = Duration::from_secs(2);

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
        info: DeviceInfo,
        status: Status,
    },
    /// A tela está conectada, mas não foi possível abri-la (regra udev).
    NoAccess(String),
    Disconnected(String),
    StatusChanged(Status),
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
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            let _ = ev.send(Event::Error(format!("não foi possível iniciar o HID: {e}")));
            return;
        }
    };
    let mut image: Option<Arc<Vec<u8>>> = None;
    let mut last_no_access = None;

    loop {
        let found = match devices::rescan(&mut api) {
            Ok(found) => found.first().copied().copied(),
            Err(e) => {
                let _ = ev.send(Event::Error(e.to_string()));
                None
            }
        };
        if let Some(info) = found {
            let opened = HidTransport::open(&api, info.vid, info.pid).map(ThermaltakeLcd::new);
            match opened {
                Ok(mut lcd) => {
                    last_no_access = None;
                    match session(&mut lcd, info, &mut api, &cmds, &ev, &mut image) {
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
    lcd: &mut ThermaltakeLcd,
    info: DeviceInfo,
    api: &mut HidApi,
    cmds: &Receiver<Command>,
    ev: &Events,
    image: &mut Option<Arc<Vec<u8>>>,
) -> End {
    match lcd.connect() {
        Ok(status) => {
            let _ = ev.send(Event::Connected { info, status });
        }
        Err(e) => return End::Lost(e.to_string()),
    }
    if image.is_some() {
        if let Err(e) = lcd.set_realtime(true) {
            return End::Lost(e.to_string());
        }
    }

    let mut next_frame = Instant::now();
    loop {
        if let Some(jpeg) = image.as_deref() {
            if Instant::now() >= next_frame {
                if let Err(e) = lcd.show_jpeg(jpeg) {
                    return End::Lost(e.to_string());
                }
                next_frame = Instant::now() + RESEND_INTERVAL;
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
                // A tela pode ter saído do modo ao vivo por timeout; religa.
                lcd.set_realtime(true)
            }
            Ok(Command::Stop) => {
                *image = None;
                Ok(())
            }
            Ok(Command::SetBrightness(v)) => {
                lcd.set_brightness(v).and_then(|()| report_status(lcd, ev))
            }
            Ok(Command::SetRotation(d)) => {
                lcd.set_rotation(d).and_then(|()| report_status(lcd, ev))
            }
            Err(RecvTimeoutError::Timeout) if image.is_none() => {
                // Ocioso: confere se a tela continua ali.
                match devices::rescan(api) {
                    Ok(found) if found.iter().any(|d| d.pid == info.pid) => Ok(()),
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
fn report_status(lcd: &mut ThermaltakeLcd, ev: &Events) -> telinhas_core::Result<()> {
    let status = lcd.connect()?;
    let _ = ev.send(Event::StatusChanged(status));
    Ok(())
}
