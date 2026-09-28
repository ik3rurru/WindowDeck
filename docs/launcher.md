# Lanzador WindowDeck

Abrir `WindowDeck.exe` en la raíz del paquete o el acceso directo WindowDeck.
Desde el repositorio, abrir `target/release/windowdeck-launcher.exe` después de
compilar. `scripts/install-shortcut.ps1` crea o actualiza el acceso directo del
escritorio; admite `-Destination RUTA.lnk` para otro destino.

El panel Rust ofrece Iniciar, Detener y Ver registros, con dos modos elegibles
antes de iniciar:

- **Extender escritorio**, predeterminado: crea la pantalla virtual y permite
  mover ventanas a la Deck. Requiere el driver instalado y su auxiliar.
- **Duplicar pantalla principal**: captura la pantalla marcada como principal
  en Windows, incluido el cursor. No crea monitores ni necesita el driver o
  `windowdeck-display.exe`. Ajusta la imagen a 1280 × 800 sin deformarla,
  añadiendo bandas cuando la proporción sea distinta. El cliente se abre igual.

Para cambiar de modo durante una sesión, pulsa Detener primero. La selección
solo se conserva mientras el panel permanece abierto; `--mirror` permite
abrirlo con duplicación preseleccionada.

Su icono está incrustado en el ejecutable. Iniciar solicita UAC para preparar
las reglas de red; en extensión el mismo gestor arranca también los brokers
del driver. En duplicación no arranca ningún broker de pantalla. El host
conserva permisos normales. Se requieren los binarios del paquete, o los
compilados en `target/release`; extensión también utiliza `target/windows-idd`.
`scripts/build-media.ps1` prepara FFmpeg y SDL junto al host. El lanzador no
instala el driver. El broker configura sus reglas UDP 5353 y TCP 48150 para el
ejecutable actual, en redes privadas y desde la subred local. Si el puerto 48150
está ocupado, el panel muestra el error del host y permite consultar sus registros.

Abrir después el cliente en la Steam Deck. Con ambos extremos actualizados, el
panel muestra «Comprobando la estabilidad de la conexión» durante una prueba
de tráfico de unos dos segundos, antes de activar la pantalla. Si falla, muestra
el problema y conserva el monitor desactivado. «Deck conectada» indica que el
host ha empezado a enviar vídeo; no confirma su presentación física en la Deck.
Tres fallos de arranque seguidos pausan los reintentos. Revisa la red y los
registros; pulsa Detener e Iniciar para volver a probar. Los clientes anteriores
conservan compatibilidad, pero no realizan la prueba previa de tráfico.
Diseño y límites en [ADR 0020](adr/0020-connection-validation.md).
Detener o cerrar solicita al host que termine y, en extensión, libere el monitor, con ocho
segundos de margen antes de terminar su grupo de procesos. Después recoge los
brokers. La interfaz sigue respondiendo durante esta espera. Cancelar un inicio
invalida la solicitud pendiente, incluso si se concede UAC después. Solo administra
sus propios procesos; los grupos de Windows también recogen los descendientes
si el panel termina inesperadamente.

Los registros se guardan en `%LOCALAPPDATA%/WindowDeck/logs/launcher-<identificador>/`:
`host.out` contiene estados, `host.log` el diagnóstico, `broker.log`/`broker.err`
los del auxiliar y `firewall-*.log` la preparación de las reglas. `launcher-error.txt`
conserva el fallo de la sesión. `WINDOWDECK_LOG_DIR` permite otro destino de pruebas.

Acceso directo para la Deck (`~/Desktop/WindowDeck.desktop`, ejecutable):

```ini
[Desktop Entry]
Type=Application
Name=WindowDeck
Exec=flatpak run io.github.ik3rurru.WindowDeck --fullscreen
Icon=io.github.ik3rurru.WindowDeck
Terminal=false
```

El cliente descubre el PC mediante mDNS; no requiere una IP fija.
El paquete Steam Deck incluye `install-steamdeck.sh`, que instala el Flatpak
y crea automáticamente el acceso en el escritorio XDG y el ejecutable
`~/.local/bin/windowdeck` para Steam. También permite `--shortcuts-only`.
Consulta la [guía de instalación y modo juego](steamdeck-distribution.md).

Validación: pruebas Rust, Clippy y arnés de apertura/cierre del panel, incluida
captura y controles accesibles. El 12 de septiembre se comprobó Iniciar/UAC/
conectar/Detener con vídeo CPU en la Deck, además del cierre inesperado y la
reconexión. El usuario aceptó latencia y retorno de ventanas. La cancelación real
de UAC, otras escalas y la instalación limpia siguen pendientes; resultados en
[testing.md](testing.md). La decisión de interfaz está en [ADR 0018](adr/0018-rust-launcher.md).

Consolidacion de 0.2.0: el panel usa extensión CPU/libx264 por defecto, incluso si se compila
la integracion multimedia. `--native` selecciona expresamente la extensión GPU experimental
y exige `native-media`; en ese modo mantiene ambos brokers para aceptar tambien
clientes antiguos. Se retira `-Legacy`: abrir el panel sin switches utiliza CPU.
No consulta `Get-NetTCPConnection`; los estados proceden del host.
Vease [implementacion, pruebas y limites](mejoras-implementadas.md).

El 13 de septiembre se retiraron el `.vbs` y el panel PowerShell al preparar la
distribución del lanzador Rust aceptado en la Deck. Sus fuentes siguen en el
historial Git; la instalación limpia conserva su prueba pendiente.

La duplicación utiliza Windows Graphics Capture y reduce la imagen en GPU antes
de codificar con libx264. Conserva la negociación H.264 integrada y MPEG-TS para
clientes anteriores. No se ha medido aún su latencia en la Deck. El bloqueo de
Windows, la desconexión del monitor o el cambio de pantalla principal pueden
cerrar la captura; la reconexión vuelve a seleccionar la principal. Ver
[ADR 0021](adr/0021-primary-screen-mirroring.md).
