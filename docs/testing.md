# Pruebas y mediciones

## Línea base H.264 local

Medición del 4 de septiembre de 2026 con host y cliente en el mismo PC Windows, monitor fuente de 2560 × 1440 y salida H.264 de 1280 × 800, 30 FPS y 4 Mbps configurados:

| Medida | Resultado |
| --- | ---: |
| Primer paquete desde el arranque de FFmpeg | 224 ms |
| Ritmo del encoder tras 5 s | 29,98 FPS |
| Velocidad del encoder tras 5 s | 0,999× |
| Datos recibidos en unos 5 s | 1,70 MB en 173 paquetes |
| CPU de FFmpeg, normalizada sobre 16 procesadores lógicos | 8,6 % |
| Memoria de trabajo de FFmpeg | 110,7 MiB |

La tasa efectiva varía con el contenido de pantalla aunque el límite sea 4 Mbps. Esta prueba de loopback confirma que el encoder mantiene tiempo real, pero no mide red, Steam Deck ni latencia visual de extremo a extremo.

## Repetir en dos equipos

1. Ejecutar el host con `cargo run -p windowdeck-host -- --h264 1 0.0.0.0:48150`.
2. Ejecutar el cliente con `cargo run -p windowdeck-client -- IP_DEL_PC:48150 --h264-test --fullscreen`.
3. Mantener la sesión al menos 30 segundos y conservar los eventos `h264_encoder_metrics`, `h264_send_metrics` y `h264_receive_metrics`.
4. Repetir por Ethernet y Wi-Fi sin cambiar resolución, FPS ni contenido.
5. Para latencia visual, grabar simultáneamente ambas pantallas con un contador visible; no restar timestamps de relojes no sincronizados.
