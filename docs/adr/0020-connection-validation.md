# Validación antes de activar la pantalla

Fecha: 13 de septiembre de 2026. Estado: implementado; pendiente de aceptación
en Windows/Steam Deck con la red de las pruebas fallidas.

## Motivo

Las pruebas del usuario con 0.2.1 producen altas y bajas continuas del monitor.
Los registros locales de `launcher-3bec9807e65c785afbe2d9414f4dae03` y
`launcher-fd7874c4c4879c71a737f49f97d09b9d` muestran negociación correcta,
primeras unidades H.264 enviadas y sesiones cortas que terminan con timeout
TCP 10060 o cola caducada. Los errores del muxer aparecen durante el cierre
de la tubería. Estos registros localizan el ciclo, pero no permiten atribuir
el atasco exclusivamente a la red, al decoder o al sistema gráfico de la Deck.

El host activaba el monitor tras negociar capacidades, antes de comprobar al
receptor. El cliente integrado inicializaba SDL después de recibir Start.
Los límites de 250 ms trataban un atasco breve como una sesión irrecuperable;
la reconexión cada segundo repetía la activación del monitor.

## Decisión

- Añadir una capacidad opcional `0x80` al campo existente de capacidades del
  protocolo 3. No representa un códec. Solo clientes que anuncian ese bit reciben
  `ConnectionProbe` (tipo 11); se conserva compatibilidad con ambos extremos
  anteriores. Una combinación con un extremo antiguo no acredita la validación.
- Enviar SessionConfig antes de cualquier concesión o auxiliar de captura.
  Los clientes nuevos validan formato y responden a ocho ráfagas de 500 kB
  de datos sintéticos. Cada ráfaga consta de ocho mensajes de 62.500 bytes,
  seguidos por Ping/Pong con un nonce ligado a la sesión y a la ronda. Un Pong
  solo se emite tras recibir y comprobar toda la ráfaga. Los datos no se muestran
  ni se pasan al decoder.
- Distribuir las ráfagas con intervalos de 250 ms: aproximadamente dos segundos
  y 4 MB de prueba, con una carga nominal de 16 Mbps. Cada ronda dispone de
  750 ms y el conjunto de cuatro segundos; lecturas y escrituras parciales
  consumen el mismo plazo. Es una prueba breve de transporte, no una medición
  de capacidad sostenida ni una garantía frente a errores posteriores.
- Inicializar el reproductor integrado antes de conectar. Un fallo al crear
  SDL, el renderer o el decoder no llega a activar el monitor. La negociación
  conserva las rutas MPEG-TS/FFplay anteriores; la prueba sintética de tráfico
  no acredita que se haya decodificado un fotograma.
- Pasar por `Validating` y `Ready` antes de `Streaming`. El panel muestra
  comprobación, activación y vídeo por separado; `streaming` se publica tras
  enviar la primera unidad/chunk, sin afirmar recepción o presentación física.
- Conservar colas de dos elementos y dar dos segundos a los atascos del
  transporte H.264 por unidades de acceso. El límite de presentación de
  imágenes ya decodificadas sigue siendo 250 ms. No se omiten fragmentos H.264
  dependientes para continuar la misma sesión. MPEG-TS conserva diez segundos.
- En el cliente integrado, una recuperación de paquetes antiguos solo cierra
  el socket de esa sesión, aunque el receptor ya negocie otra. Los eventos de
  control no caducan junto a los paquetes ni se descartan al vaciar una cola.
- Espaciar reintentos H.264 a 1, 2, 4, 8, 16 y 30 segundos. Negociar otra
  conexión no reinicia la espera; una sesión de al menos 30 segundos sí.
- Tras tres fallos consecutivos de arranque o atasco en sesiones de menos de
  30 segundos, el host cierra la escucha y retira su anuncio. Mantiene el panel
  disponible con instrucciones para Detener/Iniciar. Las validaciones rechazadas
  no cuentan como activaciones; EOF, cierre de tubería y reset del cliente no
  se consideran fallos de arranque. Un cierre normal o sesión estable reinicia
  el contador. Esta protección también limita fallos de clientes anteriores.

No se modifican el driver, resolución, FPS ni bitrate de codificación. La ruta
GPU y los fallos que solo aparecen al activar el driver necesitan una prueba
real. Actualizar host y cliente es necesario para la validación completa.

## Comprobación

Pruebas con sockets locales: carga completa, nonces incorrectos, ráfagas
incompletas, respuestas ausentes o partidas, cancelación y restauración de
timeouts. Las tres rutas automáticas rechazan una validación fallida antes de
iniciar captura o concesión. Se cubren el freno de fallos repetidos, el cierre
normal y la espera creciente.

La regresión H.264 mantiene todos los fragmentos tras una pausa de 350 ms a
mitad de un fotograma. Las colas aceptan una pausa de 800 ms y siguen rechazando
datos que exceden el límite de atasco. El arnés del cliente integrado comprueba
fallo de renderer antes de conectar, validación en cada reconexión, compatibilidad
con hosts anteriores, decodificación, pérdida de un fragmento y Stop.

Resultados ejecutados y aceptación pendiente en [testing.md](../testing.md).
