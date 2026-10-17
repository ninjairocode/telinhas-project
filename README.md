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
| Brilho, rotação, vídeo | pendente |
| Interface gráfica (libcosmic) | pendente, depois do protocolo |
| Empacotamento AppImage (CI) | pronto, nunca executado |
| Empacotamento Flatpak (exigido pela loja COSMIC) | pendente |

## Gerar o AppImage (sem instalar nada)

1. Crie um repositório no GitHub e envie esta pasta (`git init`, `git add .`,
   `git commit`, `git push`).
2. Aba **Actions → AppImage → Run workflow**. Ao terminar, baixe o artefato
   `Telinhas-AppImage`.
3. `chmod +x Telinhas-x86_64.AppImage`

## Usar

```
./Telinhas-x86_64.AppImage udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules
sudo udevadm control --reload && sudo udevadm trigger   # depois reconecte o USB
./Telinhas-x86_64.AppImage list
./Telinhas-x86_64.AppImage listen --seconds 10          # somente leitura
./Telinhas-x86_64.AppImage prepare foto.png -o foto.jpg # gera o JPEG 720x720
./Telinhas-x86_64.AppImage probe                        # handshake; mostra versões
./Telinhas-x86_64.AppImage show foto.png --seconds 30   # mostra uma imagem
./Telinhas-x86_64.AppImage show --color ff0000          # cor sólida
```

`probe` e `show` enviam dados ao dispositivo; `--dry-run` mostra os bytes sem
enviar. A tela volta à própria imagem ~5 s depois que os quadros param.

## Compilar localmente

```
sudo apt install libudev-dev pkg-config
cargo build --release
```

## Direito autoral

Engenharia reversa feita só para interoperabilidade. Não inclua código, imagens
ou binários da Thermaltake neste repositório; implemente o protocolo do zero.
