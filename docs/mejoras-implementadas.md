# Mejoras implementadas — WindowDeck 0.2.0

Implementación del 10 de septiembre de 2026. Se conserva Rust, TCP, la ruta CPU
y los procedimientos de suspensión/recuperación existentes. La integración
FFmpeg/SDL se activa al compilar con `native-media`.

## Ejecutar

En Windows, descomprimir el paquete y abrir `WindowDeck.vbs`. Incluye host,
auxiliar, cliente, FFmpeg, SDL y manifiesto de versiones. Se necesita el driver
de WindowDeck ya instalado: el paquete no instala ni reemplaza drivers.

Desde el repositorio:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-media.ps1 -Download
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/build.ps1
```

Abrir después `WindowDeck.vbs`. El lanzador utiliza `target/release`, detecta
la integración nativa y recibe estados del host. Detener solicita el cierre de
la sesión y la liberación del monitor; matar el proceso queda como respaldo si
no termina en ocho segundos. El controlador elevado conserva el monitor solo
mientras existe una concesión del host.

El lanzador mantiene disponibles los brokers GPU y CPU para negociar con clientes
anteriores. Si la Deck solo admite MPEG-TS, el host selecciona la ruta CPU anterior.
Para forzarla: `powershell -ExecutionPolicy Bypass -File scripts/WindowDeck.ps1 -Legacy`.
La corrección de las desconexiones al arrancar está en el host: no requiere
reinstalar el Flatpak 0.1.0 que ya tiene la Deck. La prueba real del
10 de septiembre está documentada en `docs/testing.md`.

Lanzamiento nativo manual: iniciar `windowdeck-display.exe --gpu-frame-broker`
como administrador y `windowdeck-host.exe --driver-native-h264 0.0.0.0:48150`
sin elevar. Iniciar también `--frame-broker` si se admitirán clientes anteriores.

El Flatpak se compila ahora con `native-media`. Sin dirección, el cliente usa
descubrimiento automático. Con IP: `windowdeck-client IP:48150 --native --fullscreen`.
Un host anterior puede negociar MPEG-TS y utilizar FFplay. `--ffplay` selecciona
expresamente la reproducción anterior.

## Cambios

- Descubrimiento activo durante toda la sesión, conexión al resolver un equipo y
  reutilización del resultado. La ventana actualiza los equipos sin reabrirse.
  Las reconexiones mantienen su identidad y consultan las direcciones actuales.
- Lanzador release con estados `listening`, `negotiating`, `streaming`, `stopped`.
  Se eliminan las consultas `Get-NetTCPConnection` del hilo del panel.
- Driver pequeño: publica superficies BGRA; el encoder permanece en el host.
  La nueva concesión `--gpu-lease` entrega al host el nombre de las superficies.
- Host integrado: abre las texturas en el adaptador del broker, consume sus
  keyed mutex, selecciona la imagen más reciente y convierte BGRA a NV12 con
  D3D11. AMF/NVENC reciben superficies GPU directamente, sin readback CPU.
- `WINDOWDECK_H264_ENCODER=auto` selecciona el encoder del adaptador y usa
  libx264 si no puede abrirlo. `libx264` fuerza el respaldo integrado con
  readback; `--driver-h264` conserva el pipeline CPU anterior completo.
- Fotogramas nuevos con más de 50 ms se descartan antes de codificar. Un
  escritorio estático puede repetir la última imagen válida a 60 Hz. Se
  registran adquisiciones nuevas, repeticiones y descartes por separado.
- Colas codificadas pequeñas. La ruta nativa limita la antigüedad a 250 ms y
  vuelve a conectar desde un IDR si caduca. La compatibilidad MPEG-TS/ffplay
  conserva dos chunks y tolera bloqueos de hasta diez segundos: su arranque y
  las pausas breves de Wi-Fi pueden superar 250 ms. Nunca se descartan bytes
  codificados para continuar con un flujo incompleto.
- Cliente integrado: FFmpeg decodifica dentro del proceso Rust y SDL administra
  ventana, F11, Escape y presentación. Se intenta D3D11VA en Windows y VAAPI en
  Linux; el registro identifica el formato realmente activado. Si no se puede
  crear el dispositivo hardware, se abre el decoder por software. La reconexión
  vacía las referencias del decoder y exige una imagen inicial independiente.

La envoltura del protocolo sigue siendo la versión 3. La capacidad nueva
`H264Frames=3` transporta unidades de acceso reales, fragmentadas hasta 64 KiB,
con timestamp QPC de adquisición y flag de keyframe del encoder. El ensamblador
limita cada unidad a 4 MiB y verifica sesión, orden y metadatos. `H264=2` conserva
MPEG-TS: `captured_micros=0` indica que su captura es desconocida y su marca
inicial conserva únicamente la semántica histórica de inicio de flujo.

## Mediciones y límites

`native_encoder_metrics` mide adquisición/publicación del driver, conversión
incluida su finalización en GPU y entrega de cada fotograma codificado.
`native_send_metrics` y `native_receive_metrics` miden operaciones locales de
transporte. `native_receive_queue_metrics` mide la espera en el cliente.
`native_player_metrics` informa decodificación, transferencia/conversión para
mostrar, llamadas de presentación y descartes de imágenes decodificadas.
La ruta anterior añade tiempos de lectura del encoder, escritura TCP, lectura
TCP y escritura al reproductor.

Los tiempos son monotónicos y locales: no se restan relojes de dos equipos.
`receive_to_present_mean_us` comienza al entregar la unidad al decoder; la
espera previa se registra por separado. SDL_RenderPresent no certifica el
instante de barrido físico del panel. Sigue pendiente medir la latencia visual
extremo a extremo con instrumentación apropiada.

El renderer SDL utiliza aceleración cuando está disponible. Actualmente descarga
las superficies del decoder hardware antes de subir la textura de presentación.
La eliminación del readback está implementada en el host; importar VAAPI
directamente a la superficie de presentación de la Deck es una optimización
posterior. No se afirma que el cliente sea una ruta sin copias CPU.

## Comprobar

```powershell
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
. ./scripts/build-media.ps1 -PrepareOnly
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
target/release/windowdeck-client.exe --media-self-test
python scripts/test-native-client.py --client target/release/windowdeck-client.exe
target/release/windowdeck-host.exe --gpu-self-test auto
target/release/windowdeck-host.exe --gpu-self-test libx264
```

Los ensayos multimedia generan contenido conocido sin capturar el escritorio.
El cliente verifica píxeles, 18 presentaciones y tres reinicios del decoder.
La prueba TCP corta una unidad a medias, reconecta, reinicia la secuencia y
comprueba una parada explícita. El ensayo del host utiliza el mismo selector de
adaptador que el broker y verifica 120 imágenes tras codificarlas y decodificarlas.

En este equipo han pasado los ensayos de NVENC, AMF en el adaptador del broker y
libx264. El ensayo sintético estabilizado alcanza aproximadamente 60 imágenes
codificadas por segundo. Esto no demuestra 60 FPS visibles en la Deck ni
sustituye una comparación de escritorio y Wi-Fi. Los procedimientos de
`driver/windows-idd/test-*.ps1` se conservan; deben repetirse físicamente con la
nueva ruta y la Steam Deck.

CI compila Rust con/sin integración en Windows/Linux e incluye codecs,
reconexión TCP y compilación/pruebas del auxiliar C++. El paquete UMDF completo
se compila localmente con WDK. CI no instala drivers ni prueba una GPU real.

## Versiones y empaquetado

`--version` identifica host, cliente, auxiliar y FFmpeg integrado. El script de
SDK fija FFmpeg 9.0.1 y SDL 2.32.10 mediante SHA256; Cargo utiliza `Cargo.lock`.
El paquete conserva licencias, código fuente del proyecto y manifiesto.
FFmpeg incluye libx264/GPLv3; esa licencia no se sustituye por las licencias del
código Rust de WindowDeck.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipBuild
powershell -ExecutionPolicy Bypass -File scripts/inventory.ps1 -Output target/inventory.json
```

El inventario distingue binarios construidos, dispositivos instalados y DLLs de
DriverStore. Reconstruir `WindowDeckDisplay.dll` no instala esa DLL. Este cambio
conserva los formatos compartidos 1/2 del driver 0.1.0.9 y añade funciones al
auxiliar; no instala una nueva DLL automáticamente.

Referencias: [FFmpeg D3D11](https://ffmpeg.org/doxygen/trunk/structAVD3D11VAFramesContext.html),
[conversión D3D11](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_video_processor_color_space)
y [texturas SDL](https://wiki.libsdl.org/SDL2/SDL_UpdateYUVTexture).
