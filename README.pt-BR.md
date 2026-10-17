<p align="center">
  <img src="packaging/icons/hicolor/256x256/apps/io.github.ninjairocode.Telinhas.png" alt="Logo do Telinhas" width="160">
</p>

<h1 align="center">Telinhas</h1>

<p align="center">
  Gerencie as mini telas do seu PC no Linux, num só lugar.<br>
  <a href="README.md">Read in English</a>
</p>

> [!WARNING]
> **Projeto em estágio inicial.** Por enquanto o Telinhas suporta **um único
> dispositivo** (veja [Dispositivos suportados](#dispositivos-suportados)). A
> compatibilidade vai aumentar a cada atualização. Espere algumas arestas e,
> por favor, relate os problemas.

<p align="center">
  <img src="docs/screenshots/main-window.png" alt="Janela principal do Telinhas" width="420">
</p>

## O que é

Os PCs têm cada vez mais telinhas: visores de watercooler, painéis de gabinete,
mini monitores USB. Cada marca oferece o próprio app, quase sempre só para
Windows. O Telinhas quer gerenciar todas elas num único app para Linux, escrito
em Rust, com interface nativa para o desktop [COSMIC](https://system76.com/cosmic).

**Hoje você pode:**

- mostrar uma imagem na tela (recortada automaticamente na resolução dela);
- ajustar brilho e rotação;
- conectar e desconectar a tela com o app aberto (ele reconecta sozinho).

**Planejado:**

- suporte a mais modelos e marcas;
- informações do sistema na tela, como temperatura e uso do processador;
- funcionar em segundo plano e iniciar com o sistema;
- animações e vídeo.

## Dispositivos suportados

| Dispositivo | ID USB | Estado |
|---|---|---|
| Thermaltake TH360 V3 Ultra ARGB Sync (LCD 720×720) | `264a:2348` | Funciona (imagem, brilho, rotação) |

Outras telas Thermaltake que usam o mesmo protocolo "BY" podem funcionar com
pequenos ajustes, mas não foram testadas. Se você tem outra tela e quer ajudar,
veja [Como contribuir](#como-contribuir).

## Instalação

O Telinhas é distribuído de duas formas:

| Canal | Estado | Atualizações |
|---|---|---|
| **AppImage** nas [Releases do GitHub](https://github.com/ninjairocode/telinhas-project/releases) | Disponível | Manual ou com AppImageUpdate (veja [Atualização](#atualização)) |
| **COSMIC Store** (Flatpak, via Flathub) | Em breve | Automáticas, pela loja |

### AppImage

1. Baixe o `Telinhas-x86_64.AppImage` da
   [última release](https://github.com/ninjairocode/telinhas-project/releases/latest).
2. Torne o arquivo executável:
   ```
   chmod +x Telinhas-x86_64.AppImage
   ```
3. **Uma vez só:** libere o acesso à tela sem root (regra udev) e depois
   desconecte e reconecte o cabo USB da tela:
   ```
   ./Telinhas-x86_64.AppImage udev-rules | sudo tee /etc/udev/rules.d/99-telinhas.rules
   sudo udevadm control --reload && sudo udevadm trigger
   ```
   Se você pular este passo, o app mostra o comando para copiar.
4. Abra com dois cliques ou rode `./Telinhas-x86_64.AppImage`.

**Requisitos:** Linux 64 bits (x86_64) com desktop Wayland ou X11 e drivers de
vídeo funcionando (Vulkan ou OpenGL). A interface foi feita para o COSMIC, mas
também roda em outros desktops. Se a AppImage não abrir por erro de FUSE (comum
em contêineres), rode com `--appimage-extract-and-run`.

### COSMIC Store

Em breve. O app será publicado no Flathub, que é a fonte usada pela COSMIC
Store. A regra udev acima continuará necessária, porque apps Flatpak não
conseguem instalá-la no sistema sozinhos.

## Uso

### Interface gráfica

1. Clique em **Escolher imagem…** e selecione um JPEG, PNG ou WebP. A prévia
   mostra como ela será recortada.
2. Clique em **Mostrar na tela**.
3. Ajuste o **Brilho** (aplicado ao soltar o controle) e a **Rotação**.

> [!NOTE]
> A tela só mantém a imagem escolhida **enquanto o Telinhas estiver aberto**:
> o app reenvia a imagem a cada 2 segundos, e a tela volta à própria imagem
> uns 5 segundos depois que isso para. Minimizar a janela não atrapalha;
> fechá-la encerra a imagem.

### Linha de comando

A AppImage também funciona como ferramenta de linha de comando quando recebe
argumentos (`telinhas`, se compilado do código-fonte):

```
./Telinhas-x86_64.AppImage list                         # procura telas suportadas
./Telinhas-x86_64.AppImage show foto.png --seconds 30   # mostra uma imagem
./Telinhas-x86_64.AppImage show --color ff0000          # cor sólida
./Telinhas-x86_64.AppImage set --brightness 60 --rotate 90
./Telinhas-x86_64.AppImage prepare foto.png -o saida.jpg # só converte, não envia
./Telinhas-x86_64.AppImage probe                        # handshake; mostra versões
./Telinhas-x86_64.AppImage listen --seconds 10          # depuração, só leitura
```

`probe`, `show` e `set` enviam dados ao dispositivo; acrescente `--dry-run`
para ver os bytes sem enviar.

## Atualização

| Instalado por | Como atualizar |
|---|---|
| **AppImage** | Baixe o novo `Telinhas-x86_64.AppImage` nas [Releases](https://github.com/ninjairocode/telinhas-project/releases/latest) e substitua o arquivo antigo (rode `chmod +x` de novo). Ou use o [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate): `AppImageUpdate Telinhas-x86_64.AppImage` baixa só o que mudou. |
| **COSMIC Store / Flatpak** | As atualizações chegam pela loja. Pelo terminal: `flatpak update`. |
| **Código-fonte** | `git pull` e depois `cargo build --release` de novo. |

Não é preciso reinstalar a regra udev ao atualizar, a menos que as notas da
versão digam o contrário (por exemplo, quando um dispositivo novo for
adicionado).

Veja o [CHANGELOG.md](CHANGELOG.md) para saber o que mudou em cada versão.

## Compilar do código-fonte

Requer [Rust](https://rustup.rs) 1.93 ou mais novo e, no Ubuntu/Debian/Pop!_OS:

```
sudo apt install libudev-dev libxkbcommon-dev libwayland-dev pkg-config
git clone https://github.com/ninjairocode/telinhas-project.git
cd telinhas-project
cargo build --release              # target/release/telinhas-gui e telinhas
cargo build --release -p telinhas  # só a linha de comando, sem o libcosmic
```

O primeiro build baixa e compila o [libcosmic](https://github.com/pop-os/libcosmic)
e leva alguns minutos. Instale a regra udev de `packaging/99-telinhas.rules`
como em [AppImage](#appimage).

### Organização do projeto

| Caminho | Conteúdo |
|---|---|
| `crates/telinhas-core` | Drivers dos dispositivos, transporte HID, protocolos, imagens |
| `crates/telinhas` | Ferramenta de linha de comando |
| `crates/telinhas-gui` | Interface COSMIC (libcosmic) |
| `packaging/` | Regra udev, atalho do menu, metadados AppStream, ícones |
| `docs/PROTOCOL.md` | Notas sobre o protocolo "BY" da Thermaltake (em inglês) |

## Como contribuir

A contribuição mais valiosa são dispositivos novos. Abra uma
[issue](https://github.com/ninjairocode/telinhas-project/issues) com o modelo
da tela e a saída do `lsusb`. Cada dispositivo vira um módulo em
`crates/telinhas-core/src/devices/`.

## Suporte

- Bugs e pedidos: [GitHub Issues](https://github.com/ninjairocode/telinhas-project/issues)
- E-mail: [ninjairocode@outlook.com](mailto:ninjairocode@outlook.com)

## Licença e aviso legal

O Telinhas é software livre, sob a [GNU GPL v3.0 ou posterior](LICENSE).

Os protocolos dos dispositivos foram implementados do zero, apenas para
interoperabilidade. Este repositório não contém código, imagens ou binários dos
fabricantes. Os nomes de produtos são marcas de seus respectivos donos; o
Telinhas não é afiliado a eles nem endossado por eles.
