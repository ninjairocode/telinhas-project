use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("device {vid:04x}:{pid:04x} not found")]
    NotFound { vid: u16, pid: u16 },

    #[error("{got} bytes exceed the {max}-byte limit")]
    PayloadTooLarge { got: usize, max: usize },

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("JPEG encoding error: {0}")]
    Jpeg(#[from] jpeg_encoder::EncodingError),

    #[error("invalid frame: {0}")]
    BadFrame(&'static str),

    #[error("unexpected reply from the device: {0:?}")]
    BadResponse(String),

    #[error("invalid value: {0}")]
    InvalidValue(String),

    #[error("no supported screen found")]
    NoDevice,

    #[error("this screen does not support {0}")]
    Unsupported(&'static str),

    #[error("no reply from the device to `{0}`")]
    NoResponse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
