//! Preparo de imagens para as telas: recorta, redimensiona e codifica em JPEG.
//!
//! Não fala com o hardware; serve tanto à CLI quanto à futura interface.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

use crate::Result;

/// Qualidade JPEG padrão (1–100).
pub const DEFAULT_QUALITY: u8 = 85;

/// Decodifica `input` (JPEG, PNG ou WebP), preenche `width`x`height`
/// recortando o excesso pelo centro e devolve um JPEG.
pub fn prepare_jpeg(input: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)?;
    let rgb = img
        .resize_to_fill(width, height, FilterType::Lanczos3)
        .to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)).encode_image(&rgb)?;
    Ok(out)
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
    fn rejeita_dados_que_nao_sao_imagem() {
        assert!(prepare_jpeg(b"isto nao e imagem", 720, 720, DEFAULT_QUALITY).is_err());
    }
}
