//! Núcleo do Telinhas.
//!
//! Cada mini tela suportada vira um módulo em [`devices`]. Todos falam com o
//! hardware através de [`transport::HidTransport`].

pub mod devices;
pub mod error;
pub mod transport;

pub use error::{Error, Result};

/// Interface comum a todas as telas suportadas.
pub trait Display {
    /// Informações estáticas do modelo (nome, resolução, VID/PID).
    fn info(&self) -> &devices::DeviceInfo;

    /// Mostra uma imagem JPEG (na resolução nativa da tela) no visor.
    fn show_jpeg(&mut self, jpeg: &[u8]) -> Result<()>;
}
