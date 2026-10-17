# Telinhas

Controle mini telas de PC (visores de watercooler, gabinete etc.) no Linux, num
só lugar. Escrito em Rust. Meta: distribuir pela loja COSMIC do Pop!_OS.

## Status

| Peça | Estado |
|---|---|
| Detecção da tela Thermaltake TH360 V3 Ultra (`264a:2348`) | funciona, testada em hardware (Pop!_OS 24.04) |
| Transporte HID (relatórios de 1024 bytes) | funciona, testado em hardware |
| Protocolo BY: handshake `conn` | funciona, testado em hardware (`telinhas probe`) |
| Preparo de imagem (recorte 720x720 + JPEG) | implementado (`telinhas prepare`) |
| Envio de imagem para a tela | funciona, testado em hardware (`telinhas show`) |
| Brilho e rotação | funciona, testado em hardware (`telinhas set`) |
| Vídeo / quadros contínuos | pendente |
| Interface gráfica (libcosmic) | funciona, testada em hardware (`telinhas-gui`) |
| Empacotamento AppImage (CI) | pronto, nunca executado |
| Empacotamento Flatpak (exigido pela loja COSMIC) | pendente |

## Gerar o AppImage (sem instalar nada)

1. Envie o código para o GitHub (`git push`).
2. Aba **Actions → AppImage → Run workflow**. Ao terminar, baixe o artefato
   `Telinhas-AppImage`.
3. `chmod +x Telinhas-x86_64.AppImage`

## Usar

Na primeira vez, instale a regra udev (acesso à tela sem root):

```
./Telinhas-x86_64.AppImage udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules
sudo udevadm control --reload && sudo udevadm trigger   # depois reconecte o USB
```

### Interface gráfica

Abra o AppImage sem argumentos (ou `telinhas-gui`). Escolha uma imagem, veja a
prévia recortada em 720x720, clique em **Mostrar na tela** e ajuste brilho e
rotação. Sem permissão de acesso, a janela oferece o comando da regra udev para
copiar.

A imagem fica na tela **enquanto o app estiver aberto** (ele a reenvia a cada
2 s); ao fechar, a tela volta à própria imagem em ~5 s.

### Linha de comando

Com argumentos, o AppImage roda a CLI (`telinhas`):

```
./Telinhas-x86_64.AppImage list
./Telinhas-x86_64.AppImage listen --seconds 10          # somente leitura
./Telinhas-x86_64.AppImage prepare foto.png -o foto.jpg # gera o JPEG 720x720
./Telinhas-x86_64.AppImage probe                        # handshake; mostra versões
./Telinhas-x86_64.AppImage show foto.png --seconds 30   # mostra uma imagem
./Telinhas-x86_64.AppImage show --color ff0000          # cor sólida
./Telinhas-x86_64.AppImage set --brightness 60 --rotate 90
```

`probe`, `show` e `set` enviam dados ao dispositivo; `--dry-run` mostra os bytes sem
enviar. A tela volta à própria imagem ~5 s depois que os quadros param.

## Compilar localmente

```
sudo apt install libudev-dev libxkbcommon-dev pkg-config
cargo build --release            # gera target/release/telinhas e telinhas-gui
cargo build --release -p telinhas  # só a CLI, sem o libcosmic
```

O primeiro build da interface baixa e compila o libcosmic (alguns minutos).

## Direito autoral

Engenharia reversa feita só para interoperabilidade. Não inclua código, imagens
ou binários da Thermaltake neste repositório; implemente o protocolo do zero.
