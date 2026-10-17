# Telinhas

Controle mini telas de PC (visores de watercooler, gabinete etc.) no Linux, num
só lugar. Escrito em Rust. Meta: distribuir pela loja COSMIC do Pop!_OS.

## Status

| Peça | Estado |
|---|---|
| Detecção da tela Thermaltake TH360 V3 Ultra (`264a:2348`) | funciona (não testado em hardware por mim) |
| Transporte HID (relatórios de 1024 bytes) | implementado |
| **Envio de imagem/vídeo para a tela** | **pendente: formato dos pacotes ainda não mapeado** (`docs/PROTOCOL.md`) |
| Interface gráfica (libcosmic) | pendente, depois do protocolo |
| Empacotamento AppImage (CI) | pronto, nunca executado |

Este código **nunca foi compilado** onde foi escrito (sem Rust no ambiente). Na
primeira execução do CI pode ser preciso ajustar algo pequeno.

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
```

Nenhum comando envia dados ao dispositivo.

## Compilar localmente

```
sudo apt install libudev-dev pkg-config
cargo build --release
```

## Direito autoral

Engenharia reversa feita só para interoperabilidade. Não inclua código, imagens
ou binários da Thermaltake neste repositório; implemente o protocolo do zero.
