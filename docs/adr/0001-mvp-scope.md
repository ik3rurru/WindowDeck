# ADR 0001: alcance del primer prototipo

- Estado: aceptado
- Fecha: 2026-09-04

## Decisión

El primer prototipo usará TCP y solo la biblioteca estándar de Rust para validar un handshake versionado y el envío de un patrón sintético de tamaño reducido.

El primer corte presentó el patrón en la terminal. La captura real, H.264, QUIC y el driver IddCx quedan pospuestos hasta que la conexión mínima sea estable y medible. La presentación posterior se describe en el ADR 0002.

## Consecuencias

TCP puede acumular latencia y la terminal no representa el renderizado final. Esta ruta existe únicamente para comprobar el protocolo y el ciclo de conexión con el mínimo de piezas.
