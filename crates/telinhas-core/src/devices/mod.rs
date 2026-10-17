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

/// Modelos conhecidos que estão conectados agora.
pub fn detect(api: &HidApi) -> Vec<&'static DeviceInfo> {
    KNOWN_DEVICES
        .iter()
        .filter(|k| {
            api.device_list()
                .any(|d| d.vendor_id() == k.vid && d.product_id() == k.pid)
        })
        .collect()
}
