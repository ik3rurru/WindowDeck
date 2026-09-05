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

## Línea base en Steam Deck por Wi-Fi

Prueba visual del 4 de septiembre de 2026 con el Flatpak del commit `cc113ec`, durante 179 segundos:

| Medida | Resultado |
| --- | ---: |
| Encoder | 29,99 FPS, velocidad 1,0× |
| Envío y recepción | 4,20 Mbps estables |
| Fluidez | Estable, con tirones en movimientos muy rápidos |
| Imagen en movimiento | Limpia; recuperación inmediata al detenerse |
| Latencia visual percibida | Molesta |

Prueba del commit `0f55519` a 60 FPS y 4 Mbps, durante 227 segundos por la misma red Wi-Fi:

| Medida | Resultado |
| --- | ---: |
| Encoder | 59,24 FPS, velocidad 1,0× |
| Envío y recepción | 4,23 Mbps estables |
| Fluidez | Mejora visible respecto a 30 FPS |
| Imagen en videojuegos | Borrosa en algunos movimientos rápidos |
| Latencia visual percibida | Notable; sin mejora respecto a la prueba anterior |

La prueba posterior del commit `99c8bbc`, con un objetivo de 8 Mbps, mostró una mejora perceptible de latencia. Persistieron artefactos en fondos de videojuego en movimiento; faltan las métricas de esa sesión para cuantificar el bitrate efectivo.

La prueba del commit `84e70f3`, con un objetivo de 12 Mbps, mejoró claramente la calidad percibida. Se continúa con 16 Mbps para localizar el punto a partir del cual aumentar el bitrate deja de aportar una mejora visible.

La prueba del commit `7749edf`, con un objetivo de 16 Mbps, mantuvo 59,25 FPS durante 122 segundos y entregó unos 14,5 Mbps tanto en el host como en el cliente. La imagen se percibió muy buena y no aparecieron indicios de saturación de red; se conserva este bitrate mientras se prueba reducir el buffering del reproductor.

## Comparar el buffering de FFplay

El cliente permite comparar la configuración anterior (`7749edf`, A) con el perfil reducido corregido (B) usando el mismo binario. El host se mantiene a 1280 × 800, 60 FPS y 16 Mbps. Solo cambian las opciones `-max_delay 0 -sync ext` del reproductor. Se ha retirado `-avioflags direct`, añadido en `40d2bd6`, por los errores de arranque descritos debajo.

Compilar el código actual en ambos equipos, o instalar en la Deck un Flatpak construido con esta opción; los Flatpak anteriores no reconocen `--ffplay-baseline`.

En Windows, mantener este host durante ambas pruebas:

```powershell
cargo run -p windowdeck-host -- --h264 1 0.0.0.0:48150 2>&1 | Tee-Object -FilePath host-comparison.log
```

En la Deck, ejecutar A durante al menos dos minutos y cerrar su ventana antes de iniciar B:

```bash
flatpak run io.github.ik3rurru.WindowDeck IP_DEL_PC:48150 --h264-test --fullscreen --ffplay-baseline 2>&1 | tee client-A.log
```

```bash
flatpak run io.github.ik3rurru.WindowDeck IP_DEL_PC:48150 --h264-test --fullscreen 2>&1 | tee client-B.log
```

Si se compila desde el repositorio, sustituir `flatpak run io.github.ik3rurru.WindowDeck` por `cargo run -p windowdeck-client --`. El evento `h264_player_started` debe indicar `buffering="baseline"` en A y `buffering="reduced"` en B.

Usar el mismo contenido en movimiento y la misma conexión. Esperar diez segundos al principio de cada sesión antes de tomar muestras; repetir en orden B → A para comprobar que la diferencia no depende del orden. Anotar tirones, congelaciones, calidad percibida y FPS junto con la latencia visual medida con el procedimiento siguiente. Los tiempos del primer paquete solo describen el arranque de la tubería.

| Conexión | Perfil | Duración | FPS encoder | Mbps recibidos | Latencia mediana | P95 | Incidencias |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Wi-Fi | A: anterior | pendiente | pendiente | pendiente | pendiente | pendiente | pendiente |
| Wi-Fi | B: reducido | pendiente | pendiente | pendiente | pendiente | pendiente | pendiente |
| Ethernet | A: anterior | pendiente | pendiente | pendiente | pendiente | pendiente | pendiente |
| Ethernet | B: reducido | pendiente | pendiente | pendiente | pendiente | pendiente | pendiente |

Conservar B si reduce de forma repetible la latencia sin introducir tirones o congelaciones. Sin grabación a cámara lenta, registrar solo una valoración subjetiva y dejar la latencia numérica pendiente. Después de elegir el perfil, mantener una sesión de una hora para comprobar que no aumenta el retraso ni la memoria de forma progresiva.

### Comprobación local del 5 de septiembre de 2026

Con FFmpeg y FFplay 9.0.1 de Gyan en Windows, se probaron ambos perfiles durante doce segundos cada uno, con el host y el cliente conectados por loopback y FFplay usando el controlador SDL `dummy` y renderizado software. Ambos recibieron datos; el perfil de `40d2bd6` produjo errores `non-existing PPS 0 referenced` y `no frame!` al arrancar. Recibir paquetes por sí solo no confirma una decodificación correcta.

Para aislar la opción responsable se generó un único MPEG-TS sintético de cuatro segundos (`testsrc2`, 1280 × 800, 60 FPS, `libx264`, `ultrafast`, `zerolatency`, 16 Mbps, sin B-frames y GOP de 60). Se reprodujeron los mismos bytes por `pipe:0` con las opciones comunes del cliente y `-vf showinfo`:

| Opciones añadidas al perfil A | Frames decodificados | Primer PTS decodificado | Mensajes de error PPS / frame |
| --- | ---: | ---: | ---: |
| Ninguna | 180 | 1 s | 0 |
| `-avioflags direct -max_delay 0 -sync ext` | 180 | 1 s | 34 |
| `-max_delay 0 -sync ext` | 180 | 1 s | 0 |
| `-avioflags direct` | 180 | 1 s | 34 |

El recuento incluye los mensajes explícitos, sin expandir las repeticiones agrupadas por FFplay. La prueba reproduce los errores al añadir `direct` y los elimina al retirarlo; por eso el perfil B conserva solo `-max_delay 0 -sync ext`. Todos los perfiles comenzaron en el keyframe de un segundo: ese PTS no mide latencia visual ni demuestra que B sea más rápido. Falta la comparación en Steam Deck y red real.

Tras corregir el cliente se repitió la prueba de loopback durante doce segundos por perfil: A y B recibieron vídeo sin errores PPS/frame. El escritorio cambió entre sesiones, por lo que sus tasas de datos no sirven para comparar rendimiento. El cierre forzado del reproductor al acabar la comprobación genera un `client_failed` esperado; no se validó aquí el cierre desde una ventana visible.

## Repetir en dos equipos

1. Ejecutar el host con `cargo run -p windowdeck-host -- --h264 1 0.0.0.0:48150`.
2. Ejecutar el cliente con `cargo run -p windowdeck-client -- IP_DEL_PC:48150 --h264-test --fullscreen`.
3. Mantener la sesión al menos 30 segundos y conservar los eventos `h264_encoder_metrics`, `h264_send_metrics` y `h264_receive_metrics`.
4. Repetir por Ethernet y Wi-Fi sin cambiar resolución, FPS ni contenido.
5. Confirmar que el encoder se mantiene cerca de 60 FPS, que su velocidad no baja de `1.0x` de forma sostenida y que los bytes recibidos siguen creciendo sin pausas.

## Medir latencia visual

1. Mostrar en Windows un cronómetro con milisegundos o alternar repetidamente una ventana entre blanco y negro.
2. Grabar a la vez el monitor del PC y la Steam Deck con una cámara a 120 o 240 FPS.
3. Contar los fotogramas entre el cambio visible en el PC y el mismo cambio en la Deck. La latencia es `fotogramas × 1000 / FPS de la cámara`; a 240 FPS cada fotograma equivale a 4,17 ms.
4. Medir al menos 20 cambios y anotar la mediana y el percentil 95, primero por Ethernet y después por Wi-Fi.

Este método incluye captura, codificación, red, decodificación y ambas pantallas. No se deben restar timestamps de equipos cuyos relojes no estén sincronizados.

| Conexión | Muestras | Latencia mediana | P95 | FPS encoder | Incidencias |
| --- | ---: | ---: | ---: | ---: | --- |
| Ethernet | 20 | pendiente | pendiente | pendiente | pendiente |
| Wi-Fi | 20 | pendiente | pendiente | pendiente | pendiente |
