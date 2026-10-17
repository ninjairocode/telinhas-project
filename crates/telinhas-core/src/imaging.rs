//! Preparo de imagens para as telas: recorta, redimensiona e codifica em JPEG.
//!
//! Não fala com o hardware; serve à CLI e à interface gráfica.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

use crate::Result;

/// Qualidade JPEG padrão (1–100).
pub const DEFAULT_QUALITY: u8 = 85;

/// Imagem pronta para a tela: o JPEG a enviar e os pixels RGBA para prévia.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub jpeg: Vec<u8>,
    /// `width * height * 4` bytes, RGBA, linha a linha.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decodifica `input` (JPEG, PNG ou WebP), preenche `width`x`height`
/// recortando o excesso pelo centro e devolve o JPEG e a prévia RGBA.
pub fn prepare(input: &[u8], width: u32, height: u32, quality: u8) -> Result<Prepared> {
    let img = image::load_from_memory(input)?.resize_to_fill(width, height, FilterType::Lanczos3);
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality.clamp(1, 100)).encode_image(&img.to_rgb8())?;
    Ok(Prepared {
        jpeg,
        rgba: img.to_rgba8().into_raw(),
        width,
        height,
    })
}

/// Como [`prepare`], mas só o JPEG.
pub fn prepare_jpeg(input: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    Ok(prepare(input, width, height, quality)?.jpeg)
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

    #[test]
    fn rejeita_dados_que_nao_sao_imagem() {
        assert!(prepare_jpeg(b"isto nao e imagem", 720, 720, DEFAULT_QUALITY).is_err());
    }
}
