//! Thermaltake TH360 V3 Ultra ARGB Sync (tela LCD 720x720).
//!
//! Fala o protocolo BY ([`crate::protocol::by`]); o handshake `conn` foi
//! confirmado em hardware. Veja `docs/PROTOCOL.md`.

use std::time::{Duration, Instant};

use super::DeviceInfo;
use crate::protocol::by;
use crate::transport::HidTransport;
use crate::{Error, Result, Screen};

pub const TH360_V3_ULTRA: DeviceInfo = DeviceInfo {
    vid: 0x264a,
    pid: 0x2348,
    name: "Thermaltake TH360 V3 Ultra ARGB Sync (LCD)",
    width: 720,
    height: 720,
};

/// Quanto esperar pela resposta de um comando.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);

pub struct ThermaltakeLcd {
    transport: HidTransport,
    realtime: bool,
    frame_counter: u8,
}

impl ThermaltakeLcd {
    pub fn new(transport: HidTransport) -> Self {
        Self {
            transport,
            realtime: false,
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

    /// Handshake; a resposta traz versões, brilho, rotação e `timeout`.
    pub fn connect(&mut self) -> Result<by::Response> {
        self.command("conn", "")
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
    fn info(&self) -> &DeviceInfo {
        &TH360_V3_ULTRA
    }

    fn show_jpeg(&mut self, jpeg: &[u8]) -> Result<()> {
        if !self.realtime {
            self.set_realtime(true)?;
        }
        for slice in by::image_slices(jpeg, self.frame_counter)? {
            self.transport.write_report(&slice)?;
        }
        self.frame_counter = (self.frame_counter + 1) % by::FRAME_COUNTER_WRAP;
        Ok(())
    }
}
