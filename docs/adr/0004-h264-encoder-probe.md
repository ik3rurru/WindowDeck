# ADR 0004: prueba inicial del encoder H.264

> Actualización de consolidación (2026-09-10): esta prueba histórica de Media Foundation se ejecuta ahora con `windowdeck-host diag encode`. El encoder y empaquetado actuales se describen en el [ADR 0017](0017-media-packaging.md). Las mediciones de este documento corresponden al prototipo original.

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

Validar primero Media Foundation mediante el encoder que ya expone `windows-capture`. La prueba entrega directamente texturas D3D11, codifica 60 frames H.264 en un contenedor MPEG-4 mantenido en memoria y no guarda contenido de pantalla.

## Consecuencias

Confirmamos captura y codificación antes de cambiar el protocolo o el cliente. El segmento resultante puede enviarse ahora con el modo de diagnóstico descrito en el ADR 0005, pero todavía no representa una ruta continua de baja latencia. Los keyframes, el escalado a 1280 × 800 y la decodificación en SteamOS quedan para los siguientes incrementos.
