# Lanzador WindowDeck

Abrir `WindowDeck.exe` en la raíz del paquete o el acceso directo WindowDeck.
Desde el repositorio, abrir `target/release/windowdeck-launcher.exe` después de
compilar. `scripts/install-shortcut.ps1` crea o actualiza el acceso directo del
escritorio; admite `-Destination RUTA.lnk` para otro destino.

El panel Rust ofrece Iniciar, Detener y Ver registros. Su icono está incrustado
en el ejecutable. Iniciar solicita UAC solo para el gestor del broker; el host
conserva permisos normales. Se requieren el driver instalado y los binarios del
paquete, o los compilados en `target/release` y `target/windows-idd`.
`scripts/build-media.ps1` prepara FFmpeg y SDL junto al host. El lanzador no
instala el driver. El broker configura sus reglas UDP 5353 y TCP 48150 para el
ejecutable actual, en redes privadas y desde la subred local. Si el puerto 48150
está ocupado, el panel muestra el error del host y permite consultar sus registros.

Abrir despues el cliente en la Steam Deck. El panel recibe estados de negociacion
y sesion del host; no confirma el barrido fisico del primer frame en la Deck.
Detener o cerrar solicita al host que termine y libere el monitor, con ocho
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

Consolidacion de 0.2.0: el panel usa CPU/libx264 por defecto, incluso si se compila
la integracion multimedia. `--native` selecciona expresamente la ruta GPU experimental
y exige `native-media`; en ese modo mantiene ambos brokers para aceptar tambien
clientes antiguos. Se retira `-Legacy`: abrir el panel sin switches utiliza CPU.
No consulta `Get-NetTCPConnection`; los estados proceden del host.
Vease [implementacion, pruebas y limites](mejoras-implementadas.md).

El 13 de septiembre se retiraron el `.vbs` y el panel PowerShell al preparar la
distribución del lanzador Rust aceptado en la Deck. Sus fuentes siguen en el
historial Git; la instalación limpia conserva su prueba pendiente.
