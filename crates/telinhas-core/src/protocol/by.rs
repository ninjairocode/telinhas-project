//! Protocolo "BY" das telas Thermaltake com controlador Linux embarcado.
//!
//! Documentado publicamente para o painel 6" (`264a:2347`); o app oficial da
//! TH360 V3 Ultra (`264a:2348`) usa o mesmo vocabulário. Implementação própria,
//! a partir da descrição do protocolo.
//!
//! Comandos são texto no estilo HTTP:
//!
//! ```text
//! POST conn 1\r\n
//! SeqNumber=112\r\n
//! ContentType=json\r\n
//! ContentLength=0\r\n
//! \r\n
//! <corpo JSON>
//! ```
//!
//! e viajam num quadro `5A | tamanho (u16 BE) | texto | soma8 | 5A`, onde o
//! tamanho é o do quadro inteiro sem escape (texto + 5), a soma8 cobre os bytes
//! do tamanho e do texto, e entre os delimitadores `5A` vira `5B 01` e `5B`
//! vira `5B 02`. Cada quadro cabe num único relatório HID.

use crate::transport::REPORT_LEN;
use crate::{Error, Result};

const FLAG: u8 = 0x5A;
const ESC: u8 = 0x5B;

/// SeqNumber que o software oficial usa nesses painéis.
pub const DEFAULT_SEQ: u32 = 112;

/// Monta o quadro com delimitadores, tamanho, soma8 e escape.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>> {
    let len = u16::try_from(payload.len() + 5).map_err(|_| Error::PayloadTooLarge {
        got: payload.len(),
        max: REPORT_LEN,
    })?;
    let mut raw = Vec::with_capacity(payload.len() + 3);
    raw.extend_from_slice(&len.to_be_bytes());
    raw.extend_from_slice(payload);
    raw.push(raw.iter().fold(0u8, |s, &b| s.wrapping_add(b)));

    let mut frame = vec![FLAG];
    for b in raw {
        match b {
            FLAG => frame.extend_from_slice(&[ESC, 0x01]),
            ESC => frame.extend_from_slice(&[ESC, 0x02]),
            _ => frame.push(b),
        }
    }
    frame.push(FLAG);
    if frame.len() > REPORT_LEN {
        return Err(Error::PayloadTooLarge {
            got: frame.len(),
            max: REPORT_LEN,
        });
    }
    Ok(frame)
}

/// Extrai o texto de um quadro: procura os delimitadores (ignora o
/// preenchimento do relatório), desfaz o escape e confere tamanho e soma8.
pub fn decode_frame(buf: &[u8]) -> Result<Vec<u8>> {
    let start = buf
        .iter()
        .position(|&b| b == FLAG)
        .ok_or(Error::BadFrame("delimitador inicial ausente"))?;
    let end = buf[start + 1..]
        .iter()
        .position(|&b| b == FLAG)
        .map(|i| start + 1 + i)
        .ok_or(Error::BadFrame("delimitador final ausente"))?;

    let mut raw = Vec::with_capacity(end - start);
    let mut it = buf[start + 1..end].iter();
    while let Some(&b) = it.next() {
        raw.push(if b == ESC {
            match it.next() {
                Some(0x01) => FLAG,
                Some(0x02) => ESC,
                _ => return Err(Error::BadFrame("escape inválido")),
            }
        } else {
            b
        });
    }

    if raw.len() < 3 {
        return Err(Error::BadFrame("quadro curto demais"));
    }
    let (body, sum) = raw.split_at(raw.len() - 1);
    if body.iter().fold(0u8, |s, &b| s.wrapping_add(b)) != sum[0] {
        return Err(Error::BadFrame("soma de verificação não confere"));
    }
    if usize::from(u16::from_be_bytes([body[0], body[1]])) != raw.len() + 2 {
        return Err(Error::BadFrame("campo de tamanho não confere"));
    }
    Ok(body[2..].to_vec())
}

/// Texto de um comando `POST <command> 1` com corpo JSON (pode ser vazio).
pub fn request(command: &str, seq: u32, json: &str) -> Vec<u8> {
    format!(
        "POST {command} 1\r\nSeqNumber={seq}\r\nContentType=json\r\nContentLength={}\r\n\r\n{json}",
        json.len()
    )
    .into_bytes()
}

/// Resposta do painel: linha de status, cabeçalhos `Chave=valor` e corpo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status_line: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn parse(text: &[u8]) -> Result<Self> {
        let text = String::from_utf8_lossy(text);
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or_default().to_owned();
        let status = status_line
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::BadResponse(status_line.clone()))?;
        let headers = lines
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Ok(Self {
            status_line,
            status,
            headers,
            body: body.trim_end_matches('\0').to_owned(),
        })
    }

    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Tamanho do cabeçalho de cada fatia de imagem.
pub const SLICE_HEADER_LEN: usize = 24;
/// Bytes de JPEG por fatia.
pub const SLICE_DATA_LEN: usize = 1000;
/// Maior JPEG aceito pelo painel.
pub const MAX_JPEG_LEN: usize = 0xC7FFF;
/// O contador de quadros volta a zero depois de 59.
pub const FRAME_COUNTER_WRAP: u8 = 60;

/// Divide um JPEG em relatórios de imagem (modo `realtimeDisplay`). Estes não
/// usam o quadro `5A`: cada relatório é
/// `5C 03 FD | contador | total u16 BE | índice u16 BE | 01 | zeros até o
/// byte 23 | 1000 bytes de JPEG`, com a última fatia completada com zeros.
pub fn image_slices(jpeg: &[u8], counter: u8) -> Result<Vec<[u8; REPORT_LEN]>> {
    if jpeg.is_empty() {
        return Err(Error::BadFrame("JPEG vazio"));
    }
    if jpeg.len() > MAX_JPEG_LEN {
        return Err(Error::PayloadTooLarge {
            got: jpeg.len(),
            max: MAX_JPEG_LEN,
        });
    }
    let chunks = jpeg.chunks(SLICE_DATA_LEN);
    // MAX_JPEG_LEN / SLICE_DATA_LEN < u16::MAX
    let total = chunks.len() as u16;
    Ok(chunks
        .enumerate()
        .map(|(i, data)| {
            let mut r = [0u8; REPORT_LEN];
            r[..3].copy_from_slice(&[0x5C, 0x03, 0xFD]);
            r[3] = counter % FRAME_COUNTER_WRAP;
            r[4..6].copy_from_slice(&total.to_be_bytes());
            r[6..8].copy_from_slice(&(i as u16).to_be_bytes());
            r[8] = 0x01;
            r[SLICE_HEADER_LEN..SLICE_HEADER_LEN + data.len()].copy_from_slice(data);
            r
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatias_de_imagem() {
        let jpeg: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        let s = image_slices(&jpeg, 61).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(&s[0][..9], &[0x5C, 0x03, 0xFD, 1, 0, 3, 0, 0, 1]);
        assert_eq!(&s[2][6..8], &[0, 2]);
        assert!(s[1][9..SLICE_HEADER_LEN].iter().all(|&b| b == 0));
        assert_eq!(&s[1][SLICE_HEADER_LEN..], &jpeg[1000..2000]);
        // última fatia: 500 bytes de dados e o resto zerado
        assert_eq!(
            &s[2][SLICE_HEADER_LEN..SLICE_HEADER_LEN + 500],
            &jpeg[2000..]
        );
        assert!(s[2][SLICE_HEADER_LEN + 500..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rejeita_jpeg_grande_demais() {
        assert!(image_slices(&vec![0; MAX_JPEG_LEN + 1], 0).is_err());
    }

    #[test]
    fn conn_tem_o_texto_esperado() {
        assert_eq!(
            request("conn", DEFAULT_SEQ, ""),
            b"POST conn 1\r\nSeqNumber=112\r\nContentType=json\r\nContentLength=0\r\n\r\n"
        );
    }

    #[test]
    fn quadro_simples() {
        // tamanho = 3 + 5 = 8; soma = (0x00 + 0x08 + 'a' + 'b' + 'c') % 256 = 0x2E
        let f = encode_frame(b"abc").unwrap();
        assert_eq!(f, [0x5A, 0x00, 0x08, b'a', b'b', b'c', 0x2E, 0x5A]);
    }

    #[test]
    fn escape_de_delimitadores() {
        let f = encode_frame(&[0x5A, 0x5B]).unwrap();
        assert_eq!(&f[3..7], &[0x5B, 0x01, 0x5B, 0x02]);
        assert_eq!(f.iter().filter(|&&b| b == 0x5A).count(), 2);
    }

    #[test]
    fn ida_e_volta_com_preenchimento() {
        for payload in [
            request("conn", DEFAULT_SEQ, ""),
            request("brightness", DEFAULT_SEQ, r#"{"value": 90}"#),
            vec![0x5A, 0x5B, 0x5B, 0x5A, 0x01, 0x02],
            (0..=255).collect(),
        ] {
            let mut report = encode_frame(&payload).unwrap();
            report.resize(REPORT_LEN, 0);
            assert_eq!(decode_frame(&report).unwrap(), payload);
        }
    }

    #[test]
    fn rejeita_soma_errada() {
        let mut f = encode_frame(b"abc").unwrap();
        f[3] ^= 1;
        assert!(decode_frame(&f).is_err());
    }

    #[test]
    fn rejeita_comando_maior_que_o_relatorio() {
        assert!(encode_frame(&[0u8; REPORT_LEN]).is_err());
    }

    #[test]
    fn le_resposta() {
        let r =
            Response::parse(b"1 200\r\nAckNumber=113\r\nContentLength=2\r\n\r\n{}\0\0").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.header("AckNumber"), Some("113"));
        assert_eq!(r.body, "{}");
    }
}
