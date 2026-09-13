# Reproductor integrado con la captura CPU

Fecha: 12 de septiembre de 2026. Estado: aceptado para la ruta CPU del prototipo.
Revisa el transporte de la ruta CPU de ADR 0016; conserva captura, driver y perfil.

## Motivo

Durante la aceptación del lanzador Rust, el usuario observó un retraso excesivo
con el cliente FFplay 0.1.0. Limitar decoder y filtros a un hilo mejoró la
respuesta, pero siguió siendo insuficiente, también tras un ensayo de temporización
inmediata. Las métricas del encoder no demostraban baja latencia de presentación.

El cliente integrado ya admite H.264 por unidades de acceso y presenta los
fotogramas decodificados sin la temporización de FFplay. Hasta ahora el host
CPU negociaba siempre MPEG-TS y ese cliente utilizaba su reproductor de respaldo.

## Decisión

- El host CPU sigue usando `--frame-broker`, frames BGRA y FFmpeg/libx264 externo,
  con 1280 × 800, 60 Hz y 16 Mbps. No selecciona la ruta GPU del host.
- Negocia `H264Frames` cuando el cliente lo admite. FFmpeg inserta un AUD por
  unidad de acceso y repite SPS/PPS en IDR. El host delimita Annex B respetando
  lecturas partidas y fragmenta cada unidad con el protocolo 3 existente.
- Una sesión comienza con SPS, PPS e IDR. Se rechazan unidades vacías, cabeceras
  inválidas y unidades mayores de 4 MiB. Al detenerse se descarta la última unidad
  incompleta y se envía Stop; no se continúa un flujo omitiendo fragmentos dependientes.
- El lector conserva dos chunks de salida. En el transporte nuevo, los bytes
  encolados y los bloqueos de envío tienen un margen de 250 ms; la espera inicial
  del encoder conserva diez segundos. Estos márgenes no acotan la latencia total.
- La tubería BGRA no conserva la marca QPC del driver a través de FFmpeg:
  `captured_micros=0` significa que no está disponible. No se inventa un tiempo
  de captura a partir de la hora de envío. Delimitar la salida requiere leer
  el siguiente AUD, que añade hasta otro intervalo de fotograma en funcionamiento normal.
- Clientes anteriores siguen negociando MPEG-TS con su margen de diez segundos.
  El cliente FFplay del árbol limita decoder y filtros a un hilo; los ensayos
  `setpts=0` y `avioflags=direct` no pasan al producto.

La compilación básica del host también admite este transporte: `native-media`
es necesario para el reproductor integrado del cliente, no para delimitar la
salida de FFmpeg en el host. El modo GPU del host conserva su selección explícita.

## Comprobación

Las pruebas cubren delimitadores partidos en cada frontera de lectura,
fragmentación/reconstrucción, comienzo inválido, límite de tamaño y parada
durante una escritura incompleta. Un ensayo con FFmpeg genera y decodifica
120 imágenes móviles con IDR y fotogramas dependientes, verificando orden,
integridad de bytes y contenido cambiante. Se ejecuta expresamente en CI nativo.

El usuario aceptó la latencia y el retorno de ventanas al detenerse. Se verificó
la retirada del monitor tras terminar el panel inesperadamente, la reconexión
de la misma instancia de la Deck al reabrirlo y la parada posterior. Evidencia
y pendientes en [testing.md](../testing.md); las pruebas sintéticas no sustituyen
la aceptación visual ni acreditan una instalación limpia o una sesión prolongada.
