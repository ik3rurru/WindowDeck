# ADR 0005: paquetes de vídeo codificado

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

La versión 3 del protocolo negocia el códec por sesión y añade fragmentos de vídeo con ID de sesión, número de frame, timestamp de captura, posición, total de fragmentos e indicador de keyframe. Cada mensaje sigue limitado a 64 KiB.

## Consecuencias

El transporte no depende de una implementación concreta del encoder y puede llevar H.264 sin relajar sus límites de memoria. El flujo visual continúa negociando RGB332; un modo separado negocia H.264 y reensambla un segmento MP4 con un límite de 4 MiB.
