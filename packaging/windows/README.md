# WindowDeck para Windows

1. Descomprime el paquete en una carpeta propia.
2. Con el driver de WindowDeck instalado, abre `WindowDeck.exe` sin ejecutar
   como administrador. Pulsa **Iniciar** y acepta el permiso del broker.
3. Abre WindowDeck en el modo escritorio de la Steam Deck.
4. Cierra el cliente o pulsa **Detener** para retirar la segunda pantalla.

La ruta CPU H.264 es la predeterminada y está comprobada en la Deck.
Utiliza el reproductor integrado del Flatpak nuevo; los clientes anteriores
siguen funcionando mediante MPEG-TS.
El paquete incluye FFmpeg y las DLL necesarias; no exige configurar variables
de entorno ni instalar FFmpeg por separado. El driver se instala por separado
siguiendo `source/driver/windows-idd/README.md`.

**El vídeo y el control todavía no están cifrados y no hay emparejamiento.**
Utiliza el prototipo en una LAN de confianza.

**Ver registros** abre los registros de la sesión, guardados en
`%LOCALAPPDATA%/WindowDeck/logs/`. `scripts/install-shortcut.ps1` crea o actualiza
el acceso directo del escritorio para abrir `WindowDeck.exe`. La carpeta `licenses`
contiene las licencias de las dependencias, `manifest.json` identifica versiones
y hashes, y `source` conserva el código y la documentación de desarrollo.

La GPU es experimental y se activa explícitamente con `WindowDeck.exe --native`.
El panel y su gestor de procesos están en Rust. La aplicación se abre mediante
`WindowDeck.exe`; no distribuye el panel anterior de PowerShell/VBS.
