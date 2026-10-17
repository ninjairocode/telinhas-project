# Protocolo da tela Thermaltake (264a:2348)

Fonte: análise estática do instalador `TT LCD Screen_1.4.0.exe` (NSIS 3.04 com
7z embutido; app Electron + Vue). Nada abaixo foi confirmado em hardware.

## Confirmado (lido diretamente dos arquivos)

- USB: VID `0x264a`, PID `0x2348`, "ThermalTake USB Device", USB 2.0.
- Uma interface HID, endpoints de interrupção IN `0x81` e OUT `0x01`,
  `wMaxPacketSize` 1024, `bInterval` 4.
- Descritor de relatório HID (36 bytes):
  `06 00 ff 09 01 a1 01 09 01 15 00 26 ff 00 75 08 96 00 04 81 02 09 01 15 00 26 ff 00 75 08 96 00 04 91 02 c0`
  = usage page de fabricante `0xFF00`; um relatório de entrada e um de saída,
  1024 bytes cada, **sem Report ID**.
- O app também suporta telas Android via ADB (600x1024, 720x1480; VID Google
  `0x18D1`) e uma porta serial; **a TH360 aparece como HID**, não como ADB.
- O app reconhece a resolução `720x720`.

## Onde está o protocolo

- A UI (renderer) é JavaScript minificado e não contém o protocolo.
- O processo principal está em **bytecode V8** (`out/main/index.jsc`); suas
  strings mostram só o fluxo: comandos `displayInSleep`, `power`,
  `realtimeDisplay`, fila de comandos com timeout/retry, `AckNumber`, checksum,
  "Protocol version not match", atualização de firmware por `.zip` via HID ou
  serial, frames em JPEG.
- A lógica de transporte está num addon nativo **Rust (napi-rs)**:
  `resources/app.asar` → `/node_modules/node-by-addons/node-by-addons.win32-x64-msvc.node`
  (20 MB, v1.16.0). Usa a crate `hidapi` 2.6.6. Módulos internos:
  `node_by_addons::devices::hid` (`DeviceInfo`, `HHidDevice`),
  `node_by_addons::protocol::low::frame_tlv` (`src\protocol\low\frame_tlv.rs`)
  e `node_by_addons::protocol::file` (`meta.rs`, `mod.rs`).
- Mensagens do módulo `frame_tlv`: "DataFrameTLV buffer len is greater then …,
  current len is …", "DataFrameTLV value buffer len is zero", "DataFrameTLV
  value buffer is too big; len …", "Failed to create data frame tlv".
- `BYMonitor64.dll` é o `HWiNFO64.dll` renomeado (sensores); `MediaStation.dll`
  é só processamento de mídia. Nenhuma das duas fala com a tela.

## Inferido (não confirmado)

- Quadro TLV (tipo, tamanho, valor) dentro de relatórios de 1024 bytes, com
  camada de "arquivo" (metadados + dados) para enviar JPEG/vídeo.
- Teto de ~1 MB/s pelo intervalo de 1 ms: algo como 15–30 quadros/s de JPEG.

## O que falta e como obter

Falta o layout exato: tamanho do cabeçalho, códigos de tipo, checksum/CRC,
numeração de ack, sequência dos pacotes de "arquivo". Dois caminhos:

1. **Captura USB** do app oficial (VM Windows com repasse da tela + `usbmon` no
   host, filtrando o endpoint `0x01`). Ações isoladas: conectar, mudar brilho,
   enviar uma imagem de cor sólida.
2. **Decompilar o addon no Ghidra** (importar o `.node` como PE x64). Pontos de
   partida (imagebase `0x180000000`):
   - strings do `frame_tlv`: `0x180e6bf40` (caminho do .rs), mensagens em
     `0x180e6c0c1`–`0x180e6c146`
   - chamada de log do TLV (macro `tracing`): `0x180059f12`
   - invólucro de escrita HID ("size can not be 0"): `0x180c9777d`
   - "Failed to create data frame tlv": `0x1802e65f0`, `0x1802e6cb9`
   Procure funções que montam um buffer de 1024 bytes e chamam `hid_write`.

## Regras

- Não envie dados desconhecidos ao dispositivo (principalmente algo parecido
  com firmware).
- Implemente o protocolo do zero; não copie código ou binários da Thermaltake.
