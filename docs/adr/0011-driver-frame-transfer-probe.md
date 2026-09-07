# ADR 0011: prototipo de frames del IDD a memoria compartida

- Fecha: 2026-09-07
- Estado: prototipo CPU implementado y validado en UMDF con el driver 0.1.0.8; comparación de alternativas e integración con el encoder pendientes.

## Alcance

Medir una ruta directa del worker UMDF/IddCx al host, sin Windows Graphics Capture ni FFmpeg. Esta iteración prueba la alternativa temporal de copia a memoria compartida del roadmap. No decide aún el IPC definitivo: faltan comparar recursos D3D11 compartidos y codificación dentro del IDD. No modifica los parámetros del vídeo existente.

El paquete de prueba 0.1.0.8 añade una salida opcional: `windowdeck-display --frame-broker` crea un mapping global nuevo por petición, publica su nombre mediante una propiedad del software device y activa el monitor existente. `--broker`, `--run` y el streaming WGC siguen disponibles. El driver abre el mapping solo si la propiedad contiene un nombre válido del prototipo.

El mapping usa un GUID aleatorio y ACL para el inicio de sesión solicitante, SYSTEM, LocalService y el SID de drivers UMDF; sin Everyone. El pipe de control sigue limitado al inicio de sesión local. El driver consulta la propiedad mediante [WdfDeviceQueryPropertyEx](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfdevice/nf-wdfdevice-wdfdevicequerypropertyex). El uso del SID UMDF se apoya en [Controlling Device Access](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/controlling-device-access); el espacio global permite compartir entre la sesión del servicio y la interactiva, con las restricciones de [Kernel object namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces).

## Tubería y límites

1. El worker adquiere la superficie BGRA 1280 × 800 y envía una copia GPU a uno de dos recursos staging. Llama a `IddCxSwapChainFinishedProcessingFrame` después de enviar el trabajo GPU, conforme a su [contrato](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/iddcx/nf-iddcx-iddcxswapchainfinishedprocessingframe).
2. Consulta el staging con `Map(DO_NOT_WAIT)` y, cuando está listo, copia las filas respetando `RowPitch` a uno de tres slots. No espera al consumidor. Los slots reservados por un lector no se sobrescriben; los listos aún no consumidos sí pueden reemplazarse.
3. El auxiliar de usuario reclama y copia un frame completo antes de liberarlo. Transfiere cabecera fija y BGRA al host Rust por stdout binario. Rust valida secuencia, dimensiones, timestamps y muestras de píxeles del patrón.

El mapping ocupa exactamente 12.288.448 bytes; el tamaño de los slots y sus offsets son constantes compiladas, nunca valores de memoria modificable por el consumidor. Hay dos texturas staging y copias adicionales en el auxiliar y Rust: es una referencia CPU, no zero-copy ni una mejora de latencia demostrada. Las métricas separan adquisición→publicación y copia CPU al mapping; no representan latencia visual de la Deck. Se mantiene `FrameStatus=DROPPED` porque este ensayo no entrega imágenes a un receptor de vídeo.

El comando `windowdeck-host --driver-frame-test` solicita 120 frames con timeout del auxiliar y un supervisor del host de 25 segundos. El auxiliar muestra únicamente en el monitor virtual una ventana de prueba sin foco, consciente del DPI: tres colores alternos y marcas blanca/negra, dibujados en un buffer y presentados juntos. Rust exige al menos 90 frames con muestras correctas y las tres fases observadas. Solo admite no coincidencias al principio, antes de la primera coincidencia; cualquier fallo posterior invalida el ensayo. Son muestras de píxeles, no una comparación de cada píxel del frame. No guarda imágenes, no transmite BGRA por red y no captura la pantalla física.

## Validación y recuperación

La autoprueba nativa comprueba layout, nombres de mapping, exclusión de slots en lectura y copia de filas con padding. Los tests Rust rechazan cabeceras inválidas, secuencias repetidas, clocks inválidos y patrones sin marcas o con muestras incoherentes. Build nativo, análisis WDK y catálogo pasan antes de la instalación.

Se instaló `oem100.inf` 0.1.0.8 con el certificado existente y se verificó esa selección mediante propiedades PnP durante cada ensayo. Se conserva 0.1.0.7 (`oem99.inf`) y su copia firmada en `target/windows-idd-test-v0.1.0.7/`. No se cambiaron GPU ni ajustes de arranque.

`test-driver-frames.ps1` pasó dos lecturas completas, terminación del host entre ellas, reconexión y conservación de los dispositivos físicos. La evidencia final es `target/driver-frames-test-e6de04bd6ed841e492972e954e98e28d/`. Cada lectura trasladó 491.520.000 bytes al host Rust, con estado de error del driver 0.

| Ensayo completo | Frames | Coincidencias tras arranque | Recepción | Adquisición→publicación media | Copia CPU media |
| --- | ---: | ---: | ---: | ---: | ---: |
| Antes de terminar el host | 120 | 100, tras 20 iniciales | 26,54 FPS | 9,310 ms | 188 µs |
| Tras reconectar | 120 | 104, tras 16 iniciales | 24,95 FPS | 9,130 ms | 183 µs |

Todas las muestras posteriores a la primera coincidencia fueron válidas. Estas tasas incluyen el patrón GDI, polling, copias y stdout; no demuestran un límite del driver ni 60 FPS de extremo a extremo. No son una comparación equivalente con WGC ni una medida de latencia de la Deck.

También pasan los 19 tests Rust, build, formato, Clippy, autoprueba nativa, tres ciclos automáticos y las pruebas existentes de fallo de encoder y terminación del host durante vídeo WGC (`target/auto-host-test-326aa1c461324bc392c728737d758db2/`). El streaming de la Deck sigue por la ruta WGC existente. Quedan el benchmark comparativo de las otras dos alternativas de IPC, conexión al encoder y suspensión/bloqueo/cambio de usuario.
