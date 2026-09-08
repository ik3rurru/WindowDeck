# ADR 0012: comparación de frames con texturas D3D11 compartidas

- Fecha: 2026-09-07
- Estado: prototipo implementado y validado en UMDF; comparación CPU/GPU realizada. Decisión de integración provisional, sin encoder nuevo.

## Diseño

La versión 0.1.0.9 añade `--gpu-frame-broker` y `windowdeck-host --gpu-frame-test`. El broker crea tres texturas BGRA 1280 × 800 con NT handles y keyed mutex, nombres globales derivados de un GUID por sesión y la misma ACL restringida que la referencia CPU. Conserva sus handles hasta retirar el monitor. La selección de GPU física de bajo consumo se comparte con el driver; el lector usa su LUID exacto. Una GPU de render distinta debe provocar un error de apertura, nunca una copia implícita entre adaptadores.

El mapping conserva el layout fijo de 12.288.448 bytes para reutilizar el ensayo, pero en versión 2 sus arrays de píxeles no se usan: solo estados y metadatos. Sus ocho bytes reservados contienen el LUID. No es el formato final del IPC.

El driver abre las texturas una vez. Reclama únicamente slots libres, adquiere la clave 0 con timeout cero, envía una copia GPU y libera la clave 1 antes de publicar metadatos. Si todos están ocupados, descarta la actualización sin esperar al lector. El lector reclama el slot, adquiere la clave 1, copia a un staging propio y devuelve la clave 0. La lectura CPU se limita al auxiliar para verificar el mismo patrón en Rust. No hay readback ni copia de píxeles CPU dentro del driver en este modo. Sigue existiendo una copia GPU: no se declara zero-copy.

Se exige `AcquireSync == S_OK`: `WAIT_TIMEOUT` y `WAIT_ABANDONED` son resultados positivos que `SUCCEEDED` no distingue. Un abandono o fallo termina el ensayo; una nueva sesión crea recursos nuevos. La recreación de swap-chain dentro de una misma sesión se rechaza expresamente en este prototipo, sin reutilizar claves de una generación anterior.

Contratos oficiales: [CreateSharedHandle](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgiresource1-createsharedhandle), [OpenSharedResourceByName](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_1/nf-d3d11_1-id3d11device1-opensharedresourcebyname), [AcquireSync](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync).

## Comparación y límites

Ambas rutas usan el mismo patrón GDI, 120 frames, validación estricta de muestras BGRA, envío binario por stdout y lector Rust. Se registran:

- adquisición → publicación: en CPU incluye readback y copia al mapping; en GPU termina al liberar la clave, sin certificar la finalización GPU. No comparar estas cifras como tiempos de trabajo equivalentes;
- adquisición → píxeles disponibles en el lector CPU: incluye espera en slots y readback del auxiliar en GPU; es un punto final comparable;
- tiempo de copia/lectura del auxiliar y FPS recibidos por Rust: incluyen costes del arnés, no representan latencia visual ni rendimiento de un encoder GPU.

`test-driver-frames.ps1` acepta `-Gpu`; ejecuta dos lecturas completas, termina el host entre ambas y comprueba reconexión, retirada y conservación de dispositivos físicos. La regresión CPU se ejecuta sin ese parámetro. Deben estar iniciados los brokers correspondientes y el monitor inactivo.

La comparación no cambia el streaming WGC a la Deck. La decisión del encoder requiere contrastar la complejidad de alojarlo en UMDF frente a consumir texturas en un proceso de usuario. No se deduce una mejora de latencia visual de esta prueba.

## Resultados locales

Instalado y seleccionado por PnP: **0.1.0.9, `oem101.inf`**. Se conserva el paquete firmado 0.1.0.8 en `target/windows-idd-test-v0.1.0.8/`, además de los respaldos anteriores. Certificado existente; no se modificaron controladores GPU ni arranque y no se reinició.

| Ruta / lectura | Frames válidos tras arranque | Recepción Rust | Adquisición → publicación | Adquisición → lector CPU | Copia/lectura del auxiliar |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU / primera | 101 de 120 | 24,60 FPS | 9,380 ms | 16,928 ms | 0,182 ms |
| CPU / reconexión | 101 de 120 | 24,36 FPS | 9,341 ms | 16,982 ms | 0,186 ms |
| D3D11 / primera | 100 de 120 | 25,70 FPS | 0,102 ms | 23,975 ms | 15,106 ms |
| D3D11 / reconexión | 104 de 120 | 24,08 FPS | 0,100 ms | 23,893 ms | 15,242 ms |

Las columnas temporales son medias de los 120 frames, incluidos los iniciales. Todas las muestras posteriores a la primera coincidencia fueron válidas; las tres fases del patrón estuvieron presentes. Ambos ensayos pasaron terminación del host entre lecturas, reconexión, retirada y conservación del monitor físico y las dos GPU. No hubo errores reportados por el driver. La ruta CPU puede reemplazar slots listos; la GPU conserva los tres hasta que el lector devuelve la clave, por lo que las políticas de cola tampoco son idénticas. Dos lecturas por ruta no constituyen un benchmark estadístico ni prueban 60 FPS.

Evidencias:

- D3D11: `target/driver-frames-test-0cc388090b7f45f3ba9a996a7f7e7b9d/`.
- CPU: `target/driver-frames-test-fb0a9b125cbd435e817bb0bb5c527e1f/`.
- Regresión WGC, fallo del encoder y terminación del host: `target/auto-host-test-5ba1512fbc904a6d84e9e0a3b1ddd16f/`.
- Tres ciclos automáticos: `target/windows-idd/auto-display-test.log`.
- Brokers: `target/gpu-probe-ba6b0517407d40eab3c202d520011c16/`.

Un primer intento no llegó a activar el monitor porque los brokers habían terminado junto con su lanzador. Se iniciaron como procesos independientes y la aceptación posterior indicada arriba pasó; ese primer fallo no fue un error de D3D11.

## Cómo alimentar el encoder

La publicación GPU reduce el trabajo CPU del driver, pero en este arnés desplaza el readback al auxiliar y no mejora la disponibilidad de píxeles CPU. **Para el siguiente prototipo con libx264 se propone alimentar el encoder desde los frames CPU directos**, en un proceso de usuario, conservando parámetros y WGC como ruta de referencia. El ensayo debe medir el flujo codificado completo antes de cambiar la ruta usada por la Deck; no se elige aún un IPC definitivo.

Las texturas compartidas quedan como alternativa comprobada para un futuro encoder que acepte superficies GPU en el mismo adaptador. Faltan consumo por ese encoder, conversión de formato y recuperación de device-loss/swap-chain; la aceleración hardware continúa aplazada.

Codificar dentro de UMDF es una alternativa arquitectónica: [IddCx entrega superficies DirectX para procesar la imagen](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview). Nuestra valoración es que alojar allí el encoder acoplaría sus dependencias, fallos y actualizaciones al paquete del driver. Por ahora se prefiere el encoder fuera de UMDF para aprovechar el control de procesos existente. **No se ha implementado ni medido un encoder dentro del IDD**, y no se atribuye una ventaja de rendimiento a ninguna ubicación sin esa medición.
