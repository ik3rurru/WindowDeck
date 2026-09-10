# Rutas soportadas y diagnósticos del host

Fecha: 10 de septiembre de 2026. Estado: aceptado para consolidar el prototipo.
No decide el IPC definitivo ni acredita la ruta GPU en la Deck.

## Evidencia

La comparación CPU/WGC del 8 de septiembre favoreció CPU en la valoración del
usuario. Sus límites están en [testing.md](../testing.md): mismo perfil de
1280 × 800/60 Hz/16 Mbps, sin medición visual absoluta ni contenido idéntico.
La corrección del 10 de septiembre mantuvo el cliente instalado durante 207 y
54 segundos sin desconexiones inesperadas, incluyendo una pausa de dos segundos.
Se conserva la evidencia y la prueba de regresión de la cola codificada.

La integración GPU/FFmpeg/SDL del host tiene pruebas sintéticas y CI. La nueva
ruta completa sigue sin aceptación en la Deck; compilarla no autoriza seleccionarla
automáticamente. Los ADR 0011/0012 conservan su decisión provisional de IPC.

## Decisión

- `windowdeck-host --driver-h264 [DIRECCION]` es la ruta soportada del prototipo.
  Sin argumentos se usa esa misma ruta. Conserva CPU, libx264, MPEG-TS, resolución,
  bitrate, temporización y el límite de bloqueo de diez segundos de la corrección.
- El panel utiliza CPU por defecto. `scripts/WindowDeck.ps1 -Native` selecciona
  explícitamente el experimento GPU y requiere `native-media`. Se retira el
  switch redundante `-Legacy`; abrir el panel sin switches cumple esa función.
- Los ensayos y las rutas históricas se agrupan bajo `windowdeck-host diag`.
  El patrón, RGB332, ddagrab y WGC siguen disponibles para reproducir mediciones.
  Se congelan como referencias, conservando el código de captura y los ADR.
- Los flags antiguos se rechazan con instrucciones de migración. Scripts y
  documentación operativa se actualizan juntos; los registros históricos
  conservan la sintaxis y las versiones utilizadas en sus ensayos.
- La codificación continúa fuera del driver. La comparación de IPC pendiente
  no implica trasladar el encoder a UMDF.

## Consecuencias y aceptación

El README ofrece un recorrido de inicio. [development.md](../development.md)
contiene la tabla de comandos de desarrollo y su migración. El contrato de CLI
se prueba con diagnósticos válidos, argumentos inválidos y selecciones de captura
y códec. Las ayudas y errores deben terminar sin abrir red, captura ni GPU.

No se modifica la DLL del driver ni el protocolo de red. Los arneses de frames
y de vida del monitor usan los comandos nuevos y conservan su procedimiento.
Una retirada futura de código requerirá conservar sus mediciones y decisión.
