# WindowDeck para Windows

1. Descomprime el paquete en una carpeta propia.
2. Abre `WindowDeck.exe` sin ejecutar como administrador. Elige
   **Extender escritorio** o **Duplicar pantalla principal**, pulsa **Iniciar**
   y acepta el permiso para preparar la conexión.
3. Abre WindowDeck en el modo escritorio de la Steam Deck.
4. Cierra el cliente o pulsa **Detener** para terminar la transmisión. En
   extensión también se retira la pantalla virtual y se recuperan las ventanas.

La extensión CPU H.264 es la predeterminada y requiere el driver virtual.
La duplicación muestra la pantalla principal de Windows, incluido el cursor,
sin instalar ni activar ese driver. Conserva la proporción al ajustar la imagen
a 1280 × 800. Para cambiar de modo, pulsa Detener primero.
Utiliza el reproductor integrado del Flatpak nuevo; los clientes anteriores
siguen funcionando mediante MPEG-TS.
El paquete incluye FFmpeg y las DLL necesarias; no exige configurar variables
de entorno ni instalar FFmpeg por separado. Si vas a extender el escritorio,
instala el driver siguiendo `source/driver/windows-idd/README.md`. En duplicación,
el permiso UAC solo prepara las reglas de red y no arranca brokers de pantalla.

**El vídeo y el control todavía no están cifrados y no hay emparejamiento.**
Utiliza el prototipo en una LAN de confianza.

**Ver registros** abre los registros de la sesión, guardados en
`%LOCALAPPDATA%/WindowDeck/logs/`. `scripts/install-shortcut.ps1` crea o actualiza
el acceso directo del escritorio para abrir `WindowDeck.exe`. La carpeta `licenses`
contiene las licencias de las dependencias, `manifest.json` identifica versiones
y hashes, y `source` conserva el código y la documentación de desarrollo.

La codificación GPU del driver es experimental y se activa explícitamente con
`WindowDeck.exe --native`. `WindowDeck.exe --mirror` preselecciona duplicación.
El panel y su gestor de procesos están en Rust. La aplicación se abre mediante
`WindowDeck.exe`; no distribuye el panel anterior de PowerShell/VBS.
