//! Núcleo do Telinhas.
//!
//! Cada mini tela suportada vira um módulo em [`devices`], registrado em
//! [`devices::KNOWN_DEVICES`] e acessado só pela interface [`Screen`]. Veja
//! `docs/ADDING_A_DEVICE.md`.

pub mod devices;
pub mod error;
pub mod imaging;
pub mod protocol;
pub mod transport;

pub use error::{Error, Result};
pub use hidapi;

/// Estado de uma tela, no que o modelo souber informar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenStatus {
    pub firmware: Option<String>,
    pub brightness: Option<u8>,
    pub rotation: Option<u16>,
}

/// O que uma tela sabe fazer além de mostrar quadros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Aceita brilho de 0 a 100.
    pub brightness: bool,
    /// Rotações aceitas, em graus; vazio se não gira.
    pub rotations: &'static [u16],
}

/// Interface comum a todas as telas suportadas. A CLI e a interface gráfica
/// só usam esta interface, nunca o driver concreto.
pub trait Screen: Send {
    /// Informações estáticas do modelo (nome, resolução, formato de quadro).
    fn info(&self) -> &'static devices::DeviceInfo;

    fn capabilities(&self) -> Capabilities;

    /// Inicia a conversa com a tela e devolve o estado dela.
    fn connect(&mut self) -> Result<ScreenStatus>;

    /// Mostra um quadro no formato de [`devices::DeviceInfo::frame`], na
    /// resolução nativa. Telas com [`devices::DeviceInfo::keepalive`] precisam
    /// receber quadros nesse intervalo para manter a imagem.
    fn show_frame(&mut self, frame: &[u8]) -> Result<()>;

    /// Brilho de 0 a 100.
    fn set_brightness(&mut self, _value: u8) -> Result<()> {
        Err(Error::Unsupported("brilho"))
    }

    /// Rotação em graus, um dos valores de [`Capabilities::rotations`].
    fn set_rotation(&mut self, _degrees: u16) -> Result<()> {
        Err(Error::Unsupported("rotação"))
    }
}
