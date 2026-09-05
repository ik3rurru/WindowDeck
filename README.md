# WindowDeck

WindowDeck busca convertir la pantalla de una Steam Deck en un monitor secundario real de Windows 11 mediante la red local.

El proyecto está en fase de prototipo: valida el protocolo, una conexión TCP manual y la captura de una pantalla real de Windows. También puede transmitir y reproducir H.264 continuo a 1280 × 800 y 60 FPS, pero todavía no usa aceleración hardware ni crea un monitor virtual. El HDMI del dock de Steam Deck es una salida, no una entrada.

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

Durante la sesión, `h264_encoder_metrics`, `h264_send_metrics` y `h264_receive_metrics` muestran FPS, velocidad del encoder, bitrate, bytes y paquetes. `h264_first_packet_*` mide el arranque local de la tubería, no la latencia visual entre dos equipos. La línea base y el procedimiento reproducible están en [docs/testing.md](docs/testing.md).

Para comparar el buffering anterior de FFplay con el actual, añade `--ffplay-baseline` al cliente junto con `--h264-test`. Sin esa opción se usa el buffering reducido. El [procedimiento A/B](docs/testing.md#comparar-el-buffering-de-ffplay) mantiene el mismo host y registra ambos perfiles.

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
