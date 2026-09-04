# ADR 0001: alcance del primer prototipo

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

El primer prototipo usará TCP y solo la biblioteca estándar de Rust para validar un handshake versionado y el envío de un patrón sintético de tamaño reducido.

El cliente lo presentará en la terminal. Una ventana, la captura real, H.264, QUIC y el driver IddCx quedan pospuestos hasta que esta conexión mínima sea estable y medible.

## Consecuencias

TCP puede acumular latencia y la terminal no representa el renderizado final. Esta ruta existe únicamente para comprobar el protocolo y el ciclo de conexión con el mínimo de piezas.
