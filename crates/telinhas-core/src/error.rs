use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("erro de HID: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("dispositivo {vid:04x}:{pid:04x} não encontrado")]
    NotFound { vid: u16, pid: u16 },

    #[error("payload de {got} bytes excede o relatório HID de {max} bytes")]
    PayloadTooLarge { got: usize, max: usize },

    #[error("protocolo ainda não implementado: {0}")]
    ProtocolNotImplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
