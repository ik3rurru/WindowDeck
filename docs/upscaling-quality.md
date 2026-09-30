# Calidad de imagen y reescalado

Investigación e implementación: 29 de septiembre de 2026. Estado: primera fase
implementada, sin dependencias nuevas. El usuario confirma que el escalado
ocupa el marco y pide mejorar la imagen pixelada. FSR y los nuevos filtros de
ampliación GPU siguen siendo propuestas para una fase posterior.

## Primera fase implementada

El cliente integrado acepta `--quality auto|deck|1080p|1440p`. Auto consulta la
salida SDL al abrir y anuncia un máximo acotado a 2560 × 1440; mantiene al menos
1280 × 800 para compatibilidad con extensión. Los perfiles manuales anuncian
1280 × 800, 1920 × 1080 y 2560 × 1440. Son límites, no resoluciones forzadas.
El host lee esas capacidades antes de seleccionar el vídeo, conserva proporción
y dimensiones pares, y no amplía una fuente menor. MPEG-TS mantiene su máximo
anterior. El formato de mensajes y la versión 3 del protocolo no cambian.

La captura de duplicación usa `scale_mode=bicubic`. Los objetivos iniciales de
bitrate son 16 Mbps hasta 1280 × 800 píxeles, 28 Mbps hasta 1920 × 1080 y 40 Mbps
por encima. Son objetivos del encoder, no una reserva de red ni máximos
instantáneos garantizados. Se pueden ajustar en el proceso del host mediante
`WINDOWDECK_MIRROR_BITRATE_MBPS` (entero 4–100). Son valores de partida sujetos
a validación de calidad y estabilidad, no un resultado de optimización.

El reproductor configura y limita el decoder para cada sesión, incluida la
reconexión. Rechaza dimensiones decodificadas distintas de las negociadas antes
de transferir o presentar la imagen. Conserva los límites de unidades de acceso.
La ampliación final sigue usando el filtro lineal existente.

Para cambiar de pantalla, conectar la TV antes de abrir el cliente. Un cambio
HDMI durante la sesión sigue ajustando la presentación; hay que reabrir el
cliente para elegir el nuevo perfil automático. En modo juego, Steam debe
exponer la resolución nativa externa. Se registra en `native_display` y
`native_quality`; el host registra fuente, tamaño, filtro y bitrate.

## Diagnóstico anterior a esta implementación

- `windowdeck-host/src/capture.rs::mirror_size` limita la fuente al rectángulo
  1280 × 800. Una fuente 2560 × 1440 o 3840 × 2160 se transmite a 1280 × 720.
- `gfxcapture` reduce en GPU antes de descargar la imagen. El comando no fija
  `scale_mode`; la ayuda del FFmpeg 9.0.2 empaquetado confirma que usa bilinear
  y que también dispone de bicubic. No requiere instalar otro FFmpeg.
- El vídeo usa libx264 ultrafast/zerolatency, 16 Mbps objetivo, hasta 60 FPS y
  YUV 4:2:0. El límite de resolución y la compresión son factores diferentes:
  aumentar bitrate puede reducir artefactos, pero no recuperar detalle que ya
  desapareció al reducir la imagen. El texto coloreado merece revisión aparte.
- `windowdeck-media/native/media.cpp` presenta una textura IYUV mediante SDL2
  con filtro lineal. A 4K, 1280 × 720 se amplía tres veces en cada dimensión.
  Esa relación explica una pérdida potencial importante de detalle, aunque
  todavía no identifica por sí sola la causa visual concreta del usuario.
- El cliente anuncia y valida un máximo de 1280 × 800; el decoder repite ese
  límite. El host calcula el tamaño antes de leer las capacidades del cliente.
  Subir solo una constante del host rompería compatibilidad.
- La ruta VAAPI descarga cada imagen decodificada a memoria y SDL la vuelve a
  cargar. A mayor resolución hay que medir también ese coste, además del encoder.

## Opciones

| Cambio | Utilidad | Límite o coste |
| --- | --- | --- |
| Comparar bicúbico con bilinear al reducir en el host | Puede reducir artefactos de líneas y texto en la Deck | Puede suavizar; requiere comparación del mismo contenido |
| Transmitir 1080p o 1440p en duplicación | Conserva más información para una TV | Más red y trabajo de codificación/decodificación |
| Lanczos o bicúbico en GPU en el cliente | Alternativa para ampliar escritorio y texto | Puede crear halos; debe compararse visualmente |
| FSR 1: EASU y RCAS regulable | Reconstrucción de bordes y nitidez al ampliar | No garantiza recuperar detalle perdido; puede acentuar compresión |
| FSR de Gamescope | Ensayo previo en modo juego | No cubre escritorio; necesita una superficie de entrada realmente menor que la salida |

[FFmpeg documenta los filtros de gfxcapture](https://ffmpeg.org/ffmpeg-filters.html#gfxcapture)
y los algoritmos de [libswscale](https://ffmpeg.org/ffmpeg-scaler.html).
La opción `best` del hint de SDL2 está documentada como equivalente a `linear`:
[SDL_HINT_RENDER_SCALE_QUALITY](https://wiki.libsdl.org/SDL2/SDL_HINT_RENDER_SCALE_QUALITY).
Cambiar ese texto no aporta un filtro avanzado.

[FSR 1](https://gpuopen.com/fidelityfx-superresolution/) trabaja con una imagen,
sin profundidad ni vectores de movimiento, y publica shaders con licencia MIT.
Es viable como candidato para nuestro vídeo decodificado. Para una salida 4K,
AMD documenta entradas 1440p en modo Quality y 1080p en Performance; 720p → 4K
supone una ampliación mayor que esos modos. No extrapolar las cifras de coste
de otras GPUs a la Steam Deck.

[FSR 2](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/super-resolution-temporal/)
necesita datos del renderizado, incluidos profundidad y movimiento. Nuestra
captura de escritorio no los proporciona; no es una integración directa.

[Gamescope](https://github.com/ValveSoftware/gamescope) ofrece `-F fsr` y
resoluciones separadas de entrada y salida. Si WindowDeck ya entrega una imagen
ampliada a 4K, ese escalado previo impide evaluar correctamente FSR sobre los
720p originales. El ensayo debe controlar quién realiza la única ampliación.

## Implementación recomendada

### 1. Conservar más detalle en la transmisión

1. Registrar fuente, vídeo y salida real. Comparar texto pequeño, diagonales,
   desplazamiento y vídeo antes de elegir filtros. En modo juego, comprobar
   qué resolución ofrece Gamescope al cliente.
2. Añadir perfiles de duplicación: Deck, 1080p y 1440p. Elegir una resolución
   proporcional limitada por fuente, perfil y capacidades del receptor.
   No ampliar en el host una fuente menor ni cambiar los modos del driver.
3. Reordenar la negociación del host para leer capacidades antes de elegir
   resolución. Los campos actuales permiten tamaños mayores sin cambiar el
   formato del protocolo. Conservar el modo actual para receptores antiguos.
4. Ampliar los límites del cliente de forma acotada. Pasar el tamaño negociado
   al reproductor y validar cada sesión, incluidos reconexiones, decoder,
   asignaciones de memoria y tamaño máximo de unidades de acceso.
5. Hacer configurable el bitrate por perfil; determinar valores con mediciones.
   Mantener baja latencia y probar encoder hardware si la CPU limita 1440p.
   El selector de encoder existente no acredita su rendimiento en este PC.
6. Comparar el bicúbico disponible en `gfxcapture` con el bilinear actual.
   Para la pantalla integrada, priorizar una buena reducción a su tamaño útil.

### 2. Añadir un filtro de ampliación en el cliente

1. Prototipo de presentación GPU con conversión YUV → RGB y salida al tamaño
   del área visible, conservando proporción y bandas cuando corresponda.
2. Comparar lineal, Lanczos/bicúbico y FSR 1 con nitidez moderada ajustable.
   SDL_Renderer no ofrece en la interfaz usada un pase de shaders propio:
   esta fase requiere un backend gráfico, por ejemplo SDL + OpenGL, shaders
   empaquetados y mantenimiento de la ruta SDL actual como alternativa.
3. Aplicar FSR solamente al ampliar; usar reducción adecuada al disminuir y
   evitar filtros extra en 1:1. Evitar una segunda ampliación de Gamescope.
4. Rehacer recursos al conectar HDMI, cambiar salida o perder el dispositivo.
   Conservar reconexión y comportamiento de Detener.
5. Medir antes de adoptar: tiempo de GPU, copias de memoria, frames descartados
   y latencia. Los shaders se distribuirían con el cliente; la propuesta no
   requiere un servicio ni un driver adicional en Windows.

Primera combinación a evaluar en TV 4K: 1440p transmitidos y ampliación GPU a
4K. Mantener 1080p como perfil alternativo si red o encoder no sostienen 1440p.
La transmisión 4K nativa puede estudiarse después si se necesita más fidelidad.
No hay todavía resultados que permitan prometer 60 FPS o una latencia concreta.

## Criterios de aceptación

- Comparación del mismo contenido en Deck y TV: letras legibles, diagonales,
  halos, ruido de compresión y detalle en movimiento; valoración del usuario.
- Ensayos de ambas negociaciones, receptor antiguo, perfiles y reconexión con
  cambio de tamaño. Mantener límites explícitos ante vídeo inválido.
- Medición de captura/encoder/red/decoder/presentación para localizar el coste;
  los tiempos internos no equivalen a una medición física de latencia completa.
- Conexión/desconexión HDMI, cambio de salida y modos juego/escritorio. No
  adoptar por defecto un filtro que empeore claridad o estabilidad.
