pub mod thermaltake;

use hidapi::HidApi;

/// Descrição estática de um modelo suportado.
#[derive(Debug, Clone, Copy)]
pub struct DeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
}

/// Todos os modelos que o Telinhas conhece.
pub const KNOWN_DEVICES: &[DeviceInfo] = &[thermaltake::TH360_V3_ULTRA];

/// Relê a lista de dispositivos USB e devolve os modelos conhecidos
/// conectados. O `HidApi` guarda a lista de quando foi criado; programas de
/// longa duração (a interface gráfica) devem chamar isto periodicamente para
/// perceber telas conectadas ou desconectadas.
pub fn rescan(api: &mut HidApi) -> crate::Result<Vec<&'static DeviceInfo>> {
    api.refresh_devices()?;
    Ok(detect(api))
}

/// Modelos conhecidos na lista atual do `HidApi` (sem reler o USB).
pub fn detect(api: &HidApi) -> Vec<&'static DeviceInfo> {
    KNOWN_DEVICES
        .iter()
        .filter(|k| {
            api.device_list()
                .any(|d| d.vendor_id() == k.vid && d.product_id() == k.pid)
        })
        .collect()
}
