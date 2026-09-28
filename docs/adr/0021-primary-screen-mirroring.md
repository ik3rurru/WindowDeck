# Duplicar la pantalla principal sin monitor virtual

Fecha: 28 de septiembre de 2026. Estado: implementado; el usuario confirma
funcionamiento satisfactorio. La comparación cuantitativa de rendimiento
con extensión sigue pendiente.

## Motivación

Ofrecer una alternativa a extender el escritorio: transmitir lo que ya muestra
el PC sin activar un monitor virtual. El cliente debe funcionar igual. Retirar
el driver y la transferencia de sus frames puede reducir trabajo, pero no
demuestra por sí mismo menor latencia: captura, escalado, codificación, red y
presentación siguen contribuyendo al resultado.

## Decisión

- El panel conserva **Extender escritorio** por defecto y añade **Duplicar
  pantalla principal**. La selección se bloquea durante la sesión. El switch
  `--mirror` del lanzador permite preseleccionarla; el del host inicia esa ruta.
- La fuente se resuelve con `Monitor::primary()` después de validar la conexión
  y se pasa su HMONITOR a `gfxcapture`. No se supone que la primera salida DXGI
  de la primera GPU sea la principal. Esta entrega no incluye selector de otros
  monitores ni captura de un escritorio combinado.
- FFmpeg realiza el escalado y las bandas en GPU con `width=1280:height=800`
  y `resize_mode=scale_aspect`, antes de descargar BGRA. Se conserva libx264,
  16 Mbps, hasta 60 FPS y el transporte integrado por unidades de acceso, con
  MPEG-TS para clientes anteriores. No se modifica el cliente ni el protocolo.
- La negociación con un cliente antiguo conserva la fuente física. Solo la
  ruta experimental del driver GPU cambia al driver CPU para MPEG-TS.
- Duplicar no verifica ni inicia `windowdeck-display.exe`, no adquiere una
  pantalla virtual y no requiere instalar el driver. El gestor elevado solo
  prepara las reglas de red existentes para LAN privada y conserva su canal de
  cancelación. No arranca los brokers de frames CPU/GPU.
- La captura conserva el cierre por desconexión, Detener, monitor cambiado o
  encoder detenido. Un cambio de principal termina esa sesión; una nueva
  conexión resuelve la principal actual. Los Job Objects del lanzador siguen
  recogiendo host y encoder al cerrar inesperadamente el panel.

## Alcance y validación

Las pruebas cubren la selección de modo, compatibilidad con clientes antiguos,
validación antes de capturar, ausencia de requisitos del driver y estados del
panel. `scripts/test-mirror-host.py` captura el escritorio real por loopback,
decodifica H.264 y MPEG-TS, reconecta y detiene el host. Mantiene el vídeo en
memoria y fuerza una ruta de auxiliar inexistente.

No acredita latencia en la Deck, estabilidad prolongada, cambio de usuario,
HDR ni bloqueo/reanudación. La comparación con extensión debe usar el mismo
contenido, red, bitrate y cliente, midiendo latencia visual y carga CPU/GPU.
Las antiguas mediciones de WGC sobre el monitor virtual no evalúan esta ruta.

La selección de HMONITOR y el escalado se basan en las opciones documentadas de
[gfxcapture de FFmpeg](https://ffmpeg.org/ffmpeg-filters.html#gfxcapture).
