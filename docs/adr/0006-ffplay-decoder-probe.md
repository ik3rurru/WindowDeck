# ADR 0006: reproducción inicial con FFplay

> Actualización de consolidación (2026-09-10): FFplay sigue siendo el decoder de la ruta CPU soportada. El cliente multimedia nativo es experimental. Véanse los [ADR 0016](0016-supported-video-routes.md) y [0017](0017-media-packaging.md); el contenido siguiente documenta la prueba original.

- Estado: sustituido por ADR 0007
- Fecha: 2026-09-04

## Decisión

Delegar la primera prueba visible de H.264 en FFplay. El cliente valida el segmento MP4 recibido y lo sirve desde memoria en una dirección HTTP efímera limitada a `127.0.0.1`; esto permite las lecturas por rango que necesita el contenedor sin escribir contenido de pantalla en disco.

## Consecuencias

Validamos la decodificación con un componente multimedia maduro sin añadir bindings nativos a Rust. FFplay es una dependencia de ejecución detectada al usar `--h264-test`; la ruta continua deberá integrar un decoder con control explícito de colas, aceleración y presentación.
