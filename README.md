# WindowDeck

**Actualización 0.2.0:** descubrimiento continuo, lanzador release con estados del
host, colas limitadas e integración opcional FFmpeg/SDL con codificación GPU y
cliente propio. Se conserva la compatibilidad con clientes MPEG-TS anteriores.
Consultar [las mejoras implementadas y su validación](docs/mejoras-implementadas.md)
para compilar y ejecutar la versión actual. Las secciones siguientes documentan
también las rutas anteriores del prototipo.

WindowDeck busca convertir la pantalla de una Steam Deck en un monitor secundario real de Windows 11 mediante la red local.

El proyecto está en fase de prototipo: valida el protocolo, una conexión TCP manual y la captura de una pantalla real de Windows. También puede transmitir y reproducir H.264 continuo a 1280 × 800 y 60 FPS, y ahora dispone de la ruta hardware descrita en las mejoras 0.2.0. El [prototipo de monitor virtual](driver/windows-idd/README.md) activa un escritorio extendido de 1280 × 800 a 60 Hz y supera diez ciclos de activación/retirada. La nueva ruta `--virtual-h264` captura ese escritorio mediante Windows Graphics Capture y ya permite usar la Deck como pantalla extendida, confirmado visualmente por el usuario. La latencia es perceptible y todavía no está medida. La opción `--auto-virtual-h264` vincula la activación y retirada del monitor a la sesión, con un controlador local iniciado previamente como administrador. El HDMI del dock de Steam Deck es una salida, no una entrada.

Para retomar el desarrollo, consultar el [punto de continuación de la última sesión](docs/continuation.md).

La ruta experimental `--driver-h264` codifica los frames CPU directos del driver mediante libx264. Está validada en loopback y en la Deck: el usuario confirma calidad y latencia aceptables y recuperación de ventanas al cerrar el cliente. Para probarla, compila con `driver/windows-idd/build.ps1` y `cargo build --workspace`, inicia `target/windows-idd/windowdeck-display.exe --frame-broker` como administrador y ejecuta `target/debug/windowdeck-host.exe --driver-h264 0.0.0.0:48150` sin elevar. Usa el cliente H.264 habitual. El monitor se activa al conectar y se retira al desconectar. Véase [ADR 0013](docs/adr/0013-driver-cpu-h264.md).

## Requisitos

- Rust estable con Cargo.
- FFmpeg disponible en `PATH` en el host y FFplay en el cliente para la prueba H.264 visible.
- Dos equipos en la misma red local o dos terminales en el mismo equipo.
- El puerto TCP elegido permitido por el firewall de Windows; el predeterminado es `48150`.

## Ejecutar

En Windows:

```powershell
cargo run -p windowdeck-host -- 0.0.0.0:48150
```

En SteamOS, sustituye `IP_DEL_PC` por la dirección local de Windows:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150
```

Añade `--fullscreen` para abrir directamente a pantalla completa:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150 --fullscreen
```

El cliente abre una ventana con el patrón; el número superior indica el frame y el inferior los milisegundos transcurridos en la sesión. Si el host se reinicia, la ventana permanece abierta y reconecta automáticamente. Ambos procesos imprimen métricas en la terminal. F11 alterna la pantalla completa, Escape vuelve al modo ventana y cerrar la ventana detiene el cliente. Pulsa `Ctrl+C` para detener el host.

Para comprobar la captura real de la pantalla principal en Windows sin enviarla todavía por red:

```powershell
cargo run -p windowdeck-host -- --capture-test
```

Puedes añadir el índice de otro monitor, por ejemplo `--capture-test 2`. La prueba confirma la recepción de una textura D3D11 y termina tras el primer frame.

Para transmitir una vista previa del primer monitor, inicia el host así y usa el cliente normalmente:

```powershell
cargo run -p windowdeck-host -- --capture 1 0.0.0.0:48150
```

Esta ruta temporal reduce la captura a 128 × 80 y RGB332 antes de enviarla. Sirve para validar el recorrido completo; H.264 y la resolución final pertenecen al siguiente hito.

Para comprobar el encoder H.264 de Windows sin guardar ni enviar el contenido de pantalla:

```powershell
cargo run -p windowdeck-host -- --encode-test
```

Puedes indicar otro monitor, por ejemplo `--encode-test 2`. La prueba codifica 60 frames a 60 FPS y 16 Mbps en memoria, muestra el tamaño resultante y termina. Usa la resolución actual de la pantalla.

Para probar H.264 continuo a través de la red, inicia el host:

```powershell
cargo run -p windowdeck-host -- --h264 1 0.0.0.0:48150
```

Y ejecuta el receptor desde el otro equipo o una segunda terminal:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150 --h264-test
```

FFmpeg captura el monitor indicado, lo ajusta a 1280 × 800, codifica H.264 por software a 60 FPS y 16 Mbps y envía MPEG-TS mientras la captura sigue activa. El cliente valida el orden de los paquetes y alimenta FFplay directamente, sin guardar la pantalla en disco ni crear una cola en la aplicación. Cierra la ventana para terminar y añade `--fullscreen` si quieres verla a pantalla completa.

Tras una conexión H.264 correcta, el cliente conserva la ventana y reintenta si se interrumpe la red. Cerrar la X cancela los reintentos. La primera conexión fallida, los errores de protocolo y la parada explícita del host terminan el cliente. Véase [ADR 0014](docs/adr/0014-h264-reconnection.md).

Para experimentar con encoders del host, configura `WINDOWDECK_H264_ENCODER` como `libx264` (predeterminado), `h264_amf`, `h264_nvenc` o `h264_mf` antes de iniciar el host. Los perfiles hardware son experimentales y deben compararse con movimiento; no se seleccionan automáticamente.

Durante la sesión, `h264_encoder_metrics`, `h264_send_metrics` y `h264_receive_metrics` muestran FPS, velocidad del encoder, bitrate, bytes y paquetes. `h264_first_packet_*` mide el arranque local de la tubería, no la latencia visual entre dos equipos. La línea base y el procedimiento reproducible están en [docs/testing.md](docs/testing.md).

Para comparar el buffering anterior de FFplay con el actual, añade `--ffplay-baseline` al cliente junto con `--h264-test`. Sin esa opción se usa el buffering reducido. El [procedimiento A/B](docs/testing.md#comparar-el-buffering-de-ffplay) mantiene el mismo host y registra ambos perfiles.

## Enviar el monitor virtual a la Deck

Para activar y retirar el monitor automáticamente con cada sesión, recompila la utilidad y el host. Con el driver de prueba ya instalado, abre una terminal de Windows **como administrador** y ejecuta una vez:

```powershell
.\target\windows-idd\windowdeck-display.exe --broker
```

En otra terminal normal, con FFmpeg en `PATH`:

```powershell
cargo run -p windowdeck-host -- --auto-virtual-h264 0.0.0.0:48150
```

Abre el cliente H.264 en la Deck como antes. Una conexión compatible activa la pantalla; cerrar el cliente detiene el encoder y retira el monitor. El host y el controlador quedan esperando otra sesión. Cierra el controlador con Ctrl+C cuando termines de usar WindowDeck. No hace falta pulsar X por cada conexión. El controlador debe pertenecer al mismo inicio de sesión de Windows que el host; todavía se inicia manualmente, sin servicio instalado. Véase la [decisión de ciclo de vida](docs/adr/0010-automatic-display-lifetime.md).

Para usar la activación manual de diagnóstico:

Con el [driver de prueba instalado](driver/windows-idd/README.md), recompila la utilidad con `driver/windows-idd/build.ps1`. En una terminal de Windows **como administrador**, activa el monitor y mantenla abierta:

```powershell
.\target\windows-idd\windowdeck-display.exe --run
```

En otra terminal normal, con FFmpeg disponible en `PATH`:

```powershell
cargo run -p windowdeck-host -- --virtual-h264 0.0.0.0:48150
```

Usa el mismo cliente H.264 de la Deck y la IP actual del PC. El host selecciona WindowDeck automáticamente; no hay que indicar un número de monitor. Se requiere FFmpeg con el filtro `gfxcapture`, comprobado en 9.0.1. Si ejecutas el host fuera de `target/debug` o `target/release`, coloca `windowdeck-display.exe` a su lado o configura `WINDOWDECK_DISPLAY_EXE` con su ruta.

Mueve una ventana al escritorio extendido para verla en la Deck. En este modo manual, cerrar el cliente detiene el vídeo; pulsa **X** en la utilidad de Windows para retirar el monitor. La [integración provisional](docs/adr/0009-virtual-desktop-capture.md) recaptura el escritorio: el intercambio directo de superficies con el driver sigue pendiente.

## Prototipo de frames directos del driver

Con el paquete de prueba **0.1.0.8** instalado y sin una sesión de vídeo activa, abre una vez como administrador:

```powershell
.\target\windows-idd\windowdeck-display.exe --frame-broker
```

Desde una terminal normal:

```powershell
cargo run -p windowdeck-host -- --driver-frame-test
```

La prueba activa el monitor virtual, muestra allí un patrón conocido, recibe 120 frames BGRA del driver mediante memoria compartida y comprueba muestras de sus píxeles en Rust. Retira la pantalla al terminar y no guarda imágenes. Es una referencia con copias CPU; el vídeo de la Deck sigue usando WGC. Resultados y límites en [ADR 0011](docs/adr/0011-driver-frame-transfer-probe.md).

Para comparar con texturas D3D11 compartidas, iniciar `windowdeck-display --gpu-frame-broker` como administrador y ejecutar `cargo run -p windowdeck-host -- --gpu-frame-test` desde una terminal normal. Requiere el driver 0.1.0.9. La prueba verifica el mismo patrón; el readback se realiza en el auxiliar. Ambas rutas han pasado cierre del host y reconexión. La comparación no demuestra una mejora para el encoder CPU actual; véanse resultados y decisión provisional en [ADR 0012](docs/adr/0012-shared-d3d11-frame-probe.md).

## Probar el Flatpak en Steam Deck

La acción `Flatpak` de GitHub genera un artefacto `WindowDeck-flatpak` para Steam Deck. Descarga y descomprime el artefacto, copia `WindowDeck.flatpak` a la Deck y, en modo escritorio, ejecuta:

```bash
flatpak install --user ./WindowDeck.flatpak
flatpak run io.github.ik3rurru.WindowDeck IP_DEL_PC:48150 --h264-test --fullscreen
```

El Flatpak incluye el cliente y usa FFplay y los códecs del runtime Freedesktop 25.08. En el PC debe seguir ejecutándose el host H.264 mostrado arriba. Para desinstalar la prueba:

```bash
flatpak uninstall --user io.github.ik3rurru.WindowDeck
```

## Comprobar

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Consulta [la hoja de ruta](WINDOWDECK_ROADMAP.md) para conocer el alcance y los hitos.

## Licencia

Disponible bajo licencia MIT o Apache 2.0, a elección del usuario.

Excepción: la adaptación del ejemplo Microsoft en [driver/windows-idd](driver/windows-idd/README.md) se distribuye bajo [MS-PL](driver/windows-idd/LICENSE).

Estado de rendimiento (2026-09-08): el usuario acepta la transmisión estable durante el uso normal y pausa la optimización de FPS. Se conservan las métricas por intervalo; véase [revisión de cadencia](docs/fps-pacing-review.md).
