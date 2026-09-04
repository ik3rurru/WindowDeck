# ADR 0002: ventana del cliente

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

El cliente mostrará el patrón sintético en una ventana de `winit` mediante un buffer de CPU de `softbuffer`. El hilo de red conservará un único frame pendiente y reemplazará el anterior cuando la interfaz no pueda seguir el ritmo.

## Consecuencias

Esta ruta permite validar ventana, escalado y descarte de frames en Windows, X11 y Wayland sin introducir todavía una API gráfica de GPU. Se sustituirá cuando la decodificación de vídeo requiera presentar superficies aceleradas directamente.
