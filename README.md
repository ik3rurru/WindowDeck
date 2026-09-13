# WindowDeck

[![CI](https://github.com/ik3rurru/WindowDeck/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ik3rurru/WindowDeck/actions/workflows/ci.yml)
[![Flatpak](https://github.com/ik3rurru/WindowDeck/actions/workflows/flatpak.yml/badge.svg?branch=main)](https://github.com/ik3rurru/WindowDeck/actions/workflows/flatpak.yml)

WindowDeck convierte la Steam Deck en una segunda pantalla de Windows 11 a través
de la red local. Crea un monitor virtual de 1280 × 800 a 60 Hz, permite extender el
escritorio y retira la pantalla al terminar la sesión. El HDMI del dock es una
salida, no una entrada.

**Ruta recomendada: frames CPU del driver → H.264/libx264 → cliente de la Deck.**
Es la ruta comprobada en la Deck y la que utiliza el panel por defecto. La
integración GPU permanece experimental hasta validar su recorrido completo en
ese equipo. Los 60 Hz configurados no garantizan 60 imágenes nuevas por segundo.

## Empezar

Descarga los paquetes de Windows y Steam Deck desde
[GitHub Releases](https://github.com/ik3rurru/WindowDeck/releases).

Necesitas Windows 11 con el [driver de prueba de WindowDeck instalado](driver/windows-idd/README.md),
el paquete completo de Windows y el cliente Flatpak en la Deck, ambos en una LAN
de confianza. El paquete incluye FFmpeg y sus DLL; no requiere instalarlos por
separado ni modificar el PATH. El driver sigue utilizando firma de desarrollo;
el paquete de la aplicación no lo instala.

1. Descomprime el paquete Windows y abre **`WindowDeck.exe`** sin ejecutar como
   administrador. Pulsa **Iniciar** y acepta la elevación del broker.
2. Abre **WindowDeck** en el modo escritorio de la Steam Deck. El descubrimiento
   automático localiza el PC; si encuentra varios, permite elegirlo.
3. Mueve una ventana al escritorio extendido. Cierra el cliente o pulsa
   **Detener** en Windows para retirar el monitor y recuperar las ventanas.

El panel muestra el estado de conexión y permite abrir los registros. El host
se ejecuta sin elevar. Detalles en [la guía del lanzador](docs/launcher.md).

El host negocia vídeo con el reproductor integrado; los clientes anteriores
siguen siendo compatibles mediante MPEG-TS. En la Deck, extrae el paquete
`WindowDeck-0.2.1-steamdeck-x86_64.tar.gz` y ejecuta en su carpeta:

```bash
bash install-steamdeck.sh
```

Instala el Flatpak y crea automáticamente el acceso del escritorio y
`~/.local/bin/windowdeck`. En Steam, utiliza **Añadir un producto que no es de
Steam** y selecciona **WindowDeck**, o busca ese ejecutable; no fuerces Proton.
Aparecerá en **Fuera de Steam** del modo juego. Véase la
[guía de Steam Deck y comparación de formatos](docs/steamdeck-distribution.md).
La aceptación de vídeo en modo juego sigue pendiente.

También puedes abrir solo `WindowDeck.flatpak` con Discover o instalarlo
directamente; esta vía crea la entrada del menú. Para añadir después el acceso
del escritorio, ejecuta `bash install-steamdeck.sh --shortcuts-only`.

```bash
flatpak install --user ./WindowDeck.flatpak
flatpak run io.github.ik3rurru.WindowDeck --fullscreen
```

Si mDNS no está disponible, admite una dirección manual:
`flatpak run io.github.ik3rurru.WindowDeck IP_DEL_PC:48150 --native --fullscreen`.
El cliente anterior utiliza `--h264-test` en lugar de `--native`.
Cerrar la X cancela la reconexión; F11 alterna pantalla completa y Escape vuelve
al modo ventana. El panel de Windows y la gestión de sus procesos están en Rust.
El acceso directo de Windows se crea con `scripts/install-shortcut.ps1`.

## Seguridad actual

El protocolo TCP todavía **no autentica dispositivos ni cifra el vídeo o el
control**. El descubrimiento mDNS identifica servicios, no acredita su identidad.
Utiliza el prototipo únicamente en una red local de confianza.

Existen mensajes versionados, límites de 64 KiB por mensaje, validación de
fragmentos y límites de tiempo y de colas. El lanzador configura reglas de
firewall para el host en redes privadas y desde la subred local. Estas medidas
no sustituyen el emparejamiento ni el cifrado. Quedan pendientes autenticación,
claves por dispositivo, revocación y completar la cobertura de entradas inválidas.
La entrada remota de ratón, teclado y táctil aún no está implementada.

## Desarrollo y comprobaciones

La [guía de desarrollo](docs/development.md) explica cómo compilar el paquete,
iniciar la ruta CPU manualmente y ejecutar `windowdeck-host diag --help`.
Los diagnósticos y las rutas históricas se mantienen allí, con enlaces a sus
mediciones. La captura WGC y los prototipos RGB332 quedan congelados como
referencias; el uso normal emplea `windowdeck-host --driver-h264`.

```bash
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

CI ejecuta estas comprobaciones en Windows y Linux, y una segunda matriz compila
y prueba la integración nativa. Los badges enlazan los resultados públicos.
Las pruebas con driver y GPU reales se documentan por separado; CI no sustituye
las sesiones con la Deck.

- [Pruebas y mediciones, incluida la corrección de desconexiones](docs/testing.md).
- [Integración multimedia y límites de la versión 0.2.0](docs/mejoras-implementadas.md).
- [Roadmap original](WINDOWDECK_ROADMAP.md) y [consolidación](windowdeck-roadmap-consolidacion.md).
- [Decisión de rutas soportadas](docs/adr/0016-supported-video-routes.md).
- [Decisión de integración y distribución de FFmpeg](docs/adr/0017-media-packaging.md).
- [Estado para continuar el desarrollo](docs/continuation.md).

## Licencias

El código Rust de WindowDeck se distribuye bajo MIT o Apache 2.0, a elección del
usuario. La adaptación del ejemplo de Microsoft en
[driver/windows-idd](driver/windows-idd/README.md) utiliza [MS-PL](driver/windows-idd/LICENSE).
El paquete conserva las licencias de sus dependencias, incluidas FFmpeg/libx264
GPLv3 y SDL. Véase el [ADR de distribución](docs/adr/0017-media-packaging.md).
