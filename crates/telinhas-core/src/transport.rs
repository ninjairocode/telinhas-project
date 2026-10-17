use hidapi::{HidApi, HidDevice};

use crate::{Error, Result};

/// Tamanho fixo dos relatórios HID de entrada e de saída do visor Thermaltake
/// (descritor de relatório: usage page de fabricante 0xFF00, 1024 bytes,
/// sem Report ID).
pub const REPORT_LEN: usize = 1024;

/// Camada fina sobre `hidapi`: relatórios de tamanho fixo, sem Report ID.
pub struct HidTransport {
    dev: HidDevice,
}

impl HidTransport {
    /// Abre o dispositivo pelo VID/PID (nunca pelo caminho `/dev/hidrawN`,
    /// que muda a cada conexão).
    pub fn open(api: &HidApi, vid: u16, pid: u16) -> Result<Self> {
        let present = api
            .device_list()
            .any(|d| d.vendor_id() == vid && d.product_id() == pid);
        if !present {
            return Err(Error::NotFound { vid, pid });
        }
        let dev = api.open(vid, pid)?;
        Ok(Self { dev })
    }

    /// Envia um relatório de saída. `payload` pode ter menos de 1024 bytes; o
    /// restante é preenchido com zeros.
    ///
    /// ATENÇÃO: não envie dados ao dispositivo sem conhecer o protocolo.
    pub fn write_report(&self, payload: &[u8]) -> Result<usize> {
        if payload.len() > REPORT_LEN {
            return Err(Error::PayloadTooLarge {
                got: payload.len(),
                max: REPORT_LEN,
            });
        }
        // O primeiro byte é o Report ID (0x00 = sem ID); o hidapi o remove
        // antes de enviar, então o dispositivo recebe os 1024 bytes de dados.
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..1 + payload.len()].copy_from_slice(payload);
        Ok(self.dev.write(&buf)?)
    }

    /// Lê um relatório de entrada; `None` se o tempo esgotar sem dados.
    pub fn read_report(&self, timeout_ms: i32) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; REPORT_LEN];
        let n = self.dev.read_timeout(&mut buf, timeout_ms)?;
        if n == 0 {
            Ok(None)
        } else {
            Ok(Some(buf[..n].to_vec()))
        }
    }
}
