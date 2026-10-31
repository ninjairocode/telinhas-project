//! Thermaltake TH360 V3 Ultra ARGB Sync (tela LCD 720x720).
//!
//! Fala o protocolo BY ([`crate::protocol::by`]), confirmado em hardware:
//! handshake, quadros JPEG, brilho e rotação. Veja `docs/PROTOCOL.md`.

use std::time::{Duration, Instant};

use super::{Bus, Connection, DeviceInfo, FrameFormat};
use crate::protocol::by;
use crate::transport::HidTransport;
use crate::{Capabilities, Error, Result, Screen, ScreenStatus};

pub const TH360_V3_ULTRA: DeviceInfo = DeviceInfo {
    name: "Thermaltake TH360 V3 Ultra ARGB Sync (LCD)",
    connection: Connection::UsbHid {
        vid: 0x264a,
        pid: 0x2348,
    },
    width: 720,
    height: 720,
    frame: FrameFormat::Jpeg { quality: 85 },
    // A tela volta à própria imagem após 5 s sem quadros (`timeout` do `conn`).
    keepalive: Some(Duration::from_secs(2)),
    open: open_hid,
};

const ROTATIONS: &[u16] = &[0, 90, 180, 270];

fn open_hid(bus: &Bus, info: &'static DeviceInfo) -> Result<Box<dyn Screen>> {
    let Connection::UsbHid { vid, pid } = info.connection;
    let transport = HidTransport::open(bus.hid(), vid, pid)?;
    Ok(Box::new(ThermaltakeLcd::new(transport, info)))
}

/// Quanto esperar pela resposta de um comando.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// Sem quadros por mais que isto, a tela pode ter saído do modo ao vivo
/// (`timeout` de 5 s no `conn`); religa antes do próximo quadro.
const REALTIME_STALE: Duration = Duration::from_secs(4);

pub struct ThermaltakeLcd {
    transport: HidTransport,
    info: &'static DeviceInfo,
    realtime: bool,
    last_frame: Option<Instant>,
    frame_counter: u8,
}

impl ThermaltakeLcd {
    pub fn new(transport: HidTransport, info: &'static DeviceInfo) -> Self {
        Self {
            transport,
            info,
            realtime: false,
            last_frame: None,
            frame_counter: 0,
        }
    }

    /// Envia `POST <command> 1` e espera a resposta `200`.
    pub fn command(&mut self, command: &str, json: &str) -> Result<by::Response> {
        // Descarta relatórios antigos para não confundi-los com a resposta.
        while self.transport.read_report(10)?.is_some() {}

        let frame = by::encode_frame(&by::request(command, by::DEFAULT_SEQ, json))?;
        self.transport.write_report(&frame)?;

        let end = Instant::now() + REPLY_TIMEOUT;
        while Instant::now() < end {
            let Some(r) = self.transport.read_report(200)? else {
                continue;
            };
            let Ok(text) = by::decode_frame(&r) else {
                continue;
            };
            let resp = by::Response::parse(&text)?;
            if resp.status != 200 {
                return Err(Error::BadResponse(resp.status_line));
            }
            return Ok(resp);
        }
        Err(Error::NoResponse(command.to_owned()))
    }

    /// Handshake com o estado completo do protocolo BY (inclui `timeout`).
    pub fn connect_by(&mut self) -> Result<by::Status> {
        // Um novo `conn` reinicia a sessão; o modo ao vivo precisa ser religado.
        self.realtime = false;
        by::Status::parse(&self.command("conn", "")?.body)
    }

    /// Liga ou desliga o modo de quadros ao vivo. Ligado, a tela volta à
    /// própria imagem se ficar `timeout` segundos sem receber quadros.
    pub fn set_realtime(&mut self, enable: bool) -> Result<()> {
        self.command("realtimeDisplay", &format!(r#"{{"enable": {enable}}}"#))?;
        self.realtime = enable;
        Ok(())
    }
}

impl Screen for ThermaltakeLcd {
    fn info(&self) -> &'static DeviceInfo {
        self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            brightness: true,
            rotations: ROTATIONS,
        }
    }

    fn connect(&mut self) -> Result<ScreenStatus> {
        let s = self.connect_by()?;
        Ok(ScreenStatus {
            firmware: Some(s.version.firmware).filter(|f| !f.is_empty()),
            brightness: Some(s.brightness),
            rotation: Some(s.degree),
        })
    }

    fn show_frame(&mut self, jpeg: &[u8]) -> Result<()> {
        let stale = self.last_frame.is_none_or(|t| t.elapsed() > REALTIME_STALE);
        if !self.realtime || stale {
            self.set_realtime(true)?;
        }
        for slice in by::image_slices(jpeg, self.frame_counter)? {
            self.transport.write_report(&slice)?;
        }
        self.frame_counter = (self.frame_counter + 1) % by::FRAME_COUNTER_WRAP;
        self.last_frame = Some(Instant::now());
        Ok(())
    }

    fn set_brightness(&mut self, value: u8) -> Result<()> {
        if value > 100 {
            return Err(Error::InvalidValue(format!(
                "brightness {value} (use 0–100)"
            )));
        }
        self.command("brightness", &format!(r#"{{"value": {value}}}"#))?;
        Ok(())
    }

    fn set_rotation(&mut self, degrees: u16) -> Result<()> {
        if !ROTATIONS.contains(&degrees) {
            return Err(Error::InvalidValue(format!(
                "rotation {degrees} (use 0, 90, 180 or 270)"
            )));
        }
        self.command("rotate", &format!(r#"{{"degree": {degrees}}}"#))?;
        Ok(())
    }
}
