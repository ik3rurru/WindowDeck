# IPC definitivo del driver: frames CPU

Fecha: 3 de noviembre de 2026. Estado: aceptado. Cierra el bloque 3 de la
[consolidación](../../windowdeck-roadmap-consolidacion.md) y sustituye el
estado provisional de los ADR [0011](0011-driver-frame-transfer-probe.md) y
[0012](0012-shared-d3d11-frame-probe.md).

## Motivo

El IPC del driver era el mayor riesgo abierto: los ADR 0011/0012 dejaron la
elección pendiente de comparar las rutas completas CPU (`--driver-h264`) y GPU
(`--driver-native-h264`) en la Deck. Mientras siga abierto, cada cambio en el
driver, el broker o el encoder tiene que mantener ambas.

## Evidencia disponible

- Ruta CPU: memoria compartida con slots, libx264 en el host y el cliente
  integrado. Es la predeterminada desde el [ADR 0016](0016-supported-video-routes.md)
  y la única usada en la Deck desde septiembre. El 12 de septiembre el usuario
  aceptó latencia y retorno de ventanas, y pidió pausar la optimización y no
  seguir con el encoder GPU ([testing.md](../testing.md)). En la prueba de 0.4.0
  por Wi-Fi de 5 GHz presentó 60 FPS sin descartes, con unos 5,5 ms entre
  recibir y presentar; el tirón residual se atribuyó a la red.
- Ruta GPU: texturas D3D11 compartidas con keyed mutex y conversión y encoder
  en `windowdeck-media`. Pasa el probe del driver y pruebas sintéticas, pero
  nunca se ha aceptado en la Deck. En el probe, la publicación GPU (≈0,1 ms)
  no adelantó la disponibilidad de píxeles (≈24 ms frente a ≈17 ms en CPU),
  porque el readback pasa al auxiliar.

**No se ha hecho la comparación A/B completa en la Deck que pedía el bloque 3.**
Esta decisión acepta ese riesgo: la ruta CPU cumple el objetivo actual
(1280 × 800 a 60 Hz) y el cuello de botella observado es el Wi-Fi, no el IPC.

## Decisión

- El IPC del driver es la **memoria compartida con frames CPU**. El encoder
  sigue fuera del driver, en el host.
- La ruta de texturas D3D11 (`--driver-native-h264`, `WindowDeck.exe --native`)
  queda congelada como experimento. Se conservan código, probe y mediciones,
  pero no bloquea releases ni se mantiene a la par de la ruta CPU. Si se rompe,
  se puede retirar con un ADR que conserve estas mediciones.
- La rama de investigación D3D11 compartida queda cerrada en el roadmap.

## Cuándo reabrirla

Solo con una necesidad medida que la ruta CPU no cubra, por ejemplo:

- 90 Hz en la Deck OLED o un modo extendido mayor que 1280 × 800, con el host
  perdiendo frames por la copia CPU o por libx264;
- un encoder por hardware (AMF/NVENC/QSV) que requiera superficies GPU para
  bajar el consumo de CPU de forma significativa.

Reabrirla exige la comparación completa en la Deck: mismo contenido, latencia
física con grabación, carga de CPU y GPU, y recuperación ante device-loss.

## Consecuencias

- El Hito 4 conserva como pendientes la firma y distribución del driver, el
  cambio de usuario, varias GPU y la consolidación del broker; el IPC deja de
  estar entre ellos.
- Las pruebas de aceptación y release solo exigen la ruta CPU.
