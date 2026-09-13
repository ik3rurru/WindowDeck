# Punto de continuación — 13 de septiembre de 2026

## Distribución Windows/Steam Deck

El usuario solicita publicar todo lo nuevo, limpiar sobrantes y distribuir
Windows y Flatpak, con acceso automático del escritorio e integración con Steam.
Se conserva la aceptación de vídeo CPU y lanzador del día 12.

Implementado: workflow de release por etiqueta, ZIP Windows con fuentes del
commit, Flatpak con icono/metadatos, paquete Steam Deck con instalador que crea
el acceso XDG y `~/.local/bin/windowdeck`. La guía y la investigación de formatos
están en [steamdeck-distribution.md](steamdeck-distribution.md).

Se retiraron el VBS y panel PowerShell anteriores y la copia duplicada de la
imagen de la raíz. Las propuestas de `mejoras.txt` se archivaron en
`docs/archive/mejoras-propuestas-2026-09-08.txt`. Se conservan las evidencias,
paquetes y respaldo del driver de `target/`.

Validación local: 46 pruebas Rust en cada configuración, formato y Clippy,
regresión H.264 con FFmpeg, pruebas sintéticas del cliente, arnés del panel y
ZIP Windows extraído con PATH acotado. Los siete casos del instalador pasan
en la Deck con perfiles temporales, sin cambiar su Flatpak. Detalle en
[testing.md](testing.md#distribución--13-de-septiembre-de-2026).

Publicada [v0.2.1 preliminar](https://github.com/ik3rurru/WindowDeck/releases/tag/v0.2.1),
fuentes `959b0a60d61b2f1ae2aed2c53aa176ce19b5fe6b`. Los siete trabajos del
[workflow Release](https://github.com/ik3rurru/WindowDeck/actions/runs/34749771422)
terminaron correctamente: pruebas base/nativas en Windows/Linux, paquetes de
Windows/Deck y publicación. Las tres descargas tienen SHA256 contrastado con
los digests de GitHub; además se descargaron y verificaron ambos paquetes Linux.
Evidencia local: `target/release-v0.2.1/verification.json`.

El intento `v0.2.0` generó Windows y Flatpak,
pero no publicó la release: la captura del panel coincidía con el cierre
automático del modo de prueba, y Git rechazó la propiedad del checkout dentro
del contenedor. Corregidos mediante cierre solicitado por el arnés y excepción
de propiedad acotada a esa lectura del repositorio. No confundir la publicación con instalación
limpia ni aceptación de vídeo en modo juego, todavía pendientes.

El Flatpak instalado en la Deck no se cambió durante esta entrega: las pruebas
del instalador fueron aisladas. Para instalar la release y sus accesos, utilizar
su paquete Steam Deck. Los binarios locales de las pruebas anteriores se
conservan; reconstruir con `scripts/build-media.ps1` antes de usarlos como 0.2.1.

## Historial — 12 de septiembre de 2026

## Vídeo CPU y lanzador aceptados en la Deck

Resultado del 12 de septiembre: el usuario acepta la latencia con el
reproductor integrado y confirma que Detener devuelve las ventanas al PC.
También se verificaron retirada del monitor y recogida de procesos al terminar
el panel inesperadamente, reconexión de la misma instancia de la Deck tras
reabrirlo y una segunda parada limpia. Al terminar no quedan procesos de sesión
ni monitor virtual. Los accesos directos existentes abren las versiones nuevas.
Detalles, hashes y alcance de las pruebas en la sección del día 12 de `testing.md`.

Quedan pendientes cancelación real de UAC, otras escalas DPI, sesión prolongada
e instalación limpia. No se ha modificado el driver ni validado la ruta GPU.
Los cambios siguen en el árbol de trabajo, sin commit ni publicación.

Se retomó la aceptación del lanzador Rust con la Deck disponible. El paquete
`WindowDeck-0.2.0-5123194d4c2e469483928d7367788ce3` pasó Iniciar/UAC, descubrimiento
mDNS y activación de vídeo CPU H.264 1280 × 800. El usuario observó demasiado
retraso, que se reprodujo al cerrar y abrir completamente el cliente. No dar
la latencia por aceptada ni sustituir esta valoración por los FPS del encoder.

El Flatpak anterior conservaba el commit
`a78bd7428e8b9d8be77f387a329ceb610a57e019afaa4b1d7b65d4073199e681` y usa FFplay 7.1.3.
La comparación temporal con `-threads 1 -filter_threads 1` mejoró claramente
según el usuario, pero la latencia seguía siendo excesiva. Esos dos ajustes
están añadidos al código Rust del cliente y pasan formato, Clippy y las 41
pruebas tanto base como nativas antes del cambio de transporte del host.

El ensayo con `-vf setpts=0` también mejoró, pero el usuario siguió observando
demasiado retraso. No se integró ese filtro. El siguiente ensayo con
`-avioflags direct -probesize 2048` produjo numerosos errores de PPS/slices y
se descartó. Esas sesiones ya estaban cerradas al retomar por la tarde.

La implementación actual conserva captura CPU/libx264 y el perfil de vídeo,
pero negocia H.264 por unidades de acceso con el cliente integrado; mantiene
MPEG-TS y su margen de diez segundos para clientes anteriores. Diseño y límites:
[ADR 0019](adr/0019-cpu-integrated-player.md). El host nuevo pasó una sesión
de compatibilidad con el cliente 0.1.0 antes de actualizarlo.

Se instaló el Flatpak nativo de CI, fuentes `ae6fcb6fa7311f9b75bdaefc22f0ca932e01c1a2`,
commit Flatpak `29c2e47f646b5ed8b710354ca10d5bceeb2d51b4b47c05372afc06a19016d4a0`.
La copia anterior está en `/home/deck/Downloads/windowdeck-before-native-20260912.flatpak`.
No se modificó el driver. El acceso directo existente selecciona el reproductor
integrado automáticamente porque utiliza descubrimiento y el cliente tiene `native-media`.

Paquete de la prueba: `target/WindowDeck-0.2.0-37a30a1942174e3fab15b72378ca103e.zip`,
evidencias en `target/launcher-deck-e2e7a530894b4e66982cf0a7303a9941/`.
Se observan `h264_frames`, renderer OpenGL y decoder VAAPI en la Deck.
La recuperación final está en `target/launcher-deck-d7ef341b254c4549a1567d0fc7010396/`.
`target/launcher-deck-current.txt` señala esa prueba ya cerrada; una nueva sesión
requiere Start, no Restart sobre su panel terminado. El paquete con documentación
actualizada queda indicado por `target/launcher-package-current.txt`; sus binarios
deben conservar los hashes del paquete probado. No confundir métricas internas
con latencia completa ni aceptación visual.

Evidencias: `target/launcher-deck-7591a6b538b74efc89dbf5231a56fb54/`.
El arnés local `target/launcher-deck-check.ps1` permite Inspect/Stop/Close/Crash
y verifica la identidad del panel por PID, fecha de creación y ejecutable.
SSH autorizado de pruebas: `deck@192.168.1.18`, clave `~/.ssh/steamdeck_key`.
Los registros remotos están en `/home/deck/Downloads/windowdeck-launcher-*.log`.
Para cerrar una prueba, consultar `flatpak ps --columns=instance,application,pid`
y terminar su instancia concreta: detener la unidad temporal de systemd no
garantiza cerrar el proceso Flatpak, que puede continuar en su propio scope.

Las capturas del reloj acotan el retraso en instantes concretos, incluyen la
instrumentación y no miden el barrido físico de la pantalla. La primera captura
útil da un límite superior de 572 ms; con un hilo de decoder/filtros, 366 ms.
No presentar esos límites de una sola muestra como promedios ni como una mejora
porcentual controlada. La estimación inicial de transporte usa relojes con un
desfase inestable y no acredita una latencia precisa de red.

## Punto de control — 11 de septiembre de 2026

## Entrega actual: lanzador Rust

Se retomó la siguiente entrega indicada abajo: migrar el panel a
`crates/windowdeck-launcher`. Están implementados los tres botones, estados,
elevación del gestor del broker, cancelación y recogida de procesos mediante
Job Objects y conexión local ligada al panel. CPU sigue siendo predeterminada;
`--native` es explícito. El paquete abre `WindowDeck.exe`; el ejecutable de
desarrollo es `target/release/windowdeck-launcher.exe`. Registros en
`%LOCALAPPDATA%/WindowDeck/logs/`; acceso directo mediante
`scripts/install-shortcut.ps1`. Diseño y límites en ADR 0018.

El arnés de interfaz verifica apertura, controles accesibles, icono, segunda
instancia y cierre sin iniciar host ni broker. Las pruebas del workspace cubren
la nueva CLI, estados tardíos, salida fragmentada, pérdida de conexión local y
recogida de hijos propios. Consultar la sección del 11 de septiembre en
`testing.md` para la validación final y los artefactos.

También pasaron el ciclo real Iniciar/UAC/host en espera/Detener y la terminación
inesperada del panel con recogida de host y broker. No hubo cliente conectado ni
monitor activo. `--verify` devolvió 4 al terminar; no quedaron procesos de sesión.

Paquete probado: `target/WindowDeck-0.2.0-5123194d4c2e469483928d7367788ce3.zip`,
también señalado por `target/launcher-package-current.txt`. Su `WindowDeck.exe`
pasó Iniciar/UAC/espera/Detener; hashes y versión comprobados. El acceso directo
del escritorio ya abre `target/release/windowdeck-launcher.exe`. Los cambios
quedan en el árbol de trabajo, sin commit ni publicación.

No confundir estos ensayos con la aceptación de vídeo en la Deck o con una
instalación limpia. Se conservan `.vbs` y panel PowerShell hasta esa aceptación.
Los pasos pendientes son completar el ciclo con vídeo del nuevo panel y sus fallos,
probar otras escalas DPI y la instalación limpia; después continúa el bloque
de emparejamiento/cifrado. Driver y perfiles de vídeo no se modifican.

## Punto de control anterior — 10 de septiembre

Actualización del 10 de septiembre: el usuario ha pedido implementar `mejoras.txt`
y reanudar este trabajo. El estado actual es [WindowDeck 0.2.0](mejoras-implementadas.md).
La pausa de optimización descrita más abajo pertenece a la sesión anterior.

## Consolidación actual

Las entregas 1 y 2 acordadas actualizan el estado y simplifican la CLI. El panel
utiliza CPU/libx264 por defecto; `-Native` activa la GPU experimental. El host
sin argumentos o con `--driver-h264` utiliza CPU. Las pruebas y referencias
históricas pasan a `diag`; ver [development.md](development.md) y
[ADR 0016](adr/0016-supported-video-routes.md). La integración y distribución
de FFmpeg quedan documentadas en [ADR 0017](adr/0017-media-packaging.md).

Cierre de la entrega: formato y Clippy correctos, 32 pruebas superadas tanto
en configuración base como nativa (multicast excluido), release y autopruebas
multimedia correctos. El paquete local generado es
`target/WindowDeck-0.2.0-25dab3af41da42b4b54520c6d571aecf.zip`;
se verificaron sus ejecutables, fuentes y hashes. El detalle de validación está
en [testing.md](testing.md#consolidación-de-rutas-y-cli--10-de-septiembre-de-2026).

La regresión del límite de 250 ms está corregida para MPEG-TS. Las pruebas reales
del día 10 registraron 207 y 54 segundos sin desconexiones inesperadas. Suspensión
y bloqueo con el cliente anterior se recuperaron en los ensayos finales del día
8, descritos en [testing.md](testing.md); los fallos intermedios que aparecen más
abajo son históricos y no representan el estado actual.

Siguiente entrega: migrar el panel y su gestión de procesos a `windowdeck-launcher`
en Rust, conservando acciones y permisos. Después quedan emparejamiento/cifrado,
validación CPU/GPU en la Deck, decisión de IPC e instalación limpia. No dar por
comprobados la ruta GPU completa, una hora de vídeo o cambio de usuario. La DLL
del driver y los perfiles de vídeo no se modifican en esta consolidación.

El [roadmap de consolidación](../windowdeck-roadmap-consolidacion.md) distingue
lo implementado de la validación pendiente. Los registros de sesiones antiguas
se conservan a continuación como evidencia, no como instrucciones de continuación.

## Historial del 8 de septiembre: H.264 desde frames CPU del driver

**Evaluación de encoders para latencia/FPS (8 de septiembre):** se añadió `WINDOWDECK_H264_ENCODER` para comparar `libx264`, `h264_amf`, `h264_nvenc` y `h264_mf`, manteniendo `libx264` como valor predeterminado. En pruebas sintéticas todos los encoders hardware superan tiempo real; en la ruta real CPU del driver, AMF empezó cerca de 35 FPS y subió a unos 49, mientras NVENC alcanzó unos 60 FPS en escritorio estático pero cayó a unos 30 FPS con movimiento. AMF entregó el primer paquete en 411 ms y NVENC en 171 ms, pero sus tasas no fueron consistentes con movimiento. No se cambia aún el predeterminado. Siguiente trabajo de rendimiento: separar y optimizar la transferencia BGRA y el backpressure del pipeline; después repetir una prueba visual con el candidato hardware más estable.

**Comparación A/B CPU/WGC completada en la Deck (8 de septiembre):** CPU entregó el primer paquete en 494 ms y WGC en 915 ms. La sesión CPU mantuvo alrededor de 50,26 FPS efectivos; el usuario considera su latencia claramente mejor. WGC se percibió con latencia terrible. Evidencias CPU `target/deck-cpu-live-b6c32cd7121140caa2ef79eddcf19967/` y WGC `target/deck-wgc-live-e76dcc6c2fcf4334b20ebe8fef577844/`, con logs remotos `/home/deck/Downloads/windowdeck-ab-cpu.log` y `/home/deck/Downloads/windowdeck-ab-wgc.log`. Ambas sesiones se cerraron y `--verify` confirmó retirada del monitor. Decisión provisional: priorizar la ruta CPU para baja latencia y conservar WGC como referencia/fallback; falta una medición absoluta de latencia antes de cambiar el modo predeterminado.
**Resultado de la suspensión real: reconexión H.264 pendiente.** El journal de la Deck confirma suspensión entre 09:22:27 y 09:23:29 CEST (07:22:27–07:23:29 UTC), unos 62 segundos. Windows retiró monitor y procesos de sesión; el host registró `encoder_stalled` y timeout TCP 10060 a las 07:22:39 UTC. Al despertar, el cliente registró `client_failed` con `Resource temporarily unavailable (os error 11)` y salió; el usuario confirma que la ventana se cerró y no volvió la conexión. El cliente H.264 actual realiza una sola conexión en `receive_h264_test`: no tiene bucle de reconexión (el modo RGB332 sí). No clasificar este ensayo como superado en recuperación automática. Próximo cambio concreto: reconexión H.264 tras fallo de transporte, distinguiéndola del cierre voluntario con X; después actualizar el Flatpak y repetir suspensión y cierre interactivo. Observador de esta prueba detenido; host 8012 y broker 10496 siguen en reposo, sin cliente, encoder ni auxiliar CPU. Evidencia: `target/deck-suspend-retry-72c33b15279247e6b211f8ae3c95d1ab/` y `/home/deck/Downloads/windowdeck-suspend-retry.log`.

**Repetición de suspensión preparada:** el usuario aclara que apagó la Deck en el primer intento y confirma recuperación perfecta de ventanas en Windows. Ese intento no valida suspensión/reanudación; el host registró error TCP 10054 y el observador confirmó monitor inactivo y ausencia de procesos de sesión. Tras el nuevo arranque, SSH vuelve a responder (uptime de seis minutos a las 07:21:18 UTC). Se abre el cliente otra vez con `windowdeck-suspend-retry.service`, log `/home/deck/Downloads/windowdeck-suspend-retry.log`. Nueva evidencia: `target/deck-suspend-retry-72c33b15279247e6b211f8ae3c95d1ab/`, apuntada por `target/deck-suspend-current.txt`; observación cada dos segundos durante un máximo de 15 minutos. Se detuvo el observador anterior. Host 8012 y broker 10496 reutilizados, sin reiniciar. Pendiente de que el usuario suspenda mediante pulsación breve, espere 30 segundos y despierte; no se ha validado todavía el resultado.

**Prueba de suspensión preparada y en curso:** se abrió otra sesión CPU con el host 8012 y broker 10496 existentes. Cliente en la Deck: unidad temporal `windowdeck-suspend-test.service`, log `/home/deck/Downloads/windowdeck-suspend-test.log`. Se pidió al usuario suspender unos 30 segundos y despertar la Deck, observando recuperación de ventanas y vídeo. Pendiente de su respuesta; no afirmar que ha suspendido ni que la prueba ha pasado. Evidencia: `target/deck-suspend-f06ca221eafd4f60b5e1d569643562f2/`, apuntada por `target/deck-suspend-current.txt`. `observations.jsonl` registra cada dos segundos estado del monitor y procesos hijos del host durante un máximo de 15 minutos; crear `stop-observer` en esa carpeta detiene el observador. Logs del host siguen en el directorio de la prueba visual anterior. A las 07:12:35 UTC seguían activos monitor, auxiliar CPU 11068 y encoder 7620. Verificar identificadores al retomar. No se ha forzado suspensión ni reiniciado procesos durante la observación.

**Prueba CPU en la Deck superada (8 de septiembre):** el usuario confirma «la calidad y la latencia son aceptables al cerrar la ventana se recupera perfectamente». Sesión de unos 356 segundos, 636.138.796 bytes recibidos y 25.705 chunks. El host detectó `client_disconnected`; después se verificó `--verify`: 4, ausencia de encoder/auxiliar de frames/instancia Flatpak y conservación del monitor físico y ambas GPU con PnP OK. Los últimos valores del encoder rondan 50,32 FPS efectivos: la valoración visual no acredita 60 FPS ni mide latencia absoluta. Evidencias PC: `target/deck-cpu-live-57d03b14186f43d2ab175225cf9b0c34/`, apuntado por `target/deck-cpu-live-current.txt`, incluidos `devices-before.json` y `devices-after.json`. Cliente: `/home/deck/Downloads/windowdeck-cpu-live.log`. Host sin elevar 8012 y broker CPU elevado 10496 permanecen en reposo; verificar identidades antes de actuar. Crear `stop-broker` en el directorio de evidencias termina el broker mediante su auxiliar administrador. No se han cambiado driver ni ajustes de calidad. No se ha hecho una comparación A/B controlada con WGC ni validado el cursor por separado.

**Reconexión H.264 tras suspensión superada en la Deck (8 de septiembre):** con el Flatpak construido desde `52c4fca`, el cliente detectó `Resource temporarily unavailable (os error 11)` al volver la red, conservó la ventana y reintentó. A las 07:25:03 UTC negoció la sesión nueva y recibió su primer paquete 448 ms después (`h264_reconnected session_id=1788867900363608`). El usuario confirma que la imagen se recuperó y se estabilizó, con latencia inicial notable pero aceptable; considera cumplido el objetivo. Host: `target/deck-cpu-live-e34b5af439ee4f249f437616939dbdb7/`; cliente: `/home/deck/Downloads/windowdeck-reconnect-final.log`. Tras cerrar la prueba, `--verify` devolvió 4 y se retiró el broker CPU; no quedan procesos de sesión. La reconexión no mantiene el socket original durante suspensión: crea una sesión nueva. Quedan optimización de la recuperación inicial, comparación A/B con WGC y pruebas de bloqueo/cambio de usuario.

**Bloqueo de Windows superado en la Deck (8 de septiembre):** con `Win + L`, el cliente H.264 conservó la ventana y reconectó tras el desbloqueo. El primer paquete de la sesión nueva llegó 581 ms después de `h264_reconnected`. El usuario confirma funcionamiento correcto y latencia inicial comparable al arranque, con estabilización posterior. Se cerró la sesión y `--verify` devolvió 4. Evidencias: `target/deck-cpu-live-1da441ce883a4fbc8ac86dedec668d78/` y `/home/deck/Downloads/windowdeck-lock-test.log`. La ruta CPU supera ya cierre, suspensión y bloqueo con reconexión. Quedan comparación A/B con WGC, mejora de la recuperación inicial y cambio de usuario.

Implementado `windowdeck-host --driver-h264 [DIRECCIÓN]`, conectado a `windowdeck-display --cpu-frame-stream` y al broker elevado `--frame-broker`. Transmite el escritorio real por BGRA → libx264 → MPEG-TS, con los mismos ajustes de calidad y protocolo del cliente. No requiere reinstalar el driver 0.1.0.9. Véase [ADR 0013](adr/0013-driver-cpu-h264.md).

- El auxiliar conserva un frame y lo repite con cadencia nominal de 60 FPS cuando no llegan superficies nuevas. Las primeras pruebas breves dan aproximadamente 57 FPS efectivos de encoder. No se demuestra una mejora de latencia ni 60 frames nuevos del driver por segundo.
- Pasan build/análisis/autoprueba nativos, 19 tests Rust, formato y Clippy. El arnés local CPU y la regresión WGC pasan fallo de encoder, desconexión/reconexión al mismo host, decodificación H.264 1280 × 800 a 60 FPS declarados y terminación inesperada del host, con retirada del monitor y procesos de sesión.
- Prueba visual CPU y cierre interactivo superados en la Deck, según la validación anterior. La integración CPU → libx264 queda validada en este equipo; no se elige aún el IPC definitivo. Comparación controlada con WGC, encoder GPU y pruebas de suspensión/bloqueo/cambio de usuario siguen pendientes. Calidad y optimización de latencia siguen aplazadas.
- Aceptación final CPU: `target/auto-host-test-3bc9b983eb1d4bceb9d7ebea23697fe5/`; WGC: `target/auto-host-test-60ffe67270054cf5bef84aa1cd1cfd6d/`. Se verificaron 120 frames decodificados por sesión y conservación de monitores/GPU físicos con PnP OK. Los dos brokers temporales se cerraron al terminar; no se deja host escuchando.
- Cambios de esta iteración y de la anterior aún sin commit ni push.

Las secciones siguientes conservan el historial; consultar esta sección para el estado actual.

## Estado más reciente: comparación D3D11 / CPU

**Texturas D3D11 compartidas implementadas y validadas desde UMDF hasta el lector del host.** Driver instalado **0.1.0.9, `oem101.inf`**; copia firmada anterior en `target/windows-idd-test-v0.1.0.8/`. Se mantienen certificado y ajustes de GPU/arranque. Véase [ADR 0012](adr/0012-shared-d3d11-frame-probe.md).

- Nuevos modos `windowdeck-display --gpu-frame-broker` y `windowdeck-host --gpu-frame-test`. Tres texturas BGRA con NT handles, ACL restringida, keyed mutex y timeout cero en el driver. El auxiliar realiza el readback para verificar las mismas muestras en Rust. El modo CPU anterior sigue disponible.
- Ambas rutas pasan dos lecturas completas de 120 frames, terminación del host entre ellas, reconexión y conservación de dispositivos físicos. D3D11 verificó 100 y 104 frames tras el arranque; CPU, 101 y 101. Todas las no coincidencias son prefijos iniciales.
- GPU: publicación media 0,100–0,102 ms; adquisición hasta píxeles CPU en el lector 23,893–23,975 ms. CPU: publicación 9,341–9,380 ms; hasta el lector 16,928–16,982 ms. Recepción en ambos ensayos alrededor de 24–26 FPS. Las medidas incluyen el arnés GDI/polling/stdout y no representan latencia visual ni un encoder GPU; tampoco son una prueba de 60 FPS.
- Evidencias GPU: `target/driver-frames-test-0cc388090b7f45f3ba9a996a7f7e7b9d/`; CPU: `target/driver-frames-test-fb0a9b125cbd435e817bb0bb5c527e1f/`. PnP confirma versión y paquete en cada ciclo.
- Pasan compilación/análisis WDK/catálogo/autoprueba nativa, 19 tests Rust, formato y Clippy. También tres ciclos automáticos y regresión WGC de fallo del encoder/terminación del host (`target/auto-host-test-5ba1512fbc904a6d84e9e0a3b1ddd16f/`). No hubo prueba nueva de vídeo en la Deck.
- Siguiente tarea propuesta: conectar los frames CPU directos a libx264 en el host como modo experimental, manteniendo calidad, FPS y ruta WGC de referencia. La ruta GPU es viable para un futuro encoder que consuma superficies; aún no se ha implementado ni comparado un encoder dentro del IDD. IPC definitivo, aceleración hardware y optimización de latencia siguen pendientes.
- Directorio de brokers: `target/gpu-probe-ba6b0517407d40eab3c202d520011c16/`, apuntado por `target/gpu-probe-current.txt`. Se cierran los brokers de esta prueba al finalizar; al retomar no había host ni brokers previos. Los PID de las secciones históricas no son actuales.
- Cambios de esta iteración aún sin commit ni push.

Lo siguiente conserva el historial del prototipo CPU anterior; sus versiones y procesos no describen el estado actual.

## Estado más reciente: frames directos del driver

**Prototipo CPU del driver al host implementado y validado.** `windowdeck-host --driver-frame-test` recibe 120 frames BGRA 1280 × 800 desde UMDF por memoria compartida y un auxiliar nativo; valida muestras del patrón en Rust sin WGC ni FFmpeg. Es una referencia temporal, no el IPC definitivo ni una nueva ruta de vídeo para la Deck. Véase [ADR 0011](adr/0011-driver-frame-transfer-probe.md).

- Instalado y verificado por PnP: **0.1.0.8, `oem100.inf`**. Certificado existente, sin cambios de GPU, arranque ni reinicio. Respaldo firmado 0.1.0.7 en `target/windows-idd-test-v0.1.0.7/` y `oem99.inf`; permanece también `oem98.inf` 0.1.0.0. La copia firmada 0.1.0.8 está en `target/windows-idd-test/`.
- Nuevo controlador opcional `--frame-broker`, separado del `--broker` habitual. Crea un mapping global aleatorio de 12.288.448 bytes, con ACL para el logon solicitante, SYSTEM, LocalService y drivers UMDF; su nombre llega al driver como propiedad del software device. Las activaciones normales borran esa propiedad para no reutilizar mappings anteriores.
- Dos staging D3D11 con `Map(DO_NOT_WAIT)` y tres slots de memoria con exclusión por estado; el driver nunca espera a un consumidor lento. Se mantienen estadísticas IddCx `DROPPED` porque el prototipo no transmite a la Deck. Los offsets y tamaños están fijados, no se toman de datos modificables por el lector.
- Aceptación final: `target/driver-frames-test-e6de04bd6ed841e492972e954e98e28d/`. Dos lecturas completas de 120 frames, terminación del host entre ambas y reconexión: pasan. Coincidieron las muestras de los últimos 100 y 104 frames, respectivamente; todas las no coincidencias eran prefijos iniciales (20 y 16 frames). La validación estricta rechaza cualquier no coincidencia posterior a la primera válida.
- Tasas de esos ensayos: 26,54 y 24,95 FPS recibidos; adquisición→publicación medias 9,310 y 9,130 ms; copia CPU al mapping 188 y 183 µs. Incluyen patrón GDI/polling/copias/stdout: no equivalen a latencia visual, no acreditan 60 FPS ni una mejora sobre WGC. Todavía falta comparar handles D3D11 compartidos y codificación dentro del IDD antes de elegir el IPC definitivo.
- Pasan build/análisis WDK/catálogo/autoprueba, los 19 tests Rust, formato y Clippy. También pasaron otra vez tres ciclos automáticos y la regresión WGC de fallo de encoder y terminación del host (`target/auto-host-test-326aa1c461324bc392c728737d758db2/`). La pantalla física VG27AQL5A y ambas GPU siguen con PnP OK.
- Estado final: sin monitor virtual, encoder ni lector de frames. Host normal `--auto-virtual-h264 0.0.0.0:48150` **35812**, broker habitual **21740**, broker de frames **33220**, todos en reposo. Verificar identidades antes de actuar. Los brokers se abrieron elevados y ocultos para las pruebas; no hay instalación de servicio.
- Directorio actual `target/frame-probe-a3a489f15d7a4d55abc47f532f8f6821/`, apuntado por `target/frame-probe-current.txt` y `target/auto-live-current.txt`; contiene instalación, logs de ambos brokers, PID y host reabierto. `probe.log` es la primera aceptación permisiva; `probe-detail.log` identificó que las no coincidencias eran iniciales. Usar la aceptación estricta final indicada arriba.
- Punto de control de Git: driver virtual, captura WGC, vida automática y prototipo CPU de frames directos, junto con sus pruebas y ADR. Consultar `git log -1` para identificar el commit local; no se ha hecho push. Siguiente trabajo: comparar la transferencia mediante recursos D3D11 compartidos con esta referencia CPU y decidir cómo alimentar el encoder. Se mantienen calidad y latencia aplazadas.

Lo siguiente conserva el historial de la automatización anterior; sus versiones y PID ya no son el estado actual.

## Estado más reciente: vida automática del monitor

**Cierre interactivo y recuperación de ventanas superados:** se abrió el cliente en modo ventana mediante `windowdeck-auto-manual-close.service` y se confirmó vídeo a 1280 × 800 y 60 Hz. El usuario cerró con la X de «WindowDeck H.264» en la Deck y confirmó «correcto, se recupera perfectamente». No se sustituyó este cierre por SSH. Tras unos 92 segundos de vídeo, el host registró `virtual_display_released` (`timestamp_ms=1788767971678`) y cierre del socket (10053). La comprobación posterior devuelve `--verify`: 4; no quedan FFmpeg ni auxiliar `--lease`, solo el host y el broker en reposo. Registro remoto de esta prueba: `/home/deck/Downloads/windowdeck-auto-manual-close.log`; registro del host en el directorio indicado debajo.

**Implementado `--auto-virtual-h264` y comprobado con la Deck.** El host negocia capacidades antes de pedir el monitor a `windowdeck-display --broker`. El broker se abre una vez como administrador en el mismo inicio de sesión; el host y su auxiliar `--lease` no se elevan. Cerrar la sesión libera el monitor después del encoder. La terminación inesperada del host o del auxiliar también retira el dispositivo. No se reinstaló el driver ni se cambiaron sus fuentes, calidad o latencia. `--virtual-h264` conserva el modo manual.

- Pasan build nativo y autoprueba, build Rust, formato, Clippy y los tests Rust (ahora 17 en Windows, incluido rechazo de capacidades antes de crear la pantalla).
- `driver/windows-idd/test-auto-display.ps1` pasó tres ciclos: cierre normal, terminación del auxiliar, segunda petición rechazada y dispositivos físicos conservados. La primera ejecución falló al iniciar el tercer ciclo porque la retirada PnP todavía estaba terminando; se añadió una espera máxima de un segundo al abrir el pipe ocupado y pasó la repetición completa.
- `driver/windows-idd/test-auto-host.ps1` pasó fallo de arranque de FFmpeg y terminación del host durante vídeo local; desaparecen monitor y encoder. Evidencia final: `target/auto-host-test-f327f57482774aed8347d46465217cdc/`, incluida una nueva negociación tras el fallo del encoder. El ensayo inicial se conserva en `target/auto-host-test-c81f922a6f42454087360215ceb76336/`.
- Dos conexiones de la Deck activaron el monitor automáticamente. La primera transmitió unos 246 segundos cerca de 60 FPS y la segunda unos cinco segundos. Cada cierre de la instancia Flatpak por SSH retiró pantalla, auxiliar y encoder. Una tercera conexión confirmó después el cierre interactivo de FFplay y la recuperación visual de ventanas en esta ruta automática, como se recoge arriba.
- Estado al terminar las pruebas: monitor inactivo (`--verify`: 4), host automático y broker en reposo; cliente remoto cerrado. PID observados: host **3676**, broker **7612**. Verificar identidad antes de usarlos. El shell que lanzó el host confirmó `IsInRole(Administrator)=False`.
- Evidencias actuales: `target/auto-live-31af49a835584b1395669c2439c24f15/` (apuntado por `target/auto-live-current.txt`), con `host.log`, `broker.log`, `broker.stderr.log`, `deck-reconnect.log` y PID; Deck `/home/deck/Downloads/windowdeck-auto.log`. Registro de ciclos: `target/windows-idd/auto-display-test.log`, que conserva también el primer intento incompleto.
- Procedimientos y límites en [ADR 0010](adr/0010-automatic-display-lifetime.md). Quedan IPC directo de superficies, servicio/inicio automático del broker y pruebas de suspensión, bloqueo, cambio de usuario y pérdida de red sin cierre TCP. Latencia y calidad parametrizable continúan aplazadas.

Las secciones siguientes conservan el historial previo; sus procesos y opciones manuales no describen la prueba automática actual.

## Reanudación del 7 de septiembre

- Al retomar no había procesos WindowDeck/FFmpeg ni escritorio virtual activo. El PC conserva `192.168.1.12` y la Deck responde por SSH en `192.168.1.18`, con el mismo Flatpak. Pasan build Rust, formato, Clippy, los 16 tests y `windowdeck-display --self-test`.
- Se reactivó el monitor a 1280 × 800 y 60 Hz, en posición **-1280,635**, a la izquierda del principal, y se transmitió con los ajustes existentes. La prueba ya ha terminado; el host permanece escuchando.
- **Desconexión abrupta y nueva conexión comprobadas:** al cerrar la instancia Flatpak concreta, FFmpeg desapareció en la comprobación a los tres segundos; el host y el monitor manual siguieron activos. Se reabrió el cliente y volvió a recibir vídeo, con encoder cercano a 60 FPS durante al menos 47 segundos. No equivale a validar cierre mediante la ventana de FFplay ni pérdida de conectividad Wi-Fi.
- Detener `windowdeck-resume-test.service` **no cerró el cliente Flatpak** en esta sesión: su proceso permaneció vivo. El intento de reapertura dejó una conexión pendiente que terminó por EOF al liberarse el host. Para cerrar una prueba remota, consultar `flatpak ps --columns=instance,application,pid`, identificar la instancia de WindowDeck y usar `flatpak kill ID_INSTANCIA`; no asumir que parar la unidad termina el sandbox ni reutilizar identificadores históricos.
- Prueba actual: `target/virtual-resume-41cb3d456cd8422180dde8195477dfde/`, apuntada por `target/virtual-resume-current.txt`; contiene `host.log`, `host.stdout.log`, `host.pid` y `display.pid`. Cliente: unidad temporal `windowdeck-resume-test.service`, registro `/home/deck/Downloads/windowdeck-resume.log`. La instancia consultada tras reconectar fue `42055024`; verificar de nuevo antes de actuar.
- **Retirada durante vídeo y recuperación de ventana confirmadas:** el usuario pulsó X y respondió «sí» a la recuperación de la ventana en el monitor principal. El host registró `virtual_capture_stopped reason="monitor_removed_or_changed"` y cerró la sesión. `--verify` devuelve 4; no quedan utilidad, FFmpeg, monitor ni adaptador virtual. VG27AQL5A, NVIDIA y AMD siguen presentes con estado PnP OK. La Deck registró `h264_stream_stopped` (306994600 bytes, 14313 chunks) y su instancia Flatpak desapareció. El host propio sigue escuchando (PID observado 3352; comprobar identidad antes de actuar).
- La sesión posterior a la reconexión duró unos 173 segundos. El cierre deliberado del encoder deja un aviso `ffmpeg terminó con exit code: 1` en el host; no confundirlo con un fallo espontáneo del encoder: lo precede el evento de retirada. Quedan cierre interactivo de FFplay, pérdida de red sin cierre TCP, timeout del encoder, suspensión, bloqueo y cambio de usuario. No reiniciar ni bloquear la sesión automáticamente. El siguiente trabajo de integración es diseñar IPC directo y vida del monitor ligada a la conexión; latencia y calidad parametrizable siguen aplazadas.

El historial siguiente corresponde al 6 de septiembre; sus procesos e identificadores no describen la prueba actual.

**El monitor virtual ya se activa a 1280 × 800 y 60 Hz y su escritorio se envía a la Deck mediante `--virtual-h264`.** La primera prueba real registra más de dos minutos de recepción y encoder cercano a 60 FPS; el usuario confirma visualmente que funciona como esperaba, con latencia notable respecto a la pantalla nativa. Es una recaptura mediante Windows Graphics Capture, no IPC directo del driver. Se mantiene el driver 0.1.0.7 que prefiere AMD en este equipo y superó diez ciclos de activación/retirada. Falta identificar el motivo interno de la ruta NVIDIA.

## Objetivo y decisiones que se mantienen

- WindowDeck: usar Steam Deck como segunda pantalla real de Windows 11. Host LAN actual: `192.168.1.12`, puerto `48150`; la IP anterior `192.168.1.23` dejó de corresponder a este PC. Deck: `192.168.1.18`. Comprobar las IP al retomar.
- La retransmisión H.264 actual ofrece muy buena calidad según el usuario. Se mantienen 1280 × 800, 60 FPS, objetivo 16 Mbps y buffering reducido de FFplay.
- Hay latencia perceptible y bloqueos muy breves por Wi-Fi, sin causa cuantificada. El usuario aplaza ajustes de latencia, calidad parametrizable y aceleración hardware. No modificar ahora esa ruta.
- Trabajo actual: puente del escritorio virtual a H.264 con `gfxcapture` de FFmpeg, conservando libx264 a 60 FPS y objetivo de 16 Mbps. Todavía sin IPC directo ni activación/retirada del monitor vinculada a la conexión. Véase [ADR 0009](adr/0009-virtual-desktop-capture.md).

## Estado confirmado

- Código bajo `driver/windows-idd/`, adaptación MS-PL del ejemplo oficial Microsoft fijado al commit `d5569c08aa2818c6240744bb47a00f67f20fdb54`.
- Compila x64 con Visual Studio 2026 Build Tools 18.9.2, MSVC 14.51.36231 y SDK/WDK NuGet 10.0.28000.2526. UMDF 2.25 / IddCx 1.4; Windows 11 local, build 26200.
- Un monitor con EDID `WindowDeck`, identidad WND0001, modo 1280 × 800 a 60 Hz. Timing: total 1440 × 825, reloj 71.28 MHz, checksum válido.
- Instalado y firmado localmente: versión **0.1.0.7, paquete `oem99.inf`**, seleccionado por PnP durante la aceptación. Se conserva `oem98.inf` 0.1.0.0 y su copia firmada como respaldo.
- El driver enumera GPU por preferencia de bajo consumo mediante DXGI y consulta D3DKMT para excluir adaptadores indirectos y software. Antes de anunciar el monitor establece el adaptador físico preferido mediante `IddCxAdapterSetRenderAdapter`. En este equipo selecciona AMD; no fija proveedor ni LUID. `RenderPreference` permite ajustar la preferencia al compilar para otros equipos. Si no puede seleccionar uno, conserva la elección predeterminada de Windows.
- El worker consume superficies y reporta estadísticas por frame con estado `DROPPED`, coherente con este prototipo sin encoder. La sesión corregida registró 556 adquisiciones y 556 informes correctos; el modo de 60 Hz no demuestra 60 frames nuevos por segundo con un escritorio estático.
- Durante la activación: adaptador `SWD\WINDOWDECK\WINDOWDECKDISPLAY` y monitor `DISPLAY\WND0001\1&21883EBA&0&UID256`, ambos sin problemas PnP. El cierre forzado del proceso propio retira ambos.
- Al terminar la aceptación automatizada no quedó ningún proceso `windowdeck-display`, monitor virtual ni sesión ETW WindowDeck en ejecución; se conservó el monitor físico VG27AQL5A y los adaptadores NVIDIA RTX 5070 Ti / AMD Radeon Graphics.
- Al retomar la sesión se abrió la utilidad interactiva y el usuario confirmó «veo la segunda pantalla». `--verify` confirmó 1280 × 800 a 60/1 Hz, posición 2560,0. El usuario no pudo comprobar el movimiento/recuperación de una ventana; no considerarlo validado por la miniatura de Configuración. Se reabrió la utilidad a petición del usuario y, tras indicarle que pulsara X, confirmó «hecho». La comprobación posterior confirmó ausencia del proceso y de la pantalla activa (`--verify`: 4), retirada del adaptador/monitor virtual y conservación de VG27AQL5A y ambas GPU físicas, todos con estado PnP OK. La utilidad ya está cerrada.
- Build, análisis estático WDK, INF/CAT y `--self-test` pasan. También `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los 14 tests Rust. Host, cliente y protocolo no se modificaron en esta iteración.
- Estas comprobaciones pasaron con la corrección final del 6 de septiembre. `--verify` también se comprobó sin monitor: devuelve 4. `test-device.ps1` completó diez ciclos con escritorio activo 1280 × 800 a 60 Hz, cierre normal y conservación de los dispositivos físicos.

## Historial del fallo previo a la corrección

Antes de la corrección, el usuario oía el sonido de conexión, pero no aparecía ninguna pantalla adicional ni opción para extender en Configuración. Las pruebas siguientes corresponden al paquete original y a variantes ya descartadas; sus conclusiones pendientes describen ese momento. El resultado actual se recoge después y en el [registro de selección de GPU](testing.md#selección-de-gpu-y-corrección-del-prototipo).

1. `DN_STARTED` y el callback de creación no confirman llegada del monitor ni escritorio activo. La utilidad ya aclara este límite en su mensaje y pide mantener la terminal abierta.
2. `DisplayConfigGetDeviceInfo` devuelve el modo preferido 1280 × 800. `SetDisplayConfig` devuelve éxito al validar y aplicar la extensión, pero `QueryDisplayConfig`, GDI y un proceso nuevo siguen mostrando solo el monitor físico activo, 2560 × 1440 a aproximadamente 120 Hz.
3. Ya se probaron extensión genérica, rutas explícitas conservando la pantalla principal, modos/posición explícitos, `SDC_NO_OPTIMIZATION`, `SDC_VIRTUAL_MODE_AWARE`, `ChangeDisplaySettingsEx` y espera de diez segundos tras aplicar. No activaron la pantalla.
4. La sesión es la de consola (1), estación WinSta0, escritorio Default; no se detectó ejecución en una sesión remota o escritorio distinto.
5. La traza WPP IddCx confirma inicialización, aceptación de modos y llegada del monitor con `STATUS_SUCCESS`. No contiene errores explícitos ni commit de modo/asignación de swap-chain. El worker de superficies existe, pero **no está validado en ejecución**.
6. Cambiar solo `IDDCX_TRANSMISSION_TYPE_OTHER` por `WIRED_OTHER`, como en el ejemplo oficial, tampoco funcionó. Variante 0.1.0.1 (`oem99.inf`) comprobada realmente como seleccionada por PnP y después retirada. Código y paquete firmado original restaurados. No presentar ese cambio como una solución ni dejarlo aplicado.
7. El 6 de septiembre se probó la variante 0.1.0.2: `MonitorDescription.DataSize = 0` y `pData = nullptr`, conservando el modo y sus timings. IddCx ejecuta `DefaultModes`, acepta un modo y anuncia el monitor `DISPLAY\DEFAULT_MONITOR\1&21883EBA&0&UID256`. La solicitud explícita de extensión valida/aplica con resultado 0, pero ocho segundos después la ruta continúa inactiva. No hay commit de modo ni asignación de swap-chain.
8. Sobre la variante sin EDID, la 0.1.0.3 cambia únicamente los timings anunciados al cálculo del ejemplo oficial: tamaño total igual al activo (1280 × 800), hSync 48 kHz y pixelRate 61.44 MHz. Mismo resultado: PnP correcto, modo preferido 1280 × 800, extensión con resultado 0 y pantalla inactiva, sin swap-chain. Se verificó el paquete seleccionado en ambas variantes y se retiraron sin forzar ni reiniciar. Windows reutilizó `oem99.inf` para cada una; ese nombre por sí solo no identifica una versión. Se conserva `oem98.inf` 0.1.0.0 y el código original.
9. El ejemplo Microsoft completo del mismo commit, conservando `Driver.cpp`, `Driver.h`, `Trace.h` y `IddSampleApp/main.cpp`, también falla. Compilado con sus proyectos y los kits disponibles; solo se ajustaron rutas de salida/kit, fecha/versión del INF y su proveedor a `WindowDeck Official Sample Test`. Se mantuvo incluso la ruta original `%12%\UMDF`. Se verificó la selección de `oem99.inf` 1.0.0.0 y la aparición de los tres monitores S2719DGF, LEN Y27fA y uno sin EDID. Ninguno se activó; pedir extensión del primero (preferido 2560 × 1440) devuelve 0 sin asignar swap-chain. El paquete de control se retiró al acabar; queda solo el WindowDeck original y las GPU físicas.
10. `DispBrokerDesktopSvc`, `DeviceAssociationService` y `DeviceInstall` están en ejecución. La traza DWM del ensayo original recoge tres eventos `SCHEDULE_DERIVEDISPLAYSET` (201), todos con `fSucceeded=0`, alrededor de llegada, solicitud de extensión y retirada; `nrOfAttempts` pasa de 67149 a 67550. Son evidencia de la actualización del conjunto de pantallas en DWM, no identifican por sí solos una causa raíz. La traza `DDisplay` no aportó eventos útiles.
11. Con autorización se envió una vez el atajo Win+Ctrl+Mayús+B (8/8 eventos aceptados por `SendInput`) con el driver original activo. Después de ocho segundos y una nueva extensión, el monitor siguió inactivo, sin commit ni swap-chain en IddCx. No confundir el envío del atajo con una prueba de que se haya reiniciado el proceso DWM. No se reinició Windows ni se cambiaron servicios, drivers de GPU o seguridad de arranque.
12. El usuario reinició Windows y confirmó «ya he reiniciado». `target/idd-after-restart-check.ps1` se ejecutó elevado de 15:13:18 a 15:13:35; registró `LastBootUpTime = 06/09/2026 15:11:30`, sesión de consola 1 y paquete original `oem98.inf`. Windows anunció 1280 × 800 y validó/aplicó la extensión con código 0, pero ocho segundos después la ruta seguía inactiva (`flags=8`, frecuencia `0/0`); solo VG27AQL5A estaba activo a 2560 × 1440 y aproximadamente 120 Hz. La prueba terminó con código 4. IddCx vuelve a registrar llegada correcta, sin commit ni asignación de swap-chain. Se cerraron proceso y traza, y la comprobación final conservó exactamente los dispositivos físicos iniciales.

13. Una captura ampliada de DWM, DxgKrnl, IddCx y Code Integrity no aisló un error causal. El auxiliar nativo confirmó que ambas GPU físicas podían crear un dispositivo D3D11 con BGRA; no demuestra un fallo general de ninguna GPU.
14. La variante 0.1.0.4 intentó preferir NVIDIA por identificación DXGI, pero seleccionó por error el propio adaptador indirecto, que copia descripción/proveedor de NVIDIA. IddCx rechazó ese LUID: esta ejecución no es evidencia contra la NVIDIA física. La variante 0.1.0.5 sí seleccionó AMD física y activó el monitor con entrega de superficies. Volver al original reprodujo el fallo. La variante 0.1.0.6 identificó correctamente la NVIDIA física y su preferencia fue aceptada, pero el monitor siguió inactivo, sin commit ni swap-chain.
15. Con AMD apareció además un diagnóstico explícito de IddCx tras cien frames sin estadísticas. No prueba por sí solo una interrupción del flujo. La versión 0.1.0.7 incorpora selección física por bajo consumo e informes por frame. La aceptación de unos 45 segundos registró 556 superficies y 556 informes correctos, sin ese diagnóstico, con el escritorio activo en ocho comprobaciones. Una segunda instancia fue rechazada y la original permaneció activa. Pasaron después diez ciclos normales.

La evidencia sitúa el bloqueo de activación en la ruta de renderizado NVIDIA de esta combinación de Windows/controladores; seleccionar AMD es una solución comprobada en este equipo. El motivo interno sigue sin aislar y no se ha comprobado otro equipo. No atribuirlo al Wi-Fi: sucede antes del streaming. No cambiar drivers de GPU para continuar con la integración que ya permite la corrección.

## Próximo paso

El usuario confirmó el funcionamiento de `--virtual-h264`: escritorio extendido y ventana trasladada visibles en la Deck. La latencia es notable frente al monitor nativo; decidió tenerla en cuenta para mejoras posteriores, por lo que se mantienen los ajustes. La medición cuantitativa sigue pendiente y no se ha aislado su causa. Próxima aceptación: cierre del cliente, retirada del monitor durante el vídeo y recuperación de ventanas. Los procesos se dejaron disponibles para la prueba; no se han cerrado automáticamente tras la confirmación. El supervisor de la nueva ruta compila, pero todavía no se han probado esas salidas. El hito 4 también conserva pendientes suspensión/bloqueo/cambio de usuario. El usuario ya confirmó visualmente la segunda pantalla en Configuración y completó el cierre interactivo con X antes de la integración. Las pruebas que interrumpen la sesión requieren coordinarse con el usuario.

Después, diseñar la transferencia directa de superficies al host/encoder y vincular la vida del monitor a la conexión de la Deck. El worker actual sigue descartando frames y no hay IPC; la ruta provisional recaptura el escritorio desde el host. `--h264 N` conserva la captura anterior. Preferir bajo consumo resuelve este equipo, pero deberá contrastarse en otras combinaciones de GPU antes de distribuirlo.

## Prueba de vídeo en curso

- Host compilado con `--virtual-h264 0.0.0.0:48150`; cliente y protocolo sin cambios. Pasaron build C++/autoprueba, Clippy, formato y los 16 tests Rust antes de la prueba real.
- Utilidad actualizada con `--source`, que consulta la ruta WindowDeck y devuelve su nombre GDI solo si está activa a 1280 × 800 y 60 Hz. No se reinstaló el driver. En esta activación devuelve `\\.\DISPLAY13`; el host obtiene su HMONITOR en cada sesión, sin fijar esos identificadores.
- El usuario autorizó SSH a `deck@192.168.1.18` con la clave local `~/.ssh/steamdeck_key`. El Flatpak instalado ya corresponde a `3b0cd4f`, commit Flatpak `83d40c1e0d2c5ce68f177fef0de08aa329c5e48f19ea926604c584b442e0f8e9`. No hizo falta instalar el bundle de `/home/deck/Downloads/WindowDeck.flatpak` (`/Downloads` no existe).
- Cliente iniciado en la sesión gráfica mediante la unidad temporal de usuario `windowdeck-live-test.service`, con `flatpak run io.github.ik3rurru.WindowDeck 192.168.1.12:48150 --h264-test --fullscreen`. Registro remoto: `/home/deck/Downloads/windowdeck-live.log`. El primer intento a la IP antigua no conectó; la repetición con la IP actual sí.
- Procesos dejados activos para la prueba del usuario: utilidad interactiva de Windows, host oculto y cliente a pantalla completa en la Deck. Directorio local `target/virtual-live-7a456af4883d4114a4abbb1e3bf827c7/` con `host.log`, `host.stdout.log`, `display.pid` y `host.pid`; `target/virtual-live-current.txt` apunta a él. Comprobar identidad antes de usar los PID guardados.
- Las muestras del host a unos 116 segundos indican 6947 frames y 59.99 FPS; el cliente registra recepción hasta al menos 123 segundos. Esto acredita transporte y ritmo del encoder, no 60 frames nuevos por segundo ni latencia visual. Después, el usuario confirmó que funciona como esperaba y señaló latencia notable. Queda aceptado el funcionamiento visual básico, no los objetivos de latencia ni estabilidad prolongada.
- Para cerrar: el usuario puede cerrar el cliente y pulsar X en la utilidad. Por SSH, parar la unidad temporal no garantiza cerrar el sandbox Flatpak (comprobado el 7 de septiembre); identificar la instancia de WindowDeck con `flatpak ps` y cerrarla con `flatpak kill ID_INSTANCIA`. Comprobar después la retirada del encoder. El host queda escuchando y el monitor permanece hasta cerrar su utilidad: aún no hay vida automática ligada a la conexión.
- La prueba sintética automatizada de píxeles no llegó a ejecutarse: el usuario prefirió la prueba real y se canceló la consulta de Python. No tratar esa prueba como superada.

La aceptación repetible del producto es `driver/windows-idd/test-device.ps1`; `--probe` comprueba ahora escritorio, resolución y frecuencia antes de cerrar normalmente. `target/idd-fixed-driver-check.ps1` conserva el ensayo de unos 45 segundos y su captura IddCx. `target/idd-after-restart-check.ps1` pertenece al diagnóstico histórico: exige el paquete original, por lo que no es una aceptación de la versión instalada actual. No es necesario reinstalar el paquete para repetir una activación.

## Archivos y comandos de reanudación

- [Driver y procedimientos](../driver/windows-idd/README.md), [registro de pruebas](testing.md), [ADR 0008](adr/0008-virtual-display-prototype.md), [hoja de ruta](../WINDOWDECK_ROADMAP.md).
- `Driver.cpp`: selección de GPU física, callbacks IddCx y worker D3D11 que descarta superficies y reporta estadísticas.
- `DisplayMode.h`: EDID y modos. `Control.cpp`: `SwDeviceCreate`, `--run`, `--probe`, `--verify` y autoprueba.
- `build.ps1`: compila sin instalar. `install-test.ps1`: firma/confía en certificado e instala; requiere autorización y administrador. `test-device.ps1`: diez ciclos con comprobación de escritorio activo y modo, cierre normal y conservación física.

Desde la raíz del repositorio, sin instalar ni activar dispositivos:

```powershell
git status --short
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/build.ps1
.\target\windows-idd\windowdeck-display.exe --self-test
```

Para una activación autorizada, en PowerShell como administrador, manteniendo la terminal abierta:

```powershell
.\target\windows-idd\windowdeck-display.exe --run
```

X solicita retirada. Desde otra terminal, `--verify` comprueba la ruta activa y el modo 1280 × 800 a 60 Hz: devuelve 0 si coinciden, 4 si falta la pantalla o el modo no coincide y 1 ante un error de consulta. `--probe` realiza esa comprobación cinco segundos después del arranque PnP y cierra normalmente; no sirve para inspección manual prolongada. No es necesario reinstalar el paquete para repetir una activación.

## Seguridad y cambios en el equipo

- El usuario autorizó certificado local de prueba e instalación. Certificado `FFE0706D3221825F41678E40CE25727D938F101C`, caduca 2026-12-04; confianza pública en LocalMachine/Root y TrustedPublisher. Clave privada no exportable en CurrentUser/My; copia pública en `target/idd-signing/WindowDeckDisplay.cer`.
- Secure Boot ya estaba desactivado; BCD no mostraba TESTSIGNING en la entrada actual. **No se cambiaron esos ajustes. El usuario reinició Windows manualmente el 6 de septiembre a las 15:11:30; no hay autorización para reinicios automáticos ni cambios de seguridad de arranque.**
- Se retiraron los paquetes experimentales WindowDeck y del control oficial. Permanecen `oem99.inf` 0.1.0.7 (corrección) y `oem98.inf` 0.1.0.0 (respaldo). Windows reutilizó `oem99.inf` durante distintos experimentos: verificar siempre proveedor y versión. Copia recuperable del original en `target/windows-idd-test-v0/`; paquete corregido en `target/windows-idd-test/`.
- Antes de instalar/retirar una variante, verificar proveedor, INF y versión exactos, y que no exista una utilidad del usuario en ejecución. No tocar drivers de GPU ni usar eliminación forzada.

## Evidencias locales y herramientas

`target/` está ignorado por Git; conservarlo para no perder paquetes, símbolos y registros:

- `target/windows-idd-test/`: paquete firmado corregido 0.1.0.7 y registros históricos de instalación/diagnóstico. `device-cycles.log` contiene un primer intento fallido por el manejo de `ExitCode` en PowerShell y, después, los diez ciclos superados; `pnp-1.stdout.log` a `pnp-10.stdout.log` confirman pantalla activa y cierre normal, con stderr vacío. No confundir el `idd.txt` histórico de esta carpeta con la traza corregida indicada debajo.
- Aceptación corregida: `target/idd-fixed-check-a0e51df3df534813a5ed8bfae3137db6/`, con `check.log`, salida y traza de unos 45 segundos. Tracefmt: 5104 eventos, cero perdidos, cero errores de formato y tres eventos sin formato. 556 frames adquiridos y 556 informes de estadísticas correctos; sin el diagnóstico de falta de estadísticas. Conserva verificación de `oem99.inf` 0.1.0.7, ocho muestras activas, segunda instancia rechazada y retirada final.
- Contraste de GPU: `target/idd-render-amd-check-910fad24d681462c84d2a26a90a8198a/` (AMD activa), `target/idd-render-nvidia-physical-check-67a44800b6db4759b75581f13d3f5284/` (NVIDIA física, preferencia aceptada pero inactiva) y `target/idd-after-restart-47e6fe83cfb940589bccd7aa4f023910/` (original sigue inactivo tras probar AMD). `target/idd-render-nvidia-check-ed41631ced2e4205852db0d275267264/` seleccionó por error el IDD: no es una prueba válida de NVIDIA física.
- `target/idd-adapters.cpp` y `.exe`: inventario nativo DXGI/D3DKMT y creación de dispositivo D3D11. Los LUID cambian entre sesiones/arranques; nunca copiarlos como configuración permanente. Captura ampliada de activación: `target/idd-activation-1c53933273154e12b76765a79b4f9baf/`.
- `target/idd-display-check.cpp` y `.exe`: diagnóstico nativo temporal. Sin argumentos solo consulta; `--extend` cambia la topología. No confundir este auxiliar con la utilidad del producto.
- El auxiliar ahora identifica el adaptador por su ruta de dispositivo sin distinguir mayúsculas, en lugar del nombre EDID, e incluye `--verify` (0 si hay ruta activa de WindowDeck, 4 si no). `--verify` no verifica por sí solo resolución, frecuencia ni superficies; leer también la salida y la traza. La primera prueba sin EDID no llegó a solicitar extensión por un fallo de mayúsculas en el auxiliar; se corrigió y se repitió. Usar `target/idd-no-edid-check-2/check.log` para el resultado completo, no la primera ejecución.
- Evidencias del 6 de septiembre: `target/idd-no-edid-check/`, `target/idd-no-edid-check-2/` y `target/idd-sample-timing-check/`. Incluyen paquete seleccionado, consultas antes/después, retirada y trazas WPP. Los scripts de ensayo con esos nombres están bajo `target/`; son experimentos de una sola ejecución, no pruebas del producto. Los originales de fuentes previas a los experimentos están en `target/idd-no-edid-source/` y `target/idd-sample-timing-source/`.
- Control oficial: `target/idd-official/`, con fuentes, licencia MS-PL, proyectos, paquete firmado conservado y `test.log`/`idd.txt`. Las fuentes de driver y utilidad se contrastaron con el commit remoto (iguales salvo espacio final). El paquete oficial de prueba ya está desinstalado.
- Diagnóstico de Windows: `target/idd-environment-check/` y `target/idd-recovery-check/`. En el primero, `dwm.xml` contiene los eventos 201 y ocupa unos 41 MB; procesarlo por eventos, sin volcarlo entero. `DDisplay` no contiene eventos útiles. En el segundo, `check.log` confirma el envío del atajo y el resultado inactivo. Las sesiones ETW y los procesos de prueba se cerraron.
- Tras reiniciar: `target/idd-after-restart-0610c122284d4963b0702c5ef189c543/`, con `check.log`, `stdout.log`, `stderr.log` vacío, `idd.etl` e `idd.txt`. Se verificó que la DLL IddCx conserva el GUID/age de los símbolos anteriores. Tracefmt procesó 67 eventos, sin pérdidas ni errores de formato; dos eventos no tienen formato disponible. La revisión por eventos de las trazas DWM/DxgKrnl anteriores no aisló un error causal; el inventario `previous-kernel-events.txt` corresponde a la captura del 5 de septiembre, no al nuevo arranque.
- Las fechas de archivos y compilaciones observadas pueden quedar desalineadas: después de editar, comprobar que MSBuild recompila realmente el `.cpp`, o usar una reconstrucción completa. Se tuvo que actualizar la fecha del auxiliar para que incluyera `--recover-graphics`; se verificó su ejecución por el mensaje de 8/8 eventos en el registro.
- Otros scripts antiguos bajo `target/` corresponden a ensayos históricos; por ejemplo, `idd-cycles.ps1` falló en el primer ciclo de extensión. La aceptación actual superada es la descrita arriba. En `test-device.ps1` se conserva el handle del proceso inmediatamente tras `Start-Process`, necesario para que Windows PowerShell mantenga `ExitCode` después de `WaitForExit`; sin ello el primer intento falló aunque la pantalla se activó correctamente.
- WPP IddCx: proveedor `{D92BCB52-FA78-406F-A9A5-2037509FADEA}`. Herramientas `tracepdb.exe` y `tracefmt.exe` ya disponibles en `C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/`.
- Símbolos usados: `target/idd-tools/IddCx.pdb`, GUID `27DD72A26E532EC8B3BB1FD99C63C995`, age 1; TMF en `target/idd-tools/tmf/`. Verificar que la DLL del sistema no haya cambiado antes de reutilizarlos.
- La traza DxgKrnl con Base produjo un XML de unos 240 MB sin aportar la causa. Evitar repetirla o cargar ese XML entero en memoria. `dispdiag` produce un formato DAT que no se puede tratar directamente como ETL con `tracerpt`.
- Usar MSBuild de 64 bits. `build.ps1` ya normaliza variables Path/PATH duplicadas y restaura NuGet por el endpoint v2. La elevación del entorno de ejecución no equivale a administrador Windows: UAC es un paso separado.

## Persistencia en Git

El punto de control posterior a `3b0cd4f` (buffering de FFplay) reúne el driver Windows, la integración del host, los ADR 0008–0011 y la documentación de pruebas y continuidad. Los cambios previos de Wi-Fi se conservan. Los paquetes firmados, binarios y registros de `target/` siguen siendo evidencia local ignorada por Git; conservarlos al retomar. El commit es local y no se ha hecho push.

## Cierre de FPS — 2026-09-08

El usuario acepta la transmisión durante el uso normal y considera esperables los FPS bajos al arrancar los scripts y establecer comunicación. Se pausa la optimización: no continuar automáticamente con encoder GPU ni más pruebas de rendimiento. Se conserva CPU H.264/libx264 con temporización QPC, temporizador de alta resolución y métricas por intervalo. Esta aceptación no certifica 60 FPS presentados de forma continua.

Detalle de cambios, evidencia, correcciones de interpretaciones anteriores y validación: [revisión de cadencia](fps-pacing-review.md). La última sesión quedó activa para el usuario en `target/deck-cpu-live-611371701eef48a89fb6518ee6b2f95a/`; verificar identidad de procesos antes de usar PID guardados. El Flatpak b60158e sirve para estos cambios del host. Compilar una DLL no actualiza el driver instalado.
