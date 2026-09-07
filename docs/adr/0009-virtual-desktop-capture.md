# ADR 0009: enviar el escritorio virtual mediante Windows Graphics Capture

- Estado: puente provisional implementado; recepción, desconexión abrupta, nueva conexión y retirada durante vídeo con recuperación de una ventana confirmadas; latencia notable y otras pruebas de fallos pendientes.
- Fecha: 2026-09-06

## Decisión

Añadir `windowdeck-host --virtual-h264 [DIRECCIÓN]` para capturar el escritorio del monitor WindowDeck mediante el filtro `gfxcapture` de FFmpeg. Una prueba local con `windows-capture` recibió una textura D3D11 de 1280 × 800 del monitor virtual. FFmpeg 9.0.1 instalado también pudo capturarlo. La primera conexión real de la Deck recibió H.264 con este puente.

La utilidad nativa incorpora `--source`: identifica el adaptador por su ruta `SWD#WindowDeck`, exige escritorio activo de 1280 × 800 a 60 Hz y devuelve el nombre GDI. El host busca ese dispositivo entre los monitores activos y pasa su HMONITOR a FFmpeg. No utiliza un índice supuesto ni el nombre descriptivo de una GPU. El [filtro oficial](https://www.ffmpeg.org/doxygen/trunk/vsrc__gfxcapture_8c_source.html) y su [selección de fuente](https://www.ffmpeg.org/doxygen/trunk/vsrc__gfxcapture__winrt_8cpp_source.html) permiten indicar ese handle directamente.

Se conserva libx264, preset ultrafast, zerolatency, GOP 60, 1280 × 800, objetivo de 16 Mbps y MPEG-TS sobre el protocolo existente. La fuente WGC entrega actualizaciones variables; `fps=60` regulariza su salida, incluyendo duplicados. Esto no acredita 60 imágenes nuevas por segundo ni una mejora de latencia. La opción anterior `--h264 N` conserva ddagrab y sus parámetros de vídeo.

Durante esta ruta un supervisor comprueba la presencia/modo de la fuente, el cierre del cliente y un máximo de diez segundos sin datos del encoder. Al detectar un fallo termina el FFmpeg de la sesión; nunca selecciona como alternativa un monitor físico. El propietario del proceso también lo termina y recoge en las salidas por error. El 7 de septiembre se comprobó la retirada de FFmpeg tras desconexión abrupta del cliente y una nueva conexión al mismo host. También se observó la parada del supervisor por retirada del monitor durante vídeo, cierre del cliente y recuperación de una ventana confirmada por el usuario. El timeout del encoder y otras interrupciones siguen pendientes de aceptación.

## Límites y trabajo posterior

Esta es una recaptura del escritorio compuesto por Windows. No transfiere directamente las superficies del worker IddCx: el driver 0.1.0.7 sigue descartándolas y no se reinstala. La evaluación de codificación dentro del IDD, handles D3D11 y memoria compartida del roadmap continúa pendiente; este puente no decide el IPC definitivo.

El monitor se activa manualmente con `windowdeck-display --run` como administrador y se retira con X. El host de red no necesita elevarse. Cerrar el cliente detiene su encoder pero mantiene ese monitor manual; esta ruta manual conserva la pantalla después de la desconexión. La nueva opción automática se describe en el [ADR 0010](0010-automatic-display-lifetime.md). El usuario confirmó el funcionamiento del escritorio extendido, la ventana trasladada en la Deck y su recuperación al retirar el monitor. Suspensión, bloqueo, cambio de usuario, cierre interactivo de FFplay y pérdida de red sin cierre TCP deben probarse. La latencia percibida es notable frente a la pantalla nativa; se mantiene como trabajo futuro, sin atribuir una causa ni dar por cumplido el objetivo de latencia.

El cliente y el protocolo no cambian. En la Deck se reutiliza el Flatpak construido desde `3b0cd4f`. El host requiere un FFmpeg con `gfxcapture` (comprobado en 9.0.1) y la utilidad actualizada junto al host, en `target/windows-idd/` para los builds locales, o indicada mediante `WINDOWDECK_DISPLAY_EXE`.
