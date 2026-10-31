//! Desenha as métricas sobre um quadro: uma faixa semitransparente (no topo,
//! no centro ou embaixo), com até três métricas por linha (rótulo pequeno,
//! valor grande).

use std::sync::OnceLock;

use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use serde::{Deserialize, Serialize};

use crate::sensors::Reading;
use crate::Result;

/// Noto Sans Bold, licença SIL OFL 1.1 (`assets/fonts/OFL.txt`).
const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/NotoSans-Bold.ttf");

const MAX_PER_ROW: usize = 3;
/// Opacidade da faixa (0–255).
const BAND_ALPHA: u32 = 150;
const LABEL_COLOR: Rgba<u8> = Rgba([170, 200, 230, 255]);
const VALUE_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// Onde fica a faixa de métricas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    Top,
    /// No meio: a opção mais segura para telas redondas.
    Middle,
    #[default]
    Bottom,
}

impl Position {
    pub const ALL: [Position; 3] = [Position::Top, Position::Middle, Position::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Position::Top => "Top",
            Position::Middle => "Middle",
            Position::Bottom => "Bottom",
        }
    }
}

fn font() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(FONT_DATA).expect("fonte embutida válida"))
}

/// Desenha `readings` sobre `img`. Sem leituras, não altera nada.
///
/// A faixa fica afastada das bordas e as colunas, longe das laterais; em
/// telas redondas, [`Position::Middle`] evita cortes.
pub fn draw_overlay(img: &mut RgbaImage, readings: &[Reading], position: Position) {
    if readings.is_empty() {
        return;
    }
    let (w, h) = (img.width() as f32, img.height() as f32);
    let rows = readings.len().div_ceil(MAX_PER_ROW);
    let row_h = h * 0.15;
    let band_h = row_h * rows as f32;
    let band_top = match position {
        Position::Top => h * 0.10,
        Position::Middle => (h - band_h) / 2.0,
        Position::Bottom => h * 0.90 - band_h,
    };
    darken(img, band_top as u32, (band_top + band_h) as u32);

    let (left, right) = (w * 0.12, w * 0.88);
    let label_scale = PxScale::from(h * 0.036);
    let mut row_start = 0;
    for row in 0..rows {
        // Distribui igualmente: 5 métricas viram 3 + 2, não 3 + 2 vazias.
        let remaining_rows = rows - row;
        let count = (readings.len() - row_start).div_ceil(remaining_rows);
        let col_w = (right - left) / count as f32;
        let y = band_top + row_h * row as f32;
        for (i, r) in readings[row_start..row_start + count].iter().enumerate() {
            let center = left + col_w * (i as f32 + 0.5);
            draw_centered(
                img,
                r.metric.label(),
                center,
                y + row_h * 0.08,
                label_scale,
                col_w,
                LABEL_COLOR,
            );
            let value_scale = fit(&r.value, PxScale::from(h * 0.078), col_w * 0.92);
            draw_centered(
                img,
                &r.value,
                center,
                y + row_h * 0.36,
                value_scale,
                col_w,
                VALUE_COLOR,
            );
        }
        row_start += count;
    }
}

/// Como [`draw_overlay`], sobre pixels RGBA crus (`width * height * 4`
/// bytes); para prévias. Devolve vazio se o tamanho não bater.
pub fn overlay_rgba(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    readings: &[Reading],
    position: Position,
) -> Vec<u8> {
    match RgbaImage::from_raw(width, height, rgba) {
        Some(mut img) => {
            draw_overlay(&mut img, readings, position);
            img.into_raw()
        }
        None => Vec::new(),
    }
}

/// Decodifica o JPEG, desenha as métricas e codifica de novo.
pub fn compose_jpeg(
    base: &[u8],
    readings: &[Reading],
    position: Position,
    quality: u8,
) -> Result<Vec<u8>> {
    let mut img = image::load_from_memory_with_format(base, ImageFormat::Jpeg)?.to_rgba8();
    draw_overlay(&mut img, readings, position);
    crate::imaging::encode_rgb(&DynamicImage::ImageRgba8(img).to_rgb8(), quality)
}

fn darken(img: &mut RgbaImage, top: u32, bottom: u32) {
    for y in top..bottom.min(img.height()) {
        for x in 0..img.width() {
            let p = img.get_pixel_mut(x, y);
            for c in &mut p.0[..3] {
                *c = (u32::from(*c) * (255 - BAND_ALPHA) / 255) as u8;
            }
        }
    }
}

/// Reduz a escala até o texto caber em `max_w`.
fn fit(text: &str, mut scale: PxScale, max_w: f32) -> PxScale {
    let (tw, _) = text_size(scale, font(), text);
    if tw as f32 > max_w {
        let k = max_w / tw as f32;
        scale = PxScale::from(scale.y * k);
    }
    scale
}

fn draw_centered(
    img: &mut RgbaImage,
    text: &str,
    center_x: f32,
    top: f32,
    scale: PxScale,
    max_w: f32,
    color: Rgba<u8>,
) {
    let scale = fit(text, scale, max_w);
    let (tw, _) = text_size(scale, font(), text);
    let x = (center_x - tw as f32 / 2.0).max(0.0);
    draw_text_mut(img, color, x as i32, top as i32, scale, font(), text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::Metric;

    fn readings(n: usize) -> Vec<Reading> {
        Metric::ALL[..n]
            .iter()
            .map(|&metric| Reading {
                metric,
                value: "41 °C".into(),
            })
            .collect()
    }

    #[test]
    fn sem_leituras_nao_altera() {
        let mut img = RgbaImage::from_pixel(720, 720, Rgba([200, 10, 10, 255]));
        let before = img.clone();
        draw_overlay(&mut img, &[], Position::Bottom);
        assert_eq!(img, before);
    }

    #[test]
    fn desenha_a_faixa_embaixo_e_preserva_o_topo() {
        for n in 1..=Metric::ALL.len() {
            let mut img = RgbaImage::from_pixel(720, 720, Rgba([200, 200, 200, 255]));
            draw_overlay(&mut img, &readings(n), Position::Bottom);
            assert_eq!(img.get_pixel(360, 20), &Rgba([200, 200, 200, 255]));
            // Canto esquerdo da faixa (sem texto): escurecido.
            let band = img.get_pixel(10, 600);
            assert!(band.0[0] < 120, "{n} métricas: {band:?}");
            // Algum pixel de texto claro dentro da faixa.
            let has_text = (430..640).any(|y| (90..630).any(|x| img.get_pixel(x, y).0[0] > 240));
            assert!(has_text, "{n} métricas sem texto");
        }
    }

    #[test]
    fn posicao_move_a_faixa() {
        let gray = Rgba([200, 200, 200, 255]);
        for (pos, inside, outside) in [
            (Position::Top, 80, 600),
            (Position::Middle, 360, 600),
            (Position::Bottom, 600, 80),
        ] {
            let mut img = RgbaImage::from_pixel(720, 720, gray);
            draw_overlay(&mut img, &readings(2), pos);
            assert!(img.get_pixel(10, inside).0[0] < 120, "{pos:?}");
            assert_eq!(img.get_pixel(10, outside), &gray, "{pos:?}");
        }
    }

    #[test]
    fn compoe_jpeg_no_mesmo_tamanho() {
        let base = crate::imaging::solid_jpeg(720, 720, [20, 40, 80], 85).unwrap();
        let out = compose_jpeg(&base, &readings(3), Position::Middle, 85).unwrap();
        let img = image::load_from_memory_with_format(&out, ImageFormat::Jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (720, 720));
    }
}
