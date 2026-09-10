# ADR 0007: emisión H.264 continua

> Actualización de consolidación (2026-09-10): la captura física se conserva como diagnóstico con `windowdeck-host diag h264-stream`. El arranque normal usa el driver y el encoder CPU; las rutas y el empaquetado actuales están en los [ADR 0016](0016-supported-video-routes.md) y [0017](0017-media-packaging.md). Los comandos y medidas siguientes son históricos.

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

Para validar el flujo continuo, el host ejecuta FFmpeg con `ddagrab`, ajusta la imagen a 1280 × 800 y codifica con `libx264` en modo `zerolatency`, 30 FPS y 4 Mbps. MPEG-TS sale por una tubería, cruza TCP en mensajes acotados del protocolo v3 y entra directamente en FFplay.

## Consecuencias

La reproducción comienza sin cerrar el encoder y no existe un archivo ni una cola de vídeo en Rust. FFmpeg y FFplay son dependencias de ejecución. El encoder software y TCP son límites deliberados del prototipo; se sustituirán después de medir latencia, carga y estabilidad por hardware real.
