# ADR 0018 — Panel y gestión de procesos en Rust

Fecha: 2026-09-11, actualizado el 12. Estado: implementado y probado con vídeo
CPU en la Deck; cancelación real de UAC, escalas adicionales e instalación
limpia pendientes.

## Contexto

`WindowDeck.vbs` abría un panel WinForms en PowerShell. La consolidación de
la CLI permite trasladar la interfaz y la administración de sesiones a un
binario del workspace, conservando CPU/libx264 como ruta normal.

Se evaluaron las API de [native-windows-gui](https://gabdube.github.io/native-windows-gui/)
y [egui/eframe](https://github.com/emilk/egui). NWG utiliza los controles de Windows;
eframe integra una interfaz dibujada y un backend gráfico. Para tres botones y
dos etiquetas se elige NWG con sus funciones de temporizador, recursos y DPI.
El cliente multimedia sigue siendo independiente de este panel.

El prototipo NWG se comprobó a 96 DPI: 16 ms desde el comienzo de `run` hasta
crear los controles, 311 ms hasta observarlos desde el arnés (incluye instrumentación),
18.382.848 bytes de working set y ejecutable release de 630.784 bytes. Se verificaron
la captura, los controles accesibles, la exclusión de una segunda instancia y
el cierre normal. Estas medidas corresponden a un ensayo local; no son una
comparación de rendimiento con eframe, que se evaluó por su arquitectura.

## Decisión

- `windowdeck-launcher` hereda los lints del workspace, incluido `unsafe_code = forbid`.
  Las dependencias encapsulan las API de Windows. En Linux, ayuda, versión y
  pruebas de CLI siguen compilando; abrir el panel informa que requiere Windows.
- El ejecutable contiene icono, manifiesto `asInvoker` y conciencia de DPI de
  sistema. NWG escala las coordenadas lógicas. No se anuncia soporte por monitor;
  faltan ensayos a 150/200 % y al mover entre monitores de escalas distintas.
- El hilo de interfaz procesa eventos y mensajes; UAC, validación de ejecutables,
  firewall, lectura del estado del host y espera de procesos ocurren fuera de él.
  El panel no consulta conexiones TCP para inferir el estado de la Deck.
- El gestor elevado es el mismo ejecutable con un modo interno `--broker`.
  [runas](https://docs.rs/runas/latest/runas/) solicita elevación sin bloquear la GUI.
  Un socket en `127.0.0.1`, con puerto efímero y capacidad aleatoria de 128 bits,
  vincula su vida al panel. No se reutilizan PID ni se terminan procesos por nombre.
  Cancelar antes de aceptar UAC cierra el listener; aceptar tarde no puede iniciar
  una sesión. Esta capacidad local no proporciona autenticación al vídeo de la LAN.
- El gestor utiliza `netsh` para las dos reglas de firewall de WindowDeck, con
  dirección de entrada, perfil privado, subred local y ejecutable específico.
  Conserva los nombres visibles del panel anterior. No necesita ejecutar PowerShell.
- CPU abre `--frame-broker`; `--native` abre GPU y CPU para mantener la negociación
  de clientes anteriores. El host conserva permisos normales y recibe un PATH
  privado con el directorio de sus DLL y FFmpeg.
- [command-group](https://docs.rs/command-group/latest/command_group/) crea Job Objects
  con terminación al cerrar su handle. Cubren el host, sus descendientes y los
  brokers propios incluso ante terminación del panel/gestor. La parada habitual
  escribe `WINDOWDECK_STOP_FILE`, espera hasta ocho segundos al host y después
  cierra el broker. Perder el socket concede al broker el mismo margen de limpieza.
- El paquete abre `WindowDeck.exe`; en desarrollo se abre
  `target/release/windowdeck-launcher.exe`. Los registros van al perfil del usuario
  para no exigir permisos de escritura en la carpeta instalada.

## Consecuencias y aceptación

El nuevo crate queda cubierto por formato, Clippy y pruebas del workspace.
`scripts/test-launcher.ps1` automatiza el prototipo visual en Windows sin iniciar
una sesión. `scripts/install-shortcut.ps1` actualiza los accesos directos y el
paquete incluye versión/hash del launcher. El 13 de septiembre, tras aceptar
el ciclo con vídeo y solicitar la limpieza para publicar, se retiraron el
`.vbs` y el panel anterior. Se conservan en el historial Git. La instalación
limpia sigue pendiente y no se considera validada por esa retirada.

Además del prototipo visual, `-Session` comprobó Iniciar/UAC/espera/Detener con
el host real; `-Session -TerminatePanel` verificó la recogida tras terminar el
panel. No se abrió un cliente ni se activó el monitor. Estas pruebas no sustituyen
Iniciar/UAC/conectar/Detener con vídeo en la Deck, cancelación de UAC real ni
retirada de un monitor activo después de un fallo. Los resultados y aceptaciones
pendientes se registran en [testing.md](../testing.md).

El 12 de septiembre se completó el ciclo con vídeo CPU y el reproductor integrado
del ADR 0019. El usuario aceptó la latencia y el retorno de ventanas al detenerse.
También se verificó retirada del monitor activo al terminar el panel, recogida
de procesos y reconexión del mismo cliente tras reabrirlo. La instalación limpia
y los otros casos pendientes mantienen su propio alcance de aceptación.
