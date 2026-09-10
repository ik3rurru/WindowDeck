# WindowDeck para Windows

1. Descomprime el paquete en una carpeta propia.
2. Con el driver de WindowDeck instalado, abre `WindowDeck.vbs` sin ejecutar
   como administrador. Pulsa **Iniciar** y acepta el permiso del broker.
3. Abre WindowDeck en el modo escritorio de la Steam Deck.
4. Cierra el cliente o pulsa **Detener** para retirar la segunda pantalla.

La ruta CPU H.264 es la predeterminada y está comprobada en la Deck.
El paquete incluye FFmpeg y las DLL necesarias; no exige configurar variables
de entorno ni instalar FFmpeg por separado. El driver se instala por separado
siguiendo `source/driver/windows-idd/README.md`.

**El vídeo y el control todavía no están cifrados y no hay emparejamiento.**
Utiliza el prototipo en una LAN de confianza.

**Ver registros** abre los registros de la sesión. La carpeta `licenses`
contiene las licencias de las dependencias, `manifest.json` identifica versiones
y hashes, y `source` conserva el código y la documentación de desarrollo.

La GPU es experimental y solo se activa de forma explícita desde las herramientas
de desarrollo. La migración del panel a un ejecutable Rust está pendiente.
