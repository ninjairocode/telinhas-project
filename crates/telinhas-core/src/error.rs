use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("erro de HID: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("dispositivo {vid:04x}:{pid:04x} não encontrado")]
    NotFound { vid: u16, pid: u16 },

    #[error("{got} bytes excedem o limite de {max} bytes")]
    PayloadTooLarge { got: usize, max: usize },

    #[error("erro de imagem: {0}")]
    Image(#[from] image::ImageError),

    #[error("quadro inválido: {0}")]
    BadFrame(&'static str),

    #[error("resposta inesperada do dispositivo: {0:?}")]
    BadResponse(String),

    #[error("valor inválido: {0}")]
    InvalidValue(String),

    #[error("sem resposta do dispositivo ao comando `{0}`")]
    NoResponse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
