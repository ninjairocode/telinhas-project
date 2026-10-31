//! Preparo de imagens para as telas: recorta, redimensiona e codifica em JPEG.
//!
//! Não fala com o hardware; serve à CLI e à interface gráfica.

use std::io::Cursor;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, ImageFormat};

use crate::{Error, Result};

/// Qualidade JPEG padrão (1–100).
pub const DEFAULT_QUALITY: u8 = 85;

/// Menor intervalo entre quadros de uma animação: limita a taxa a 20 fps.
pub const MIN_FRAME_DELAY: Duration = Duration::from_millis(50);
/// GIFs com atraso menor que isto (inclusive 0) usam [`DEFAULT_GIF_DELAY`],
/// como fazem os navegadores.
const TINY_GIF_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_GIF_DELAY: Duration = Duration::from_millis(100);
/// Limite de quadros de uma animação (memória e tempo de preparo).
pub const MAX_FRAMES: usize = 500;

/// Um quadro pronto para a tela e por quanto tempo mostrá-lo.
#[derive(Debug, Clone)]
pub struct PreparedFrame {
    pub jpeg: Vec<u8>,
    /// `None` em imagens estáticas.
    pub delay: Option<Duration>,
}

/// Imagem ou animação pronta para a tela: os JPEGs a enviar e os pixels RGBA
/// do primeiro quadro, para prévia.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub frames: Vec<PreparedFrame>,
    /// `width * height * 4` bytes, RGBA, linha a linha.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Prepared {
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }
}

/// Decodifica `input` (JPEG, PNG, WebP ou GIF, animado ou não), preenche
/// `width`x`height` recortando o excesso pelo centro e codifica em JPEG.
pub fn prepare(input: &[u8], width: u32, height: u32, quality: u8) -> Result<Prepared> {
    let quality = quality.clamp(1, 100);
    if image::guess_format(input)? == ImageFormat::Gif {
        let frames = GifDecoder::new(Cursor::new(input))?
            .into_frames()
            .take(MAX_FRAMES + 1)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if frames.len() > MAX_FRAMES {
            return Err(Error::InvalidValue(format!(
                "GIF with more than {MAX_FRAMES} frames"
            )));
        }
        if frames.len() > 1 {
            return prepare_animation(frames, width, height, quality);
        }
    }
    let img = image::load_from_memory(input)?.resize_to_fill(width, height, FilterType::Lanczos3);
    Ok(Prepared {
        frames: vec![PreparedFrame {
            jpeg: encode(&img, quality)?,
            delay: None,
        }],
        rgba: img.to_rgba8().into_raw(),
        width,
        height,
    })
}

fn prepare_animation(
    frames: Vec<image::Frame>,
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Prepared> {
    let mut out = Vec::with_capacity(frames.len());
    let mut rgba = Vec::new();
    for (i, frame) in frames.into_iter().enumerate() {
        let delay = Duration::from(frame.delay());
        let delay = if delay < TINY_GIF_DELAY {
            DEFAULT_GIF_DELAY
        } else {
            delay
        };
        // Triangle é bem mais rápido que Lanczos3 e basta para animações.
        let img = DynamicImage::ImageRgba8(frame.into_buffer()).resize_to_fill(
            width,
            height,
            FilterType::Triangle,
        );
        if i == 0 {
            rgba = img.to_rgba8().into_raw();
        }
        out.push(PreparedFrame {
            jpeg: encode(&img, quality)?,
            delay: Some(delay.max(MIN_FRAME_DELAY)),
        });
    }
    Ok(Prepared {
        frames: out,
        rgba,
        width,
        height,
    })
}

fn encode(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality).encode_image(&img.to_rgb8())?;
    Ok(jpeg)
}

/// Como [`prepare`], mas só o JPEG do primeiro quadro.
pub fn prepare_jpeg(input: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    Ok(prepare(input, width, height, quality)?
        .frames
        .swap_remove(0)
        .jpeg)
}

/// Fundo de uma cor só, como [`Prepared`] (por exemplo, atrás das métricas
/// quando não há imagem escolhida).
pub fn solid(width: u32, height: u32, rgb: [u8; 3], quality: u8) -> Result<Prepared> {
    let img =
        image::RgbaImage::from_pixel(width, height, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    Ok(Prepared {
        frames: vec![PreparedFrame {
            jpeg: encode(
                &DynamicImage::ImageRgba8(img.clone()),
                quality.clamp(1, 100),
            )?,
            delay: None,
        }],
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// JPEG de uma cor só; útil para testar a tela.
pub fn solid_jpeg(width: u32, height: u32, rgb: [u8; 3], quality: u8) -> Result<Vec<u8>> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb(rgb));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)).encode_image(&img)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, Rgba([255, 0, 0, 128]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn preenche_a_resolucao_pedida() {
        let jpeg = prepare_jpeg(&png(1920, 1080), 720, 720, DEFAULT_QUALITY).unwrap();
        let out = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (720, 720));
    }

    #[test]
    fn previa_rgba_tem_o_tamanho_certo() {
        let p = prepare(&png(300, 500), 72, 72, DEFAULT_QUALITY).unwrap();
        assert_eq!((p.width, p.height), (72, 72));
        assert_eq!(p.rgba.len(), 72 * 72 * 4);
    }

    fn gif(frames: &[(u8, u16)]) -> Vec<u8> {
        use image::codecs::gif::{GifEncoder, Repeat};
        use image::{Delay, Frame};
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut out);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for &(shade, ms) in frames {
                let buf = RgbaImage::from_pixel(40, 30, Rgba([shade, 0, 0, 255]));
                let delay = Delay::from_numer_denom_ms(u32::from(ms), 1);
                enc.encode_frame(Frame::from_parts(buf, 0, 0, delay))
                    .unwrap();
            }
        }
        out
    }

    #[test]
    fn gif_animado_vira_varios_quadros() {
        let p = prepare(&gif(&[(255, 200), (0, 10), (128, 30)]), 72, 72, 80).unwrap();
        assert!(p.is_animated());
        let delays: Vec<_> = p.frames.iter().map(|f| f.delay.unwrap()).collect();
        assert_eq!(
            delays,
            [
                Duration::from_millis(200),
                DEFAULT_GIF_DELAY, // 10 ms é tratado como "sem atraso"
                MIN_FRAME_DELAY,   // 30 ms é limitado a 20 fps
            ]
        );
        assert_eq!(p.rgba.len(), 72 * 72 * 4);
    }

    #[test]
    fn gif_de_um_quadro_e_estatico() {
        let p = prepare(&gif(&[(255, 100)]), 72, 72, 80).unwrap();
        assert!(!p.is_animated());
        assert_eq!(p.frames[0].delay, None);
    }

    #[test]
    fn rejeita_dados_que_nao_sao_imagem() {
        assert!(prepare_jpeg(b"isto nao e imagem", 720, 720, DEFAULT_QUALITY).is_err());
    }
}
