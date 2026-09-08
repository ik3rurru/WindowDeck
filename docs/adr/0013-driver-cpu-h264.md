# ADR 0013 — Frames CPU del driver como entrada de H.264

Fecha: 2026-09-08. Estado: experimental; validado localmente y visualmente en la Deck el 8 de septiembre de 2026.

## Decisión

Añadir `windowdeck-host --driver-h264 [DIRECCIÓN]`. Tras negociar capacidades, el host inicia `windowdeck-display --cpu-frame-stream`, conectado al controlador elevado `--frame-broker`. Este crea el monitor y la memoria compartida CPU del ADR 0011. No requiere reinstalar el driver 0.1.0.9.

El auxiliar valida las dimensiones y secuencias, copia el frame más reciente disponible y libera cada slot antes de escribir al pipe. Entrega BGRA sin cabeceras a la entrada estándar de FFmpeg. Mantiene un único buffer local de 4.096.000 bytes y repite su contenido para una entrada nominal de 60 FPS cuando no hay una superficie nueva. Si el consumidor se retrasa, salta los plazos vencidos; no acumula una cola de frames. Los contadores `outputs`, `fresh` y `last_sequence` permiten distinguir las repeticiones de las superficies recibidas. `fresh` cuenta las actualizaciones copiadas, que pueden ser varias antes de una salida.

FFmpeg conserva libx264, ultrafast, zerolatency, sin B-frames, GOP 60, objetivo 16 Mbps, yuv420p y MPEG-TS. Comparte la configuración de salida y el transporte con WGC. El cliente no cambia. El modo experimental muestra el escritorio real; no abre el patrón de los probes ni usa WGC. No se añade un compositor de cursor independiente.

## Vida de los procesos

El host conserva un pipe privado hacia stdin del auxiliar. Un hilo nativo espera su cierre y termina el auxiliar incluso si stdout está bloqueado: así la muerte del host libera la conexión al broker. En los cierres del host controlados, el propietario Rust termina y recoge FFmpeg antes de terminar y recoger el auxiliar CPU. El broker retira el monitor cuando pierde la conexión; la retirada PnP es asíncrona.

La ruta usa el watchdog virtual existente para cierre TCP del cliente, desaparición/cambio de modo del monitor y diez segundos sin salida del encoder. La negociación incompatible no inicia el auxiliar ni activa la pantalla. Los probes finitos CPU/D3D11 siguen disponibles.

## Validación y límites

Compilación WDK, análisis, catálogo y autoprueba nativa correctos. Pasan los 19 tests Rust, formato y Clippy, incluido rechazo de capacidades en el nuevo modo. `test-auto-host.ps1 -DriverCpu` comprueba fallo del encoder, dos sesiones con desconexión y reconexión al mismo host, decodificación H.264 y terminación inesperada del host. El mismo arnés se ejecuta con WGC como regresión.

Las primeras sesiones locales de unos ocho segundos dieron alrededor de 57 FPS efectivos en el encoder CPU con salida declarada a 60 FPS. El escritorio era mayoritariamente estático: esta prueba no acredita 60 superficies nuevas por segundo, fidelidad bajo movimiento ni latencia visual. Mantener WGC como referencia hasta comparar ambas rutas en la Deck. No se ha optimizado la latencia ni elegido el IPC definitivo; encoder GPU y codificación dentro del IDD siguen pendientes.

Las evidencias y el estado final se registran en `docs/testing.md` y `docs/continuation.md`.

La prueba real posterior duró unos 356 segundos. El usuario confirmó calidad y latencia aceptables y recuperación perfecta de la ventana al cerrar el cliente. Se verificó retirada de monitor, encoder y auxiliar, con conservación de dispositivos físicos. Los últimos valores del encoder rondaban 50,32 FPS efectivos. Esta aceptación visual no demuestra mejora frente a WGC ni constituye una medición de latencia. Evidencia: `target/deck-cpu-live-57d03b14186f43d2ab175225cf9b0c34/`.
