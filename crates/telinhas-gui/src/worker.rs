//! Thread do dispositivo: dona do HID (bloqueante), conversa com a interface
//! por canais e mantém o quadro na tela reenviando-o antes do timeout dela.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cosmic::iced::futures::{self, SinkExt, Stream};
use cosmic::iced::{stream, Subscription};
use telinhas_core::devices::FrameFormat;
use telinhas_core::devices::{Bus, DeviceInfo};
use telinhas_core::imaging::PreparedFrame;
use telinhas_core::render::{self, Position};
use telinhas_core::sensors::{Metric, Reading, Sensors};
use telinhas_core::{Capabilities, Error, Screen, ScreenStatus};

/// Intervalo entre buscas pela tela quando ela não está conectada (ou
/// verificações de que continua conectada, quando ociosa).
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Reenvio do quadro para telas que não pedem um intervalo próprio.
const DEFAULT_RESEND: Duration = Duration::from_secs(2);
/// A cada quanto tempo reler as métricas.
const METRICS_INTERVAL: Duration = Duration::from_secs(1);

/// Métricas escolhidas e a última leitura.
struct Overlay {
    sensors: Sensors,
    metrics: Vec<Metric>,
    position: Position,
    readings: Vec<Reading>,
    next_sample: Instant,
}

impl Overlay {
    fn new() -> Self {
        Self {
            sensors: Sensors::new(),
            metrics: Vec::new(),
            position: Position::default(),
            readings: Vec::new(),
            next_sample: Instant::now(),
        }
    }

    fn set(&mut self, metrics: Vec<Metric>, position: Position) {
        self.metrics = metrics;
        self.position = position;
        if self.metrics.is_empty() {
            self.readings.clear();
        }
        self.next_sample = Instant::now();
    }

    /// Relê as métricas se chegou a hora.
    fn refresh(&mut self) {
        if !self.metrics.is_empty() && Instant::now() >= self.next_sample {
            self.readings = self.sensors.sample(&self.metrics);
            self.next_sample = Instant::now() + METRICS_INTERVAL;
        }
    }

    /// Próxima releitura, se houver métricas.
    fn due(&self) -> Option<Instant> {
        (!self.metrics.is_empty()).then_some(self.next_sample)
    }
}

/// Pedidos da interface para a thread.
#[derive(Debug, Clone)]
pub enum Command {
    /// Mostra estes quadros (já no formato da tela) até `Stop`; com mais de
    /// um quadro, toca em loop respeitando o atraso de cada um.
    Show(Arc<Vec<PreparedFrame>>),
    /// Para de enviar quadros; a tela volta à própria imagem sozinha.
    Stop,
    SetBrightness(u8),
    SetRotation(u16),
    /// Métricas desenhadas sobre os quadros (vazio: nenhuma) e onde.
    SetOverlay(Vec<Metric>, Position),
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
            let _ = ev.send(Event::Error(format!("could not start HID: {e}")));
            return;
        }
    };
    let mut image: Option<Arc<Vec<PreparedFrame>>> = None;
    let mut overlay = Overlay::new();
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
                    let ctx = Session {
                        bus: &mut bus,
                        cmds: &cmds,
                        ev: &ev,
                        image: &mut image,
                        overlay: &mut overlay,
                    };
                    match session(screen.as_mut(), ctx) {
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
                Ok(Command::SetOverlay(m, p)) => overlay.set(m, p),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// Estado que sobrevive às reconexões, emprestado a cada sessão.
struct Session<'a> {
    bus: &'a mut Bus,
    cmds: &'a Receiver<Command>,
    ev: &'a Events,
    image: &'a mut Option<Arc<Vec<PreparedFrame>>>,
    overlay: &'a mut Overlay,
}

fn session(screen: &mut dyn Screen, ctx: Session) -> End {
    let Session {
        bus,
        cmds,
        ev,
        image,
        overlay,
    } = ctx;
    let info = screen.info();
    let FrameFormat::Jpeg { quality } = info.frame;
    let resend = info.keepalive.unwrap_or(DEFAULT_RESEND);
    match screen.connect() {
        Ok(status) => {
            let caps = screen.capabilities();
            let _ = ev.send(Event::Connected { info, caps, status });
        }
        Err(e) => return End::Lost(e.to_string()),
    }

    let mut player = Player::default();
    let mut next_frame = Instant::now();
    loop {
        if let Some(frames) = image.as_deref() {
            if Instant::now() >= next_frame {
                let frame = player.current(frames);
                let started = Instant::now();
                overlay.refresh();
                let sent = if overlay.readings.is_empty() {
                    screen.show_frame(&frame.jpeg)
                } else {
                    render::compose_jpeg(&frame.jpeg, &overlay.readings, overlay.position, quality)
                        .and_then(|jpeg| screen.show_frame(&jpeg))
                };
                if let Err(e) = sent {
                    return End::Lost(e.to_string());
                }
                player.sent(started.elapsed());
                next_frame = player.next_send(frames, resend);
                if let Some(due) = overlay.due() {
                    next_frame = next_frame.min(due);
                }
            }
        }
        let wait = if image.is_some() {
            next_frame.saturating_duration_since(Instant::now())
        } else {
            SCAN_INTERVAL
        };

        let result = match cmds.recv_timeout(wait) {
            Ok(Command::Show(frames)) => {
                *image = Some(frames);
                player = Player::default();
                next_frame = Instant::now();
                Ok(())
            }
            Ok(Command::Stop) => {
                *image = None;
                Ok(())
            }
            Ok(Command::SetOverlay(m, p)) => {
                overlay.set(m, p);
                next_frame = Instant::now();
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
                    Ok(_) => return End::Lost("screen unplugged".into()),
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

/// Avança os quadros de uma animação e mede a taxa real de envio.
struct Player {
    index: usize,
    /// Quando trocar para o próximo quadro (`None`: ainda não mostrou nenhum).
    advance_at: Option<Instant>,
    stats_since: Instant,
    stats_frames: u32,
    stats_busy: Duration,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            index: 0,
            advance_at: None,
            stats_since: Instant::now(),
            stats_frames: 0,
            stats_busy: Duration::ZERO,
        }
    }
}

/// A cada quanto tempo registrar a taxa de quadros (no terminal).
const STATS_INTERVAL: Duration = Duration::from_secs(10);

impl Player {
    /// O quadro a enviar agora, avançando a animação se chegou a hora.
    fn current<'a>(&mut self, frames: &'a [PreparedFrame]) -> &'a PreparedFrame {
        let now = Instant::now();
        match self.advance_at {
            None => self.advance_at = Some(now + frames[0].delay.unwrap_or(Duration::MAX / 4)),
            Some(at) if now >= at && frames.len() > 1 => {
                self.index = (self.index + 1) % frames.len();
                let delay = frames[self.index].delay.unwrap_or_default();
                // Atrasou mais de um quadro: recomeça a contagem a partir de
                // agora, em vez de acelerar para recuperar o tempo perdido.
                let base = if now - at > delay { now } else { at };
                self.advance_at = Some(base + delay);
            }
            Some(_) => {}
        }
        &frames[self.index]
    }

    /// Próximo envio: a troca de quadro ou, se demorar, o reenvio que mantém
    /// a imagem na tela.
    fn next_send(&self, frames: &[PreparedFrame], resend: Duration) -> Instant {
        let keepalive = Instant::now() + resend;
        match self.advance_at {
            Some(at) if frames.len() > 1 => at.min(keepalive),
            _ => keepalive,
        }
    }

    fn sent(&mut self, took: Duration) {
        self.stats_frames += 1;
        self.stats_busy += took;
        let elapsed = self.stats_since.elapsed();
        if elapsed >= STATS_INTERVAL {
            let fps = f64::from(self.stats_frames) / elapsed.as_secs_f64();
            let avg = self.stats_busy / self.stats_frames;
            eprintln!(
                "telinhas: {fps:.1} frames/s; {:.1} ms average send time per frame",
                avg.as_secs_f64() * 1000.0
            );
            self.stats_since = Instant::now();
            self.stats_frames = 0;
            self.stats_busy = Duration::ZERO;
        }
    }
}
