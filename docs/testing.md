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

Se conserva B como perfil predeterminado tras la mejora percibida en la prueba de Steam Deck descrita debajo. La latencia numérica sigue pendiente de una grabación a cámara lenta. El siguiente paso es mantener una sesión de una hora para comprobar que no aumenta el retraso ni la memoria de forma progresiva.

### Prueba visual en Steam Deck del 5 de septiembre de 2026

Tras publicar el Flatpak `3b0cd4f`, el usuario confirmó una mejora visual muy notable con el perfil reducido, ejecutado sin `--ffplay-baseline`. Se mantiene este perfil como predeterminado. La valoración es subjetiva: no se han aportado duración, tipo de conexión, logs ni latencia en milisegundos para esta sesión; siguen pendientes la medición cuantitativa y la prueba de estabilidad de una hora.

En una sesión posterior de duración considerable, sin tiempo exacto indicado, el usuario describe una calidad de imagen muy buena, latencia perceptible y bloqueos puntuales muy breves. Confirma que el PC y la Deck se conectan por Wi-Fi. No se ha confirmado si el retraso crece con el tiempo. La posible relación con la red sigue siendo una hipótesis: faltan métricas durante los bloqueos para distinguir red, captura/codificación y reproducción. Esta observación no cierra todavía el criterio de una hora sin degradación.

En el diagnóstico de red, el PC mostró una conexión Wi-Fi de 5 GHz, señal del 79 % y velocidad de enlace nominal de 1201 Mbps. Las reglas de entrada de eco ICMP de Windows estaban desactivadas, mientras la conexión TCP de vídeo seguía establecida. Se probó el sentido PC → Deck: 30 pings de 32 bytes, 30 respuestas y ninguna pérdida, con RTT mínimo de 3 ms, máximo de 30 ms y media de 11 ms. Esta muestra breve presenta variación del tiempo de respuesta, pero no demuestra que la red cause los bloqueos ni mide la latencia del vídeo. No se modificó el firewall.

El usuario aportó cuatro muestras del host entre `elapsed_ms=45809` y `48878`: el encoder pasa de 2670 a 2850 frames, con FPS acumulados entre 58,43 y 58,47 y velocidad entre `0.999x` y `1x`. No se observa una caída sostenida de ritmo en este fragmento de unos tres segundos. Los valores de envío de 19,16 a 18,14 Mbps son medias desde el arranque; al calcular las diferencias de bytes y tiempo, los tres intervalos dan 2,66, 2,89 y 2,92 Mbps. No deben interpretarse las medias acumuladas como caudal instantáneo ni como prueba de saturación. Los logs de envío describen escrituras del host, no confirman cuándo recibe o muestra la Deck cada frame; faltan datos del receptor y correlación temporal con un bloqueo.

### Comprobación local del 5 de septiembre de 2026

Con FFmpeg y FFplay 9.0.1 de Gyan en Windows, se probaron ambos perfiles durante doce segundos cada uno, con el host y el cliente conectados por loopback y FFplay usando el controlador SDL `dummy` y renderizado software. Ambos recibieron datos; el perfil de `40d2bd6` produjo errores `non-existing PPS 0 referenced` y `no frame!` al arrancar. Recibir paquetes por sí solo no confirma una decodificación correcta.

Para aislar la opción responsable se generó un único MPEG-TS sintético de cuatro segundos (`testsrc2`, 1280 × 800, 60 FPS, `libx264`, `ultrafast`, `zerolatency`, 16 Mbps, sin B-frames y GOP de 60). Se reprodujeron los mismos bytes por `pipe:0` con las opciones comunes del cliente y `-vf showinfo`:

| Opciones añadidas al perfil A | Frames decodificados | Primer PTS decodificado | Mensajes de error PPS / frame |
| --- | ---: | ---: | ---: |
| Ninguna | 180 | 1 s | 0 |
| `-avioflags direct -max_delay 0 -sync ext` | 180 | 1 s | 34 |
| `-max_delay 0 -sync ext` | 180 | 1 s | 0 |
| `-avioflags direct` | 180 | 1 s | 34 |

El recuento incluye los mensajes explícitos, sin expandir las repeticiones agrupadas por FFplay. La prueba reproduce los errores al añadir `direct` y los elimina al retirarlo; por eso el perfil B conserva solo `-max_delay 0 -sync ext`. Todos los perfiles comenzaron en el keyframe de un segundo: ese PTS no mide latencia visual ni demuestra que B sea más rápido. La mejora percibida en Steam Deck se registra arriba; falta cuantificarla en red real.

Tras corregir el cliente se repitió la prueba de loopback durante doce segundos por perfil: A y B recibieron vídeo sin errores PPS/frame. El escritorio cambió entre sesiones, por lo que sus tasas de datos no sirven para comparar rendimiento. El cierre forzado del reproductor al acabar la comprobación genera un `client_failed` esperado; no se validó aquí el cierre desde una ventana visible.

## Prototipo de monitor virtual: 5 de septiembre de 2026

Compilación local x64 con Visual Studio 2026 Build Tools 18.9.2, MSVC 14.51.36231 y SDK/WDK NuGet 10.0.28000.2526. Se añadió el componente oficial de compilación WDK, incluidas las bibliotecas Spectre. Se generaron `WindowDeckDisplay.dll`, el INF procesado, el catálogo sin firmar y `windowdeck-display.exe`.

La autoprueba `windowdeck-display --self-test` pasó: checksum del EDID, resolución 1280 × 800, frecuencia exacta de 60 Hz, correspondencia EDID/modos de Windows y callback de creación tanto con éxito como con error. La generación del catálogo pasó su comprobación de aptitud para firma sin errores ni advertencias. Estas comprobaciones no cargan el driver ni crean una pantalla.

El build con análisis estático del driver y avisos tratados como errores terminó correctamente. También pasaron `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los 14 tests del workspace Rust. No se cambió el código del host, cliente o protocolo en esta iteración.

Tras autorización, `install-test.ps1` creó un certificado local de desarrollo, añadió su parte pública a Root y TrustedPublisher, firmó DLL y catálogo y verificó sus firmas y las entradas DLL/INF del catálogo. PnPUtil instaló el paquete como `oem98.inf`. Huella: `FFE0706D3221825F41678E40CE25727D938F101C`, caducidad 2026-12-04. La clave privada no se exportó. La consulta elevada encontró Secure Boot ya desactivado y ninguna opción TESTSIGNING en la entrada de arranque actual; no se modificaron esos ajustes ni se reinició el equipo.

En las activaciones locales aparecen `SWD\WINDOWDECK\WINDOWDECKDISPLAY` y `DISPLAY\WND0001\1&21883EBA&0&UID256`, ambos sin problema PnP. Terminar exclusivamente el proceso de prueba retira ambos dispositivos y conserva el monitor físico. Esto verifica retirada por vida del proceso, no el cierre interactivo con X. La autoprueba y los 14 tests Rust se repitieron satisfactoriamente.

Limitación reproducida: el monitor virtual queda inactivo. `DisplayConfigGetDeviceInfo` devuelve un modo preferido de 1280 × 800 y `SetDisplayConfig` valida/aplica sin error una solicitud extendida de 60 Hz, pero `QueryDisplayConfig`, GDI y una consulta en un proceso nuevo siguen mostrando solo la pantalla física activa (2560 × 1440, aproximadamente 120 Hz). Se reprodujo también esperando diez segundos tras la solicitud y usando `SDC_VIRTUAL_MODE_AWARE`. No se considera validado el escritorio extendido.

La traza WPP IddCx, decodificada con los símbolos oficiales de la DLL instalada, muestra inicio del adaptador, aceptación de los modos y llegada del monitor con `STATUS_SUCCESS`, sin errores explícitos ni llamadas de commit de modo/asignación de swap-chain. Por tanto, no acredita recepción de superficies. Las pruebas se ejecutaron en la sesión de consola y escritorio Default; la causa de la activación fallida aún no está determinada. Registros locales: `target/windows-idd-test/install.log`, `probe.log`, `trace.log` e `idd.txt` (ignorados por Git).

Se añadió `test-device.ps1` para diez ciclos PnP con cierre normal del handle. Su ejecución completa quedó pendiente al cancelarse la elevación de Windows; no debe confundirse con una prueba de extensión del escritorio. También siguen pendientes la validación interactiva en Configuración de pantalla, X, segunda instancia, suspensión y conexión al streaming. El [procedimiento del prototipo](../driver/windows-idd/README.md) incluye instalación, pruebas y retirada del paquete/certificado por identidad exacta.

En la comprobación manual posterior, el usuario oye el sonido de conexión, pero no aparece una pantalla adicional ni una opción para extender. Una nueva activación controlada confirma que la pila contiene `IndirectKmd`, `WUDFRd` y `SoftwareDevice`, selecciona `oem98.inf` y presenta adaptador y monitor sin problemas PnP ni petición de reinicio. El monitor sigue inactivo con la utilidad abierta. Esto reproduce un fallo local anterior al streaming; no demuestra todavía su causa.

Se probó de forma aislada `IDDCX_TRANSMISSION_TYPE_WIRED_OTHER`, como en el ejemplo oficial, manteniendo EDID, modos y callbacks. La variante 0.1.0.1 (`oem99.inf`) tampoco activó el monitor, ni automáticamente ni tras una solicitud explícita validada por Windows. Se descartó ese cambio y se retiró únicamente `oem99.inf`, conservando el paquete original `oem98.inf`. Copias firmadas recuperables: `target/windows-idd-test-v0/` y `target/windows-idd-test-v1/`; registros: `arrival-check.log`, `wired-check.log` y `rollback.log` bajo `target/windows-idd-test/`. No se modificó la seguridad de arranque. El mensaje de la utilidad ahora aclara que `DN_STARTED` solo confirma PnP y que debe permanecer abierta durante la comprobación.

Por decisión del usuario, se aplaza el ajuste fino de la retransmisión y sus opciones de calidad. Las mediciones pendientes de latencia y estabilidad siguen abiertas.

## Diagnóstico del monitor virtual: 6 de septiembre de 2026

Se retomó el fallo con el mismo entorno Windows 11 25H2, build 26200.9168. La compilación original y su autoprueba pasaron. La comparación con el [ejemplo Microsoft fijado](https://github.com/microsoft/Windows-driver-samples/blob/d5569c08aa2818c6240744bb47a00f67f20fdb54/video/IndirectDisplay/IddSampleDriver/Driver.cpp) no identificó una directiva esencial ausente en el INF. Se ejecutaron dos variantes controladas, compiladas con análisis estático WDK y firmadas mediante el certificado existente:

| Variante | Cambio respecto al control anterior | Resultado |
| --- | --- | --- |
| 0.1.0.2 | Sin EDID; se conservan 1280 × 800 a 60 Hz, total 1440 × 825 y 71.28 MHz | `DefaultModes` y llegada del monitor correctos; extensión validada/aplicada con código 0, pero ruta inactiva ocho segundos después. |
| 0.1.0.3 | Sobre 0.1.0.2, timings calculados como en el ejemplo: total 1280 × 800, hSync 48 kHz y pixelRate 61.44 MHz | Mismo fallo: modo aceptado y extensión con código 0, sin pantalla activa. |

Se comprobó `DEVPKEY_Device_DriverInfPath` y la versión enumerada por PnPUtil para confirmar qué paquete se cargaba. Windows reutilizó `oem99.inf` después de cada retirada; el nombre publicado no basta para distinguir los experimentos. Las trazas IddCx de ambas variantes muestran el callback de modos predeterminados y la llegada correcta del monitor, pero no commit de modo ni asignación de swap-chain. No se ha validado la entrega de superficies.

El auxiliar temporal `target/idd-display-check.cpp` identifica ahora el adaptador por su ruta, sin distinguir mayúsculas, para funcionar también sin nombre EDID. La primera ejecución de 0.1.0.2 descubrió un error en esa comparación y no llegó a solicitar extensión; no sirve como prueba de extensión. Tras corregir el auxiliar, se repitió la misma variante y se confirmó la solicitud con código 0 y la ruta inactiva. `--verify` devuelve 4 cuando no existe una ruta activa de WindowDeck; su éxito solo acredita actividad de la ruta, no resolución, frecuencia ni frames.

Se retiraron ambas variantes sin `/force`, se restauró el código original y se conservaron `oem98.inf` 0.1.0.0 y su paquete firmado. Después de cada ensayo desaparecieron adaptador y monitor virtuales, conservando VG27AQL5A y las GPU NVIDIA/AMD. Se cerró exclusivamente el proceso creado por cada ensayo; no se probaron X ni los diez ciclos normales. No se cambiaron Secure Boot/TESTSIGNING, no se reinició Windows y no se modificó el streaming. Tras la restauración pasaron de nuevo el build con análisis estático y generación de catálogo, la autoprueba, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los 14 tests Rust.

Evidencias: `target/idd-no-edid-check-2/check.log`, `target/idd-sample-timing-check/check.log` y las trazas `idd.etl` de esas carpetas; copias firmadas en `target/windows-idd-test-v2/` y `target/windows-idd-test-v3/`. Estos resultados descartan las dos modificaciones como soluciones en este entorno; no identifican todavía la causa raíz.

### Control con el ejemplo oficial y recuperación gráfica

Se compiló el ejemplo Microsoft completo del mismo commit fijado, conservando los fuentes originales del driver y de `IddSampleApp`, incluidos sus tres monitores. Solo se adaptaron las rutas de compilación, la fecha/versión del INF y el nombre del proveedor para identificar el paquete de prueba. El build con análisis estático y la firma con el certificado existente pasaron. Se verificó que el adaptador seleccionaba el paquete temporal `oem99.inf` 1.0.0.0.

Los tres monitores aparecieron sin problemas PnP. La extensión del primero, con su modo preferido de 2560 × 1440, se validó y aplicó con código 0, pero permaneció inactivo ocho segundos después. IddCx registró las tres llegadas correctas, sin commit de modo ni asignación de swap-chain. El ejemplo oficial reproduce el síntoma: las modificaciones propias de WindowDeck no bastan para explicarlo, pero todavía no se ha aislado la causa compartida. Se retiró exclusivamente el paquete temporal y se conservaron `oem98.inf` y los dispositivos físicos. Fuentes, paquete firmado, log y traza: `target/idd-official/`.

Con el driver original se capturaron además eventos DWM. El evento 201 (`SCHEDULE_DERIVEDISPLAYSET`) presentó `fSucceeded=0` durante la llegada y los cambios de topología; `DispBrokerDesktopSvc` estaba en ejecución. Es una señal para investigar la sesión gráfica, no una identificación de la causa. La captura DDisplay no aportó eventos útiles. Evidencias: `target/idd-environment-check/`.

Se envió la combinación de recuperación gráfica `Win+Ctrl+Shift+B` recomendada por [Microsoft](https://support.microsoft.com/en-us/windows/hardware/display-graphics/troubleshoot-external-monitor-connections-in-windows). `SendInput` aceptó los ocho eventos de teclado; eso no demuestra por sí solo que se reiniciara un componente gráfico. Tras esperar y repetir la extensión, Windows devolvió código 0 y la ruta siguió inactiva, sin commit ni swap-chain en IddCx. Los eventos DWM 201 siguieron indicando fallo. Evidencias: `target/idd-recovery-check/`.

Todos los procesos y trazas creados para estos ensayos quedaron cerrados. Se mantiene el driver original y la pantalla física activa; no se modificaron los controladores GPU, la seguridad de arranque ni el streaming. Se preparó `target/idd-after-restart-check.ps1` para el control tras reiniciar descrito a continuación. Véase [el punto de continuación](continuation.md).

### Comprobación tras reiniciar Windows

El usuario realizó el reinicio manualmente y confirmó «ya he reiniciado». La prueba elevada se ejecutó el 6 de septiembre de 15:13:18 a 15:13:35 y registró el último arranque a las 15:11:30, Windows build 26200, sesión de consola 1 y escritorio Default. Verificó la identidad del paquete original 0.1.0.0 y la selección de `oem98.inf`.

El modo preferido siguió siendo 1280 × 800. La validación y aplicación explícita de la extensión devolvieron 0, pero ocho segundos después WindowDeck seguía inactivo (`flags=8`, frecuencia `0/0`). GDI solo enumeró la pantalla NVIDIA física a 2560 × 1440 y 120 Hz. El script terminó con código 4, retiró su proceso y monitor, detuvo la traza y verificó que permanecían exactamente VG27AQL5A y las GPU NVIDIA/AMD iniciales.

Se contrastó el GUID/age de depuración de la DLL IddCx con los símbolos conservados antes de interpretar la traza. Tracefmt procesó 67 eventos sin pérdidas ni errores de formato; dos eventos no tienen formato disponible. Se observa llegada correcta del monitor y ningún commit de modo o asignación de swap-chain. El reinicio no resolvió el fallo y la entrega de superficies sigue sin validar. Evidencias: `target/idd-after-restart-0610c122284d4963b0702c5ef189c543/`, con `check.log`, `stdout.log`, `stderr.log` vacío, `idd.etl` e `idd.txt`.

### Selección de GPU y corrección del prototipo

Después del reinicio se amplió la captura de DWM, DxgKrnl, IddCx y Code Integrity. No apareció un rechazo que explicara por sí solo la activación fallida. Un auxiliar nativo enumeró los adaptadores con DXGI/D3DKMT y creó correctamente un dispositivo D3D11 con BGRA tanto en NVIDIA RTX 5070 Ti como en AMD Radeon Graphics. Los controladores observados eran 32.0.16.1656 y 32.0.21042.62, respectivamente; no se modificaron.

Se contrastó la GPU preferida mediante [`IddCxAdapterSetRenderAdapter`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/iddcx/nf-iddcx-iddcxadaptersetrenderadapter), antes de anunciar el monitor, manteniendo los modos originales:

| Paquete | Selección comprobada | Resultado |
| --- | --- | --- |
| 0.1.0.4 | Selección errónea del IDD por descripción/proveedor DXGI copiados de NVIDIA | IddCx rechaza el LUID con `STATUS_INVALID_PARAMETER_2`. No es una prueba válida de la GPU NVIDIA física. |
| 0.1.0.5 | AMD física | Pantalla activa 1280 × 800 a 60 Hz, commit de modo y recepción de superficies. |
| Original 0.1.0.0, repetido después de AMD | Selección predeterminada | Sigue inactivo. El resultado de AMD no se explica por una recuperación persistente del entorno. |
| 0.1.0.6 | NVIDIA física identificada por su LUID real en ese arranque | IddCx acepta la preferencia, pero la pantalla sigue inactiva, sin commit ni swap-chain. |
| Corrección 0.1.0.7 | Primera GPU física de renderizado por preferencia de bajo consumo; AMD en este equipo | Pantalla activa, recepción de superficies y estadísticas por frame sin el diagnóstico observado en 0.1.0.5. |

El contraste sitúa el bloqueo en la ruta de renderizado NVIDIA de este entorno concreto. Aún no se ha aislado el motivo interno ni probado otra combinación de Windows/GPU; no demuestra una incompatibilidad general de NVIDIA. La selección corregida enumera por `DXGI_GPU_PREFERENCE_MINIMUM_POWER` y consulta `KMTQAITYPE_ADAPTERTYPE` para exigir renderizado y excluir adaptadores indirectos y software. Evita fijar un proveedor o un LUID transitorio y conserva la elección predeterminada de Windows si no encuentra un candidato. La constante `RenderPreference` permite ajustar la preferencia en una compilación para otro equipo.

La primera prueba con AMD descubrió un segundo problema: IddCx emitió un diagnóstico tras cien frames sin informes de estadísticas. Hubo actividad posterior, por lo que ese mensaje por sí solo no demuestra que se detuviera el flujo. La corrección llama a [`IddCxSwapChainReportFrameStatistics`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/iddcx/nf-iddcx-iddcxswapchainreportframestatistics) por cada superficie procesada, con su número de presentación y tiempo QPC de adquisición; declara `DROPPED` porque el prototipo descarta las superficies y todavía no las transmite.

La versión 0.1.0.7 se compiló con análisis estático y validación de APIs del WDK, se firmó con el certificado existente y se instaló como `oem99.inf`. La aceptación verificó el paquete seleccionado y mantuvo la pantalla unos 45 segundos: ocho consultas confirmaron 1280 × 800 a 60/1 Hz, en posición 2560,0. Una segunda instancia fue rechazada con `0x800700b7` y la original siguió activa. La traza contiene 556 adquisiciones y 556 informes correctos, sin el diagnóstico de estadísticas ausentes. Tracefmt procesó 5104 eventos, sin pérdidas ni errores de formato y con tres eventos sin formato disponible. El modo de 60 Hz no acredita 60 frames nuevos por segundo en un escritorio estático. Al terminar se retiraron exclusivamente proceso/monitor propios y permanecieron los dispositivos físicos iniciales.

Se añadió `windowdeck-display --verify`, una consulta de rutas activas que comprueba también resolución y frecuencia: 0 si coinciden, 4 si falta la pantalla o el modo es distinto y 1 si falla la consulta. `--probe` la ejecuta antes del cierre normal; `test-device.ps1` exige ese resultado además de comprobar PnP, retirada y conservación física. El primer intento de ciclos detectó un problema del script: Windows PowerShell podía perder `ExitCode` después de `WaitForExit`. Se reprodujo con `--self-test` y se corrigió conservando el handle inmediatamente tras `Start-Process`. La repetición completa del 6 de septiembre terminó a las 16:14:17 con **diez ciclos superados**, cada uno con pantalla activa 1280 × 800 a 60 Hz y retirada normal. La prueba negativa de `--verify` sin monitor devolvió 4.

También pasaron el build, la autoprueba, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los 14 tests Rust. El streaming no se modificó. Al retomar la sesión, el usuario confirmó «veo la segunda pantalla» durante la prueba visual en Configuración. `--verify` confirmó de nuevo 1280 × 800 a 60/1 Hz en posición 2560,0. Tras reabrir la utilidad a petición del usuario e indicarle que pulsara X, confirmó «hecho»: se comprobó que el proceso había terminado, `--verify` devolvía 4 y no quedaban dispositivos virtuales presentes. El monitor VG27AQL5A y las GPU NVIDIA/AMD se conservaron con estado PnP OK. Quedan movimiento y recuperación de una ventana (el usuario no pudo observarlos), suspensión/bloqueo/cambio de usuario y transferencia al encoder; todavía no se envía esta pantalla a la Deck. Se conserva `oem98.inf` 0.1.0.0 como respaldo y se retiraron las variantes experimentales. No se cambiaron drivers de GPU, Secure Boot ni TESTSIGNING.

Evidencias locales, ignoradas por Git:

- `target/idd-fixed-check-a0e51df3df534813a5ed8bfae3137db6/`: aceptación corregida, identidad del paquete, segunda instancia, consultas y traza.
- `target/windows-idd-test/device-cycles.log` y `pnp-1.stdout.log` a `pnp-10.stdout.log`: intento inicial y diez ciclos superados tras corregir el script; stderr de los diez ciclos vacío.
- `target/idd-render-amd-check-910fad24d681462c84d2a26a90a8198a/`: AMD activa y diagnóstico inicial de estadísticas.
- `target/idd-render-nvidia-physical-check-67a44800b6db4759b75581f13d3f5284/`: NVIDIA física aceptada como preferida, pantalla inactiva.
- `target/idd-after-restart-47e6fe83cfb940589bccd7aa4f023910/`: control original repetido después de AMD, inactivo.
- `target/idd-render-nvidia-check-ed41631ced2e4205852db0d275267264/`: selección errónea del IDD, no usar como control NVIDIA física.
- `target/idd-activation-1c53933273154e12b76765a79b4f9baf/`: captura ampliada sin error causal aislado; `target/idd-adapters.cpp` y `.exe`: inventario de adaptadores y comprobación D3D11.

### Primer envío del escritorio virtual a la Deck

El 6 de septiembre se añadió el puente provisional `--virtual-h264`, descrito en el [ADR 0009](adr/0009-virtual-desktop-capture.md). `--capture-test 2` recibió una textura D3D11 de 1280 × 800 del monitor con nombre `WindowDeck`; FFmpeg 9.0.1 pudo recapturarlo con `gfxcapture`. Se añadió `--source` a la utilidad para consultar su dispositivo por identidad del adaptador y modo; el host pasa el HMONITOR correspondiente a FFmpeg. Sin monitor, `--source` devuelve 4. El driver instalado no cambió.

Pasaron build C++/autoprueba, Clippy y los 16 tests Rust, incluidos argumentos de la nueva ruta y rechazo de respuestas inválidas de `--source`. El usuario prefirió una prueba directa con su Steam Deck a la comprobación sintética de píxeles, que quedó sin ejecutar.

La Deck `192.168.1.18` conserva el Flatpak construido desde `3b0cd4f`; no hubo cambios en cliente/protocolo ni instalación nueva. Se inició por SSH en su sesión gráfica. El primer intento usó la antigua IP del PC `192.168.1.23` y no conectó; `ipconfig` confirmó `192.168.1.12` y la repetición conectó al puerto 48150. El host registró `virtual_capture_selected`, dispositivo `\\.\DISPLAY13`, nombre `WindowDeck`, fuente `windows_graphics_capture` y formato 1280 × 800 a 60 FPS. El primer paquete salió a los 303 ms de arrancar el envío (medida local, no latencia visual).

En torno a los 116 segundos, el encoder registró 6947 frames y 59.99 FPS; la Deck seguía recibiendo datos al menos hasta los 123 segundos. No aparecieron errores en los registros consultados. El filtro `fps=60` puede duplicar actualizaciones de WGC: esas métricas no prueban 60 imágenes nuevas por segundo. Se mantienen libx264 y el objetivo de 16 Mbps; el bitrate real depende del contenido. El usuario confirmó después que funciona como esperaba: ve el escritorio extendido y la ventana trasladada en la Deck. Señaló latencia notable frente a la pantalla nativa y acordó tenerla en cuenta para futuras mejoras. Es una valoración perceptiva, sin medida en milisegundos ni causa aislada; los ajustes se mantienen. Quedan pendientes las pruebas de cierre/desconexión durante el vídeo, recuperación de ventanas al retirar el monitor y estabilidad prolongada.

La prueba se dejó activa para el usuario el 6 de septiembre; al retomar el día 7 ya no había procesos locales ni monitor virtual. Evidencias históricas: `target/virtual-live-7a456af4883d4114a4abbb1e3bf827c7/host.log`; registro de la Deck: `/home/deck/Downloads/windowdeck-live.log`, unidad temporal `windowdeck-live-test.service`. Véase el [punto de continuación](continuation.md) para el estado actual.

### Desconexión y nueva conexión del 7 de septiembre

Se repitieron build Rust, formato, Clippy, los 16 tests y la autoprueba de la utilidad: todos pasan. Se activó WindowDeck a 1280 × 800 y 60 Hz, posición -1280,635, y se conectó la Deck al mismo host `192.168.1.12:48150`.

Parar la unidad `windowdeck-resume-test.service` no terminó el cliente Flatpak: el proceso y la recepción continuaron. Por tanto, ese primer intento no probó la desconexión. Tras identificar y cerrar la instancia específica mediante `flatpak kill`, la consulta de procesos a los tres segundos confirmó ausencia de FFmpeg, con host y utilidad activos y modo virtual conservado. El host registró `session_closed` con error de socket 10053. Una conexión pendiente del intento anterior se cerró por EOF; la nueva apertura posterior recibió vídeo durante al menos 47 segundos, con encoder cercano a 60 FPS.

Esto valida desconexión abrupta del cliente y una nueva sesión en el mismo host. No prueba cierre interactivo de FFplay, pérdida de red sin cierre TCP ni el temporizador de encoder bloqueado.

Después, el usuario pulsó X en la utilidad durante el vídeo y confirmó que la ventana de prueba volvió a quedar accesible en la pantalla principal. A `timestamp_ms=1788765632435`, el host registró `virtual_capture_stopped reason="monitor_removed_or_changed"`; 23 ms después cerró la sesión con aviso `ffmpeg terminó con exit code: 1`, resultado del cierre deliberado del proceso. Esos 23 ms separan eventos del host: no miden el tiempo desde la pulsación de X. La Deck registró `h264_stream_stopped` con 306994600 bytes y 14313 chunks tras unos 173 segundos de sesión.

La comprobación posterior confirmó `--verify` con código 4 y ausencia de utilidad, FFmpeg y dispositivos virtuales. El monitor físico VG27AQL5A y las GPU NVIDIA/AMD permanecieron con estado PnP OK; `flatpak ps` ya no mostró WindowDeck en la Deck. El host sigue escuchando. Quedan así comprobadas una retirada durante vídeo y la recuperación visual de una ventana en este equipo; suspensión, bloqueo y cambio de usuario siguen pendientes.

Evidencias: `target/virtual-resume-41cb3d456cd8422180dde8195477dfde/host.log` y `/home/deck/Downloads/windowdeck-resume.log` en la Deck. Los ajustes de calidad y latencia no cambiaron.

## Vida automática del monitor (7 de septiembre)

La opción `--auto-virtual-h264` y el controlador local `windowdeck-display --broker` se describen en [ADR 0010](adr/0010-automatic-display-lifetime.md). No se reinstaló el driver 0.1.0.7. El host se inició desde un shell sin privilegios de administrador; el broker se elevó por separado.

Pasaron tres ciclos de `test-auto-display.ps1`: petición normal, rechazo de una segunda petición conservando la primera, retirada normal, terminación del auxiliar y conservación de dispositivos físicos. La primera ejecución completó dos ciclos, pero encontró el pipe ocupado al iniciar el tercero antes de terminar la retirada asíncrona. Tras añadir una espera acotada de un segundo, pasó la repetición completa. Registro: `target/windows-idd/auto-display-test.log`.

`test-auto-host.ps1` comprobó que la ausencia de FFmpeg devuelve el host a la escucha y retira el monitor. También terminó su propio host durante vídeo en loopback y comprobó la retirada del monitor y la salida del encoder. Evidencia final: `target/auto-host-test-f327f57482774aed8347d46465217cdc/`, incluida una nueva negociación tras el fallo del encoder. El ensayo inicial se conserva en `target/auto-host-test-c81f922a6f42454087360215ceb76336/`. Estas pruebas verifican fallos de proceso, no pérdida de red sin cierre TCP.

En la Deck, una primera conexión activó el escritorio 1280 × 800 a 60 Hz en posición -1280,635 y recibió vídeo durante unos 246 segundos, con encoder cercano a 60 FPS. Cerrar la instancia concreta mediante `flatpak kill` retiró el monitor. Una segunda conexión al mismo host volvió a activar la pantalla y recibió vídeo; su cierre remoto también retiró monitor, auxiliar y encoder. Host y broker quedaron en reposo.

Una tercera conexión abrió el cliente en modo ventana, sin `--fullscreen`, mediante `windowdeck-auto-manual-close.service`. Tras unos 92 segundos de vídeo, el usuario cerró la X de «WindowDeck H.264» en la Deck y confirmó «correcto, se recupera perfectamente» respecto a la retirada de la pantalla y recuperación de la ventana de prueba. El host registró `virtual_display_released` a `timestamp_ms=1788767971678` y cierre de sesión con error de socket 10053. La consulta posterior confirmó `--verify` con código 4 y ausencia de FFmpeg y del auxiliar; permanecieron host y broker en reposo. Este cierre sí fue interactivo, sin `flatpak kill`. No acredita suspensión, bloqueo, cambio de usuario ni pérdida de conectividad sin cierre TCP. Registro del cliente: `/home/deck/Downloads/windowdeck-auto-manual-close.log`.

Evidencias: `target/auto-live-31af49a835584b1395669c2439c24f15/host.log`, `broker.log`, `deck-reconnect.log` y `/home/deck/Downloads/windowdeck-auto.log` en la Deck. Se conservan 1280 × 800, 60 FPS y objetivo 16 Mbps; el bitrate medido puede superar ese objetivo. No se midió ni optimizó latencia.

## Frames directos UMDF → host (7 de septiembre)

Se compiló, firmó e instaló 0.1.0.8, conservando el paquete firmado 0.1.0.7. Durante las pruebas se verificaron `DEVPKEY_Device_DriverVersion=0.1.0.8` y `DEVPKEY_Device_DriverInfPath=oem100.inf`. El [ADR 0011](adr/0011-driver-frame-transfer-probe.md) describe la memoria compartida y los límites de esta referencia CPU.

La primera lectura obtuvo 120 frames, con 95 coincidencias de muestras del patrón y sin errores reportados por el driver. Una ejecución posterior con diagnóstico por índice confirmó que las no coincidencias se concentraban en un prefijo inicial. Se hizo el dibujo del patrón con un buffer previo y coordenadas conscientes del DPI, y se endureció la validación: tras la primera coincidencia, cualquier frame no coincidente hace fallar la prueba.

`test-driver-frames.ps1` pasó con esa validación estricta: primera lectura de 120 frames, terminación del host en un segundo ciclo y otra lectura de 120 frames al reconectar. Las lecturas completas verificaron 100 y 104 frames consecutivos después de 20 y 16 frames iniciales, respectivamente, con las tres fases del patrón presentes. Los frames se leen en Rust; no se guardan imágenes. La validación usa muestras de píxeles y no compara cada píxel.

Se recibieron 26,54 y 24,95 FPS. Adquisición→publicación medias: 9,310 y 9,130 ms; copia CPU al mapping: 188 y 183 µs. El patrón GDI, polling, readback y stdout forman parte del ensayo: no permite concluir un límite de 25 FPS del driver ni una mejora de latencia respecto a WGC. No se codificaron estos frames para la Deck.

Evidencia final: `target/driver-frames-test-e6de04bd6ed841e492972e954e98e28d/`, con logs y selección PnP por ciclo. Ensayos anteriores: `target/driver-frames-test-fe927cb6eaed4b409f6d20e234ad87ae/` y `target/frame-probe-a3a489f15d7a4d55abc47f532f8f6821/probe-detail.log`. El directorio de instalación conserva transcripción y logs de ambos brokers.

También pasaron build/análisis WDK/autoprueba nativa, 19 tests Rust, formato y Clippy. Se repitieron tres ciclos automáticos y la regresión de fallo de encoder/terminación del host durante vídeo WGC, conservada en `target/auto-host-test-326aa1c461324bc392c728737d758db2/`. Al terminar no quedaron monitor virtual, encoder ni lector; VG27AQL5A y ambas GPU seguían con PnP OK. El host habitual y ambos brokers quedaron escuchando, sin dispositivo activo.

## Texturas D3D11 compartidas (7 de septiembre)

La versión 0.1.0.9 (`oem101.inf`) pasó `test-driver-frames.ps1 -Gpu` y la regresión CPU sin ese parámetro. Cada ruta recibió dos tandas de 120 frames con validación estricta del patrón y una terminación del host entre ellas. La nueva conexión creó recursos nuevos y las pruebas conservaron los dispositivos físicos. Resultados, interpretación y rutas exactas de las evidencias en [ADR 0012](adr/0012-shared-d3d11-frame-probe.md#resultados-locales).

Adquisición hasta píxeles CPU en el lector: aproximadamente 17 ms en CPU y 24 ms con texturas compartidas y readback en el auxiliar. La publicación GPU de unos 0,10 ms no mide la disponibilidad final de píxeles ni la latencia de la Deck. El ensayo conserva stdout, patrón GDI y polling; no permite elegir un encoder GPU ni concluir 60 FPS.

También pasan compilación/análisis WDK/catálogo/autoprueba, 19 tests Rust, formato, Clippy, tres ciclos automáticos y regresión WGC de fallo del encoder/terminación del host. Al terminar se cerraron los tres brokers propios; `--verify` devuelve 4 y el monitor físico VG27AQL5A y las GPU NVIDIA/AMD siguen con PnP OK. No se inició una nueva sesión en la Deck. Copia firmada de respaldo 0.1.0.8 en `target/windows-idd-test-v0.1.0.8/`.

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
## H.264 experimental desde frames CPU — 8 de septiembre de 2026

`driver/windows-idd/test-auto-host.ps1 -DriverCpu` usa el broker elevado `--frame-broker` previamente abierto. Sin el switch usa el broker WGC `--broker`. El host y FFmpeg se ejecutan sin elevar. El arnés comprueba encoder ausente, dos sesiones de vídeo de unos ocho segundos con reconexión al mismo host, 120 frames decodificados de cada sesión con FFmpeg, H.264 1280 × 800 y frecuencia declarada 60/1 mediante FFprobe, terminación inesperada del host, cierre de procesos y conservación de dispositivos físicos. Guarda el vídeo del escritorio virtual en el directorio de evidencias local.

Aceptación CPU: `target/auto-host-test-3bc9b983eb1d4bceb9d7ebea23697fe5/`. Regresión WGC: `target/auto-host-test-60ffe67270054cf5bef84aa1cd1cfd6d/`. Todos los escenarios pasan, incluidos los contadores explícitos de 120 frames decodificados y los estados PnP finales. Primera ejecución, antes de añadir esos dos controles: CPU `target/auto-host-test-b57c7810bde8432eb6a23c51cb627c30/`; WGC `target/auto-host-test-4906d395a68145768af7c6a25475ef71/`.

Pasan compilación/análisis/catálogo/autoprueba nativos, build Rust, los 19 tests, formato y Clippy. No se reinstaló el driver. Ambos brokers temporales se cerraron al terminar y no queda una sesión activa. La cadencia nominal CPU repite frames; las primeras muestras del encoder rondaron 57 FPS con un escritorio mayoritariamente estático. No equivale a validar 60 superficies nuevas por segundo, movimiento, cursor o latencia visual en la Deck. Véase [ADR 0013](adr/0013-driver-cpu-h264.md).
## Aceptación visual de la ruta CPU en Steam Deck — 8 de septiembre de 2026

Se abrió el Flatpak ya instalado mediante SSH, unidad temporal `windowdeck-cpu-live.service`, en modo ventana y conectado a `192.168.1.12:48150`. El usuario confirmó: «la calidad y la latencia son aceptables al cerrar la ventana se recupera perfectamente». El cierre fue interactivo; no se terminó el cliente por SSH.

La sesión duró aproximadamente 356 segundos. El cliente registró `h264_stream_stopped` con 636.138.796 bytes y 25.705 chunks. El host detectó `virtual_capture_stopped reason="client_disconnected"` y cerró la sesión con el error de socket 10053. Los últimos registros del encoder indicaban aproximadamente 50,32 FPS efectivos; no confundirlos con los 60 FPS configurados ni interpretar la valoración visual como latencia medida.

Tras el cierre, `--verify` devolvió 4; no quedaban encoder, auxiliar CPU ni instancia Flatpak de WindowDeck. Se compararon los dispositivos PnP con el estado previo: monitor físico, NVIDIA y AMD conservados con estado OK, sin monitor virtual. Host 8012 y broker CPU 10496 permanecieron en reposo; comprobar identidad antes de usar esos PID.

Evidencias: `target/deck-cpu-live-57d03b14186f43d2ab175225cf9b0c34/`, con logs del host y estados de dispositivos antes/después; Deck `/home/deck/Downloads/windowdeck-cpu-live.log`. No hubo reinstalación del driver, cambio de calidad, comparación A/B controlada con WGC ni validación separada del cursor.
## Apagado de la Deck durante vídeo — 8 de septiembre de 2026

El primer intento de suspensión fue un apagado, según aclaró el usuario. Confirmó recuperación perfecta de ventanas en Windows. El host cerró la sesión por TCP 10054; el observador registró `verify=4` y ausencia de encoder y auxiliar CPU. La Deck volvió a responder por SSH después de arrancar. Esto no valida suspensión/reanudación. Evidencia: `target/deck-suspend-f06ca221eafd4f60b5e1d569643562f2/`; logs del host en `target/deck-cpu-live-57d03b14186f43d2ab175225cf9b0c34/`. Se preparó una repetición separada en `target/deck-suspend-retry-72c33b15279247e6b211f8ae3c95d1ab/`.
## Suspensión real: limpieza correcta y reconexión ausente — 8 de septiembre de 2026

El journal de `systemd-suspend.service` confirma suspensión de la Deck de 09:22:27 a 09:23:29 CEST, unos 62 segundos. Durante la interrupción, el host registró `encoder_stalled` y cerró la sesión con timeout de socket 10060 a las 07:22:39 UTC. El observador confirmó monitor inactivo (`verify=4`) y ausencia de encoder/auxiliar CPU. Al despertar, el cliente terminó con `Resource temporarily unavailable (os error 11)`; el usuario confirmó que la ventana se cerró sin recuperar la conexión.

La revisión de `receive_h264_test` confirma una sola llamada a `connect`, sin reintentos al terminar la sesión. La reconexión del modo RGB332 no se aplica a H.264. Pasa la limpieza de la sesión en Windows; falla la recuperación automática de vídeo. Pendiente implementar reconexión H.264 que respete el cierre voluntario de la ventana, actualizar el Flatpak y repetir ambas pruebas.

Evidencia: `target/deck-suspend-retry-72c33b15279247e6b211f8ae3c95d1ab/observations.jsonl`; host `target/deck-cpu-live-57d03b14186f43d2ab175225cf9b0c34/host.log`; cliente `/home/deck/Downloads/windowdeck-suspend-retry.log`. Observador detenido al finalizar; host y broker conservados en reposo.

## Reconexión H.264 tras suspensión — 8 de septiembre de 2026

El Flatpak actualizado desde commit `52c4fca` se probó en la Deck durante una suspensión real. El cliente detectó `Resource temporarily unavailable (os error 11)`, mantuvo la ventana abierta y reconectó: `h264_reconnected` a las 07:25:03 UTC, primer paquete de la sesión nueva 448 ms después. El usuario confirmó que la imagen se recuperó, con latencia inicial notable que se estabilizó, y consideró aceptable el resultado.

Tras cerrar la prueba se detuvo la instancia Flatpak y el broker CPU. `--verify` devolvió 4; no quedaron monitor virtual, FFmpeg ni auxiliar. Evidencia: `target/deck-cpu-live-e34b5af439ee4f249f437616939dbdb7/host.log` y `/home/deck/Downloads/windowdeck-reconnect-final.log`. La sesión nueva se negocia después de la suspensión; no se conserva el socket TCP original. Quedan como mejoras la recuperación visual inicial y las pruebas de bloqueo/cambio de usuario.

## Bloqueo de Windows y recuperación H.264 — 8 de septiembre de 2026

Con el Flatpak actualizado y la ruta CPU activa, se bloqueó Windows con `Win + L` y se desbloqueó después. El cliente detectó la interrupción, mantuvo la ventana y reconectó una sesión H.264 nueva: `h264_connection_lost` a las 07:34:39 UTC, `h264_reconnected` a las 07:35:02 y primer paquete 581 ms después. El usuario confirma que bloqueo y suspensión funcionan a la perfección; al recuperar la imagen la latencia inicial es similar a la del arranque y se estabiliza.

Tras cerrar la sesión se detuvieron cliente, host y broker; `--verify` devolvió 4 y no quedaron monitor virtual, FFmpeg ni auxiliares. Evidencia: `target/deck-cpu-live-1da441ce883a4fbc8ac86dedec668d78/` y `/home/deck/Downloads/windowdeck-lock-test.log`. Quedan comparación A/B con WGC, recuperación visual inicial y cambio de usuario prolongado.
## Comparación A/B CPU frente a WGC en Steam Deck — 8 de septiembre de 2026

Se ejecutaron dos sesiones consecutivas con la misma Deck, resolución 1280 × 800, 60 FPS configurados, libx264 y objetivo de 16 Mbps. La sesión CPU directa entregó el primer paquete en 494 ms y mantuvo unos 50,26 FPS efectivos. Evidencia: `target/deck-cpu-live-b6c32cd7121140caa2ef79eddcf19967/` y `/home/deck/Downloads/windowdeck-ab-cpu.log`.

La sesión WGC entregó el primer paquete en 915 ms y mostró una latencia perceptiblemente mayor. El usuario confirma que la ruta CPU era considerablemente mejor y que WGC resultó demasiado lenta. Evidencia: `target/deck-wgc-live-e76dcc6c2fcf4334b20ebe8fef577844/` y `/home/deck/Downloads/windowdeck-ab-wgc.log`.

Resultado provisional: priorizar frames CPU directos para el modo experimental de baja latencia y conservar WGC como referencia y fallback. Esta comparación no mide latencia absoluta con cámara ni controla idénticamente el contenido. Ambas sesiones se cerraron y `--verify` confirmó la retirada del monitor virtual.
## Evaluación de encoders H.264 para rendimiento — 8 de septiembre de 2026

Se compararon encoders con la ruta real de frames CPU del driver. `libx264` mantiene unos 50 FPS efectivos. `h264_amf` (AMD) entregó el primer paquete en 411 ms y pasó de unos 35 FPS iniciales a aproximadamente 49 FPS. `h264_nvenc` (NVIDIA) entregó el primer paquete en 171 ms y rondó 60 FPS con escritorio estático, pero cayó a unos 30 FPS cuando llegaron frames con movimiento. Las pruebas sintéticas de 3 segundos a 1280 × 800 mostraron que AMF, NVENC y Media Foundation superan tiempo real, aunque ese resultado no reproduce la transferencia ni el contenido del escritorio.

Conclusión: no cambiar todavía el valor predeterminado `libx264`; ningún hardware probado ofrece una mejora sostenida con movimiento. El cuello de botella probable es la transferencia BGRA y el backpressure entre el auxiliar, FFmpeg y el socket. Se añadió `WINDOWDECK_H264_ENCODER` para futuras pruebas sin recompilar. Las sesiones reales se cerraron y `--verify` confirmó el monitor virtual inactivo.
## Instrumentación de backpressure — 8 de septiembre de 2026

Se limitó la cola de entrada de FFmpeg a dos frames y se añadieron parámetros x264 de baja latencia (`sync-lookahead=0`, `rc-lookahead=0`, una referencia y sin scenecut). El auxiliar registra `write_mean_us` y `write_max_us` para cuantificar cuánto bloquea la escritura BGRA cuando el encoder no alcanza la cadencia. El perfil predeterminado sigue siendo libx264; la instrumentación no cambia por sí sola la calidad ni la resolución. Pasan build WDK/autoprueba, los 22 tests Rust, formato y Clippy.

## Cierre de FPS — 2026-09-08

El usuario acepta la transmisión durante el uso normal y considera esperables los FPS bajos al arrancar los scripts y establecer comunicación. Se pausa la optimización: no continuar automáticamente con encoder GPU ni más pruebas de rendimiento. Se conserva CPU H.264/libx264 con temporización QPC, temporizador de alta resolución y métricas por intervalo. Esta aceptación no certifica 60 FPS presentados de forma continua.

Detalle de cambios, evidencia, correcciones de interpretaciones anteriores y validación: [revisión de cadencia](fps-pacing-review.md). La última sesión quedó activa para el usuario en `target/deck-cpu-live-611371701eef48a89fb6518ee6b2f95a/`; verificar identidad de procesos antes de usar PID guardados. El Flatpak b60158e sirve para estos cambios del host. Compilar una DLL no actualiza el driver instalado.

## Corrección de desconexiones con el cliente instalado — 10 de septiembre de 2026

El host 0.2.0 negociaba correctamente el respaldo CPU/MPEG-TS con el Flatpak 0.1.0
instalado en la Deck (commit Flatpak `a78bd7428e8b9d8be77f387a329ceb610a57e019afaa4b1d7b65d4073199e681`).
Sin embargo, la nueva política de 250 ms se aplicaba también a la escritura TCP,
la espera del productor y la antigüedad de los chunks de esa ruta. Durante el
arranque se superaba ese límite: el host cortaba el envío y el cliente recibía
`failed to fill whole buffer`, reconectando repetidamente. Se reprodujeron
14 pérdidas de conexión antes de detener la prueba.

La corrección conserva la cola de dos chunks y el límite histórico de diez
segundos para los bloqueos de la ruta MPEG-TS. No se descartan bytes H.264. El
límite de 250 ms de las unidades de acceso de la ruta nativa sigue separado.
El cambio está en el host; no se actualizó el Flatpak ni se reinstaló el driver.

Verificación con la Deck real, conectada por SSH a `192.168.1.18`:

- Primera apertura mediante descubrimiento automático: 207 segundos, 434.973.720
  bytes recibidos y ninguna pérdida de conexión. Se pausó únicamente el proceso
  del cliente durante dos segundos con SIGSTOP/SIGCONT y continuó la misma sesión.
  Hubo escrituras TCP de hasta 963.802 µs, superiores al antiguo límite.
- Cierre de esa instancia mediante `flatpak kill`: `--verify` devolvió 4; se
  retiró el monitor y terminaron FFmpeg y el auxiliar de captura. Esto comprueba
  el cierre de proceso, no sustituye una aceptación visual mediante la X.
- Segunda apertura automática en el mismo host: unos 54 segundos y 105.575.724
  bytes recibidos, sin reconexiones inesperadas. La parada solicitada al host
  produjo `h264_session_stopped` y `h264_stream_stopped` en la Deck. El host
  registra la terminación forzada de su FFmpeg con código 1 durante esa parada
  deliberada; el cliente recibió Stop y terminó normalmente.
- Estado final: sin monitor virtual, host, brokers, FFmpeg ni cliente de prueba.
  Los procesos de otras aplicaciones de la Deck se conservaron.

Pasan 29 pruebas Rust (una prueba de multicast queda excluida), Clippy con todas
las funciones, formato, compilación release y autoprueba multimedia. Las nuevas
regresiones fuerzan una pausa de 800 ms en el consumidor y verifican que se
conservan todos los bytes y el orden de los nueve chunks, y que los datos de
una sesión bloqueada más de diez segundos siguen rechazándose.

Evidencias locales: `target/reconnect-before-0889999b9d5c40d19daed078aa52c243/`
y `target/reconnect-after-b84b6f0c0dd64417a03708e74efeee1b/`, incluidos los registros
copiados de la Deck. SHA256 del host corregido:
`DA362C48232621AD9D75DBF305FFF68EDFC08FBBA2100B210CAA855A51C8513D`.
Esta prueba valida la compatibilidad CPU con el cliente instalado; no acredita
la nueva ruta multimedia nativa en la Deck ni mide la latencia visual.
