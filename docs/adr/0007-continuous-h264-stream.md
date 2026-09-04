# ADR 0007: emisión H.264 continua

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

Para validar el flujo continuo, el host ejecuta FFmpeg con `ddagrab`, ajusta la imagen a 1280 × 800 y codifica con `libx264` en modo `zerolatency`, 30 FPS y 4 Mbps. MPEG-TS sale por una tubería, cruza TCP en mensajes acotados del protocolo v3 y entra directamente en FFplay.

## Consecuencias

La reproducción comienza sin cerrar el encoder y no existe un archivo ni una cola de vídeo en Rust. FFmpeg y FFplay son dependencias de ejecución. El encoder software y TCP son límites deliberados del prototipo; se sustituirán después de medir latencia, carga y estabilidad por hardware real.
