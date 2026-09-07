# ADR 0010: monitor virtual con vida ligada a la sesión

- Fecha: 2026-09-07
- Estado: implementado; ciclos locales, fallos del host y dos conexiones reales de la Deck comprobados.

## Decisión

La opción `--auto-virtual-h264` negocia Hello y Capabilities antes de activar WindowDeck. Un cliente incompatible no crea un monitor. Una petición local mantiene la pantalla durante esa sesión y se libera después de terminar y recoger el encoder, incluso al salir por error. `--virtual-h264` conserva la activación manual para diagnóstico.

`windowdeck-display --broker` se inicia una vez como administrador en el mismo inicio de sesión de Windows. No crea una pantalla en reposo. Atiende una única petición a través de un named pipe local: la conexión crea el dispositivo fijo WindowDeck, espera el modo activo 1280 × 800 a 60 Hz y responde con un byte de disponibilidad. La liberación explícita o la desconexión cierra el handle de `SwDeviceCreate`. Espera hasta cinco segundos la retirada PnP antes de atender otra petición; la apertura permite un segundo de espera para esa transición. Una segunda sesión activa se rechaza.

El host se ejecuta sin elevarse. Lanza `windowdeck-display --lease` con stdin/stdout redirigidos: recibe `READY` y mantiene abierto stdin. Al cerrar stdin, el auxiliar libera la petición y espera la retirada; si el host muere, Windows cierra su extremo del pipe. Si muere el auxiliar o el broker, el cierre de sus handles también libera el dispositivo. El auxiliar espera hasta ocho segundos la activación y el host limita la espera de su respuesta a nueve segundos. Un arranque más lento falla y libera la petición; no se modifica el cliente instalado ni su timeout de negociación.

La DACL del pipe permite I/O al SID del inicio de sesión, sin conceder `FILE_CREATE_PIPE_INSTANCE`; la etiqueta de integridad media permite la conexión del host sin elevarse. Se rechazan clientes remotos y se exige la primera instancia del pipe. El auxiliar usa `SECURITY_IDENTIFICATION` para limitar la suplantación por parte del servidor. El canal no acepta rutas, comandos, identificadores de dispositivos ni parámetros de instalación. Estas decisiones siguen la [documentación de seguridad de named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) y las [opciones de seguridad de CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

## Límites

Es un controlador local de desarrollo, iniciado manualmente una vez por sesión de Windows; no se instala como servicio ni se configura inicio automático. Debe elevarse con la misma cuenta, no con credenciales de otro usuario. El paquete de driver instalado 0.1.0.7 no cambia. La entrada de red conserva el protocolo LAN del prototipo y su ausencia de emparejamiento autenticado.

La captura sigue siendo WGC con FFmpeg/libx264 a 1280 × 800, 60 FPS y objetivo 16 Mbps. No hay transferencia directa de superficies del driver, cambios de calidad ni optimización de latencia. La nueva sesión requiere volver a abrir el cliente H.264, como antes. Suspensión, bloqueo, cambio de usuario y pérdida de red sin cierre TCP continúan pendientes.

## Validación

`driver/windows-idd/test-auto-display.ps1`, con el broker preparado, comprueba reposo sin pantalla, tres ciclos, rechazo de una segunda petición, cierre normal, cierre forzado del auxiliar y conservación de los dispositivos físicos. Pasó tras añadir la espera acotada para la retirada asíncrona. La primera ejecución llegó a completar dos ciclos y falló al abrir el tercero demasiado pronto; no confundirla con la repetición superada.

`driver/windows-idd/test-auto-host.ps1` pasó el fallo de arranque del encoder y la terminación del host durante vídeo en loopback: desaparece el monitor y no queda el encoder. El test Rust de negociación comprueba que se rechazan capacidades incompatibles antes de pedir el monitor. Dos conexiones reales de la Deck activaron la pantalla y recibieron vídeo; el cierre remoto de cada instancia Flatpak retiró el monitor, con host y broker conservados en reposo. Una tercera conexión permitió confirmar también el cierre interactivo de FFplay desde la Deck, con retirada automática del monitor y recuperación de la ventana de prueba confirmada por el usuario. Los resultados se registran en [testing.md](../testing.md).
