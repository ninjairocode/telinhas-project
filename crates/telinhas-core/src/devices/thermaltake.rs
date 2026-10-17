//! Thermaltake TH360 V3 Ultra ARGB Sync (tela LCD 720x720).
//!
//! Status: o dispositivo é detectado e o transporte HID funciona, mas o
//! formato dos pacotes ainda NÃO foi recuperado. Veja `docs/PROTOCOL.md`.

use super::DeviceInfo;
use crate::transport::HidTransport;
use crate::{Display, Error, Result};

pub const TH360_V3_ULTRA: DeviceInfo = DeviceInfo {
    vid: 0x264a,
    pid: 0x2348,
    name: "Thermaltake TH360 V3 Ultra ARGB Sync (LCD)",
    width: 720,
    height: 720,
};

pub struct ThermaltakeLcd {
    #[allow(dead_code)]
    transport: HidTransport,
}

impl ThermaltakeLcd {
    pub fn new(transport: HidTransport) -> Self {
        Self { transport }
    }
}

impl Display for ThermaltakeLcd {
    fn info(&self) -> &DeviceInfo {
        &TH360_V3_ULTRA
    }

    fn show_jpeg(&mut self, _jpeg: &[u8]) -> Result<()> {
        // TODO: montar o quadro TLV + camada de arquivo (ver docs/PROTOCOL.md)
        // e enviar em relatórios de 1024 bytes via `self.transport`.
        Err(Error::ProtocolNotImplemented(
            "formato dos quadros da Thermaltake ainda não mapeado",
        ))
    }
}
