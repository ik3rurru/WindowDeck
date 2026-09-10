# Integración y distribución de FFmpeg

Fecha: 10 de septiembre de 2026. Estado: aceptado para el paquete del prototipo;
validación en instalación limpia pendiente.

## Contexto

La ruta soportada usa FFmpeg/libx264 en el host y FFplay en el cliente. Exigir
su instalación manual fue útil en el prototipo, pero no es un requisito de uso
del paquete Windows actual. `scripts/build-media.ps1` fija FFmpeg 9.0.1 y SDL
2.32.10 con SHA256 y copia ejecutables y DLL junto al host. El Flatpak obtiene
las dependencias multimedia mediante su runtime.

`windowdeck-media` ya integra FFmpeg y SDL mediante un límite FFI Rust/C++
acotado. Su función `native-media` es opcional para host y cliente; compilarla
no sustituye la aceptación de la ruta completa en hardware real.

## Decisión

Distribuir los ejecutables y DLL junto a WindowDeck. El lanzador prepara el PATH
de sus procesos hijos sin modificar el del usuario o del sistema. El paquete
incluye manifiesto de versiones/hashes, fuentes y licencias. La instalación del
driver se mantiene separada.

Conservar una sola integración nativa, `windowdeck-media`. Introducir además
`ffmpeg-next` duplicaría el límite con las mismas bibliotecas. Una sustitución
futura debe demostrar reducción de mantenimiento y preservar las pruebas de
captura, color, decodificación y recuperación.

FFmpeg se mantiene fuera del driver para permitir actualizar y aislar sus
dependencias y fallos. El reparto CPU/GPU se decide por mediciones del recorrido
completo, no por el número de procesos o de lenguajes.

## Licencias y límites

La distribución de FFmpeg utilizada incluye libx264/GPLv3. Se conservan su
licencia, identificación y referencia a fuentes, además de SDL y las licencias
del proyecto; la licencia MIT/Apache del código Rust no sustituye las de los
componentes distribuidos. Véase el inventario del paquete.

El script de empaquetado no instala ni firma el driver. La prueba en este equipo
no demuestra que funcione en una instalación limpia: falta validar doble clic,
dependencias, conexión y cierre en Windows sin FFmpeg ni Rust previamente
instalados. Ese criterio sigue abierto en el bloque 5 de consolidación.
