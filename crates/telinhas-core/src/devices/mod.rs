//! Telas suportadas. Para adicionar um modelo, crie um módulo com o driver
//! (que implementa [`crate::Screen`]) e registre seu [`DeviceInfo`] em
//! [`KNOWN_DEVICES`]. Veja `docs/ADDING_A_DEVICE.md`.

pub mod thermaltake;

use std::fmt::Write as _;
use std::time::Duration;

use hidapi::HidApi;

use crate::{Error, Result, Screen};

/// Formato em que a tela recebe cada quadro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    Jpeg { quality: u8 },
}

/// Como o dispositivo aparece no sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    /// HID por USB (`/dev/hidraw*`).
    UsbHid { vid: u16, pid: u16 },
}

/// Descrição estática de um modelo suportado.
#[derive(Debug, Clone, Copy)]
pub struct DeviceInfo {
    pub name: &'static str,
    pub connection: Connection,
    pub width: u32,
    pub height: u32,
    pub frame: FrameFormat,
    /// Intervalo máximo entre quadros para a imagem não sumir, se houver.
    pub keepalive: Option<Duration>,
    /// Abre o dispositivo e devolve o driver.
    pub open: fn(&Bus, &'static DeviceInfo) -> Result<Box<dyn Screen>>,
}

/// Todos os modelos que o Telinhas conhece.
pub static KNOWN_DEVICES: &[DeviceInfo] = &[thermaltake::TH360_V3_ULTRA];

/// Acesso aos barramentos por onde as telas aparecem. Hoje só HID; outros
/// transportes (serial, USB bulk) entram aqui. Mantenha uma só instância por
/// processo: o hidapi não aceita duas.
pub struct Bus {
    hid: HidApi,
}

impl Bus {
    pub fn new() -> Result<Self> {
        Ok(Self {
            hid: HidApi::new()?,
        })
    }

    pub fn hid(&self) -> &HidApi {
        &self.hid
    }

    /// Relê a lista de dispositivos e devolve os modelos conhecidos
    /// conectados. Programas de longa duração (a interface gráfica) chamam
    /// isto periodicamente para perceber telas conectadas ou desconectadas.
    pub fn rescan(&mut self) -> Result<Vec<&'static DeviceInfo>> {
        self.hid.refresh_devices()?;
        Ok(self.detect())
    }

    /// Modelos conhecidos na lista atual (sem reler os barramentos).
    pub fn detect(&self) -> Vec<&'static DeviceInfo> {
        KNOWN_DEVICES
            .iter()
            .filter(|k| self.is_present(k))
            .collect()
    }

    pub fn is_present(&self, info: &DeviceInfo) -> bool {
        match info.connection {
            Connection::UsbHid { vid, pid } => self
                .hid
                .device_list()
                .any(|d| d.vendor_id() == vid && d.product_id() == pid),
        }
    }

    pub fn open(&self, info: &'static DeviceInfo) -> Result<Box<dyn Screen>> {
        (info.open)(self, info)
    }

    /// Abre a primeira tela conhecida conectada.
    pub fn open_first(&self) -> Result<Box<dyn Screen>> {
        let info = self.detect().into_iter().next().ok_or(Error::NoDevice)?;
        self.open(info)
    }
}

/// Regras udev que dão ao usuário da sessão acesso a todos os modelos
/// conhecidos, sem root.
pub fn udev_rules() -> String {
    let mut out = String::from("# Telinhas: acesso sem root às mini telas suportadas.\n");
    for d in KNOWN_DEVICES {
        let _ = writeln!(out, "# {}", d.name);
        match d.connection {
            Connection::UsbHid { vid, pid } => {
                let _ = writeln!(
                    out,
                    r#"SUBSYSTEM=="hidraw", ATTRS{{idVendor}}=="{vid:04x}", ATTRS{{idProduct}}=="{pid:04x}", MODE="0660", TAG+="uaccess""#
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regra_udev_do_pacote_esta_atualizada() {
        let packaged = include_str!("../../../../packaging/99-telinhas.rules");
        assert_eq!(
            packaged,
            udev_rules(),
            "packaging/99-telinhas.rules difere de devices::udev_rules(); \
             atualize com `telinhas udev-rules > packaging/99-telinhas.rules`"
        );
    }
}
