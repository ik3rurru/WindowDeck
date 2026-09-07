# WindowDeck Display: prototipo de monitor virtual

Primer paso del hito 4, independiente del host y del cliente Rust. Adaptación del [ejemplo oficial IndirectDisplay](https://github.com/microsoft/Windows-driver-samples/tree/d5569c08aa2818c6240744bb47a00f67f20fdb54/video/IndirectDisplay), fijado al commit `d5569c08aa2818c6240744bb47a00f67f20fdb54`.

El código declara un adaptador `WindowDeck Display`, un monitor con nombre EDID `WindowDeck` y un único modo de 1280 × 800 a 60 Hz. El campo de nombre del EDID está limitado a 13 caracteres. El worker D3D11 consume y libera las superficies; **todavía no las transmite directamente**. El host puede recapturar ese escritorio y enviarlo con `--virtual-h264`, como se explica en el [README principal](../../README.md#enviar-el-monitor-virtual-a-la-deck). Se conservan los ajustes de calidad H.264 existentes.

Estado local (2026-09-06): **escritorio extendido activo a 1280 × 800 y 60 Hz** con la versión 0.1.0.7 (`oem99.inf`). Se comprobaron 45 segundos de actividad y 556 superficies con estadísticas aceptadas, una segunda instancia rechazada y diez ciclos de activación/retirada normal. La pantalla física se conserva. Esto valida el modo activo y la recepción de imágenes, no un flujo sostenido de 60 frames nuevos por segundo ni el vídeo en la Deck.

En este equipo la ruta con NVIDIA no activa el monitor, mientras AMD integrada sí lo hace. El driver prefiere ahora una GPU física de bajo consumo mediante DXGI/D3DKMT, excluyendo adaptadores virtuales y de software antes de llamar a `IddCxAdapterSetRenderAdapter`. No fija una marca ni un LUID entre reinicios. La constante `RenderPreference` de `Driver.cpp` permite ajustar la preferencia al compilar; no se ha validado esta elección en otros equipos. El worker informa también de las estadísticas de los frames que descarta.

Antes se probaron sin éxito cambios de EDID/timings, el ejemplo Microsoft completo y un reinicio normal. Se conserva `oem98.inf` 0.1.0.0 para recuperación; los paquetes experimentales intermedios se retiraron. El [registro de pruebas](../../docs/testing.md#selección-de-gpu-y-corrección-del-prototipo) y el [punto de continuación](../../docs/continuation.md) detallan el contraste NVIDIA/AMD y los límites de las comprobaciones.

Actualización del 7 de septiembre: instalado y seleccionado `oem100.inf` **0.1.0.8**, que añade el [prototipo de frames directos](../../docs/adr/0011-driver-frame-transfer-probe.md). Mantiene la ruta anterior cuando no se solicita el mapping de prueba. Se conserva el paquete firmado 0.1.0.7 en `target/windows-idd-test-v0.1.0.7/` y en Driver Store (`oem99.inf`), además del respaldo original 0.1.0.0. Las pruebas de activación y captura anteriores vuelven a pasar.

Para repetir la transferencia con el broker `--frame-broker` abierto como administrador, host compilado y sin sesión activa:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/test-driver-frames.ps1
```

Comprueba dos lecturas de 120 frames, terminación del host, reconexión, versión seleccionada y conservación de los dispositivos físicos. Los resultados quedan en un directorio nuevo `target/driver-frames-test-…`. No instala paquetes ni guarda píxeles. No confundir `--frame-source`, que emite BGRA binario para el host, con un comando de terminal para uso manual.

## Compilar y comprobar sin instalar

Requisitos: Windows 11 x64 y Visual Studio 2026 Build Tools con C++ y el componente **Windows Driver Kit build tools** (`Component.Microsoft.Windows.DriverKit.BuildTools`), incluidas sus bibliotecas Spectre. La [distribución oficial WDK por NuGet](https://learn.microsoft.com/en-us/windows-hardware/drivers/install-the-wdk-using-nuget) proporciona SDK y WDK `10.0.28000.2526`; no es necesario instalar el IDE completo en el entorno comprobado.

Desde la raíz del repositorio:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/build.ps1
```

La excepción de política solo afecta a ese proceso. El script localiza MSBuild de 64 bits, descarga NuGet 6.14.0 si falta y verifica su firma Microsoft, restaura las versiones fijadas, compila ambos proyectos y ejecuta `--self-test`. Los paquetes y binarios quedan bajo `target/`; no instala dispositivos, no confía en certificados ni modifica el arranque o la topología de pantallas.

Resultados:

- `target/windows-idd/windowdeck-display.exe`: utilidad de activación y autoprueba.
- `target/windows-idd/WindowDeckDisplay/`: DLL, INF, CAT **sin firmar** y licencia.

Para repetir únicamente la autoprueba, sin privilegios de administrador ni driver instalado:

```powershell
./target/windows-idd/windowdeck-display.exe --self-test
```

Comprueba checksum y timing preferido del EDID, correspondencia con los modos anunciados a Windows y propagación de éxito/error del callback de creación. No crea ningún dispositivo. El build del driver ejecuta también las comprobaciones WDK del INF, APIs y catálogo. El CI Rust existente no compila aún este prototipo C++.

## Firma e instalación local de pruebas

El build continúa generando un paquete sin firmar. El script separado `install-test.ps1` crea o reutiliza un certificado de desarrollo y confía en su parte pública en los almacenes locales Root y TrustedPublisher. **Ejecutarlo solo en el equipo de desarrollo, con autorización para instalar el driver y añadir esa confianza.** No equivale a la firma de distribución de Microsoft; véase la [guía oficial de firma de prueba](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/signing-a-driver-during-development-and-testing).

La clave privada RSA de 3072 bits permanece no exportable en CurrentUser/My; el certificado caduca a los 90 días. La copia pública queda en `target/idd-signing/WindowDeckDisplay.cer`. El script firma una copia de la DLL, regenera y firma el catálogo, verifica ambas firmas y su correspondencia con DLL/INF, y llama a PnPUtil. El paquete firmado y el registro de instalación quedan en `target/windows-idd-test/`; los originales de compilación se conservan.

Desde la raíz del repositorio, cerrar antes cualquier instancia de `windowdeck-display` y ejecutar en PowerShell **como administrador**:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/install-test.ps1
./target/windows-idd/windowdeck-display.exe --run
```

La versión corregida quedó instalada como `oem99.inf` 0.1.0.7, con certificado `FFE0706D3221825F41678E40CE25727D938F101C` (caducidad 2026-12-04). También permanece el original `oem98.inf` 0.1.0.0 para recuperación. La consulta elevada encontró Secure Boot ya desactivado y ninguna opción TESTSIGNING en la entrada actual. **No se modificaron esos ajustes.** El usuario reinició manualmente durante el diagnóstico. No se presupone que otros equipos acepten esta firma: si su política exige cambios de arranque, detenerse y acordarlos por separado.

PnP debe elegir este paquete para el hardware ID `WindowDeckDisplay`. La utilidad espera el callback y el estado `DN_STARTED`; informa de errores PnP en vez de dar por iniciado un dispositivo sin driver. Ese estado puede preceder a la llegada asíncrona del monitor y no confirma la extensión del escritorio. Conserva el identificador de instancia que imprime la utilidad y el nombre publicado `oemNN.inf` que devuelve PnPUtil.

Mantener la utilidad abierta mientras se comprueba Configuración → Sistema → Pantalla. La extensión fue automática en las pruebas locales; si hace falta, seleccionar **Extender estas pantallas**. No convertir el prototipo en pantalla principal. En otra terminal, `./target/windows-idd/windowdeck-display.exe --verify` comprueba el escritorio sin cambiarlo: devuelve 0 para 1280 × 800 a 60 Hz, 4 si falta o tiene otro modo y 1 ante un error de consulta. Pulsar **X** en la utilidad para solicitar la retirada. El dispositivo tiene la vida del handle: terminar el proceso también solicita su desconexión. Windows realiza la retirada de forma asíncrona; el paquete instalado permanece disponible para la siguiente ejecución.

La utilidad actualizada incorpora `--source`: devuelve únicamente el nombre GDI (`\\.\DISPLAY…`) de la ruta WindowDeck activa a 1280 × 800 y 60 Hz. Usa los mismos códigos de salida que `--verify`; no necesita administrador. Es la consulta que utiliza el host para identificar la fuente virtual sin asumir un índice. Recompilar esta utilidad no requiere reinstalar el driver.

## Prueba manual de aceptación

La [ruta automática](../../docs/adr/0010-automatic-display-lifetime.md) usa `windowdeck-display --broker` como administrador y `windowdeck-host --auto-virtual-h264` sin elevar. El broker permanece sin pantalla hasta recibir una sesión y la retira al liberarse esa sesión. Mantén el broker abierto; no ejecutes `--run` a la vez. Recompilar la utilidad basta: no hay que reinstalar el driver.

Con el broker ya abierto en el mismo inicio de sesión, estas pruebas verifican ciclos y salidas del host, respectivamente:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/test-auto-display.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/test-auto-host.ps1
```

Ejecutarlas sin una sesión de vídeo activa: crean y retiran pantallas de prueba. La segunda requiere el host de `target/debug` compilado, FFmpeg en `PATH` y el puerto loopback 48151 libre. Cierra exclusivamente sus procesos propios y conserva registros bajo `target/`.

Para comprobar diez ciclos de creación, escritorio activo y retirada normal, con la utilidad cerrada y en una terminal elevada:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/test-device.ps1
```

El script usa `--probe`, que comprueba el escritorio activo de 1280 × 800 a 60 Hz cinco segundos después de arrancar el driver y cierra su handle. Comprueba además presencia/retirada de adaptador y monitor, conservando los dispositivos físicos iniciales. No solicita cambios de topología ni acredita por sí solo entrega de frames. Guarda el resultado en `target/windows-idd-test/device-cycles.log`. Los diez ciclos pasaron el 6 de septiembre; las superficies se comprobaron por separado mediante IddCx. Conserva el handle de `Process` para leer correctamente `ExitCode` en Windows PowerShell.

Con el paquete firmado e instalado:

1. Anotar las pantallas activas antes de ejecutar `--run`.
2. Confirmar un único monitor adicional, 1280 × 800 a 60 Hz, y ausencia de errores en el Administrador de dispositivos.
3. Extender el escritorio y mover una ventana de prueba al monitor virtual. Esta iteración no permite verla en la Deck; recuperarla con `Win+Mayús+Flecha` si hace falta.
4. Pulsar X y comprobar que el monitor adicional desaparece sin retirar los monitores físicos.
5. Repetir diez ciclos de activación/retirada; comprobar también cerrar el proceso y arrancar una segunda instancia mientras la primera sigue abierta. No debe aparecer un segundo monitor virtual.
6. Registrar tiempos, errores PnP y cualquier ventana inaccesible. Suspensión, bloqueo y cambio de usuario necesitan pruebas posteriores; no se consideran validados.

## Desinstalar únicamente este prototipo

Cerrar primero la utilidad y esperar a que desaparezca la pantalla. En una terminal elevada, listar los paquetes y comprobar **proveedor WindowDeck y nombre original WindowDeckDisplay.inf**:

```powershell
pnputil /enum-drivers /class Display
```

Usar exclusivamente el nombre publicado de ese paquete; `oemNN.inf` es un marcador, no un nombre que deba copiarse literalmente:

```powershell
pnputil /delete-driver oemNN.inf /uninstall
```

No usar comodines, `/force` ni eliminar drivers de la GPU. Si Windows indica que está en uso, no forzar: comprobar el proceso y atender el reinicio que solicite.

Después de desinstalar el prototipo, para retirar también su confianza, comprobar primero que la huella coincide con el certificado que imprimió la instalación. Para **esta instalación local**, los comandos elevados son:

```powershell
Get-Item Cert:/LocalMachine/Root/FFE0706D3221825F41678E40CE25727D938F101C
Get-Item Cert:/LocalMachine/TrustedPublisher/FFE0706D3221825F41678E40CE25727D938F101C
Remove-Item -LiteralPath Cert:/LocalMachine/Root/FFE0706D3221825F41678E40CE25727D938F101C
Remove-Item -LiteralPath Cert:/LocalMachine/TrustedPublisher/FFE0706D3221825F41678E40CE25727D938F101C
```

No copiarlos para otro certificado. Esto conserva la clave de desarrollo en CurrentUser/My y la copia pública local, pero retira la confianza añadida al equipo. Volver a ejecutar `install-test.ps1` restauraría esa confianza.

## Licencia

El ejemplo fijado usa **Microsoft Public License (MS-PL)**, no MIT. Esta adaptación conserva los avisos de Microsoft y se distribuye bajo la [licencia incluida](LICENSE). Esta excepción afecta a `driver/windows-idd/`; el workspace Rust conserva su licencia MIT OR Apache-2.0.
