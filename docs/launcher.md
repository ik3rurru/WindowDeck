# Lanzador WindowDeck

Abrir `WindowDeck.vbs` en la raiz del proyecto o el acceso directo WindowDeck
del escritorio de Windows. El panel ofrece Iniciar, Detener y Ver registros.
El icono de la ventana y del acceso directo procede de `assets/WindowDeck.ico`.
Iniciar solicita UAC solo para el broker; el host conserva permisos normales.
Se requieren el driver instalado y los binarios del paquete, o los compilados en
`target/release` y `target/windows-idd`. `scripts/build-media.ps1` prepara FFmpeg
y SDL junto al host. El lanzador no instala el driver. El broker configura sus
reglas UDP 5353 y TCP 48150 para el ejecutable actual, en redes privadas locales.
Si el puerto 48150 esta ocupado, cerrar la prueba anterior.

Abrir despues el cliente en la Steam Deck. El panel recibe estados de negociacion
y sesion del host; no confirma el barrido fisico del primer frame en la Deck.
Detener o cerrar solicita al host que termine y libere el monitor, con ocho segundos
para liberar el monitor. Solo administra sus propios procesos. Los registros
se conservan en `target/launcher-<identificador>/`.

Acceso directo para la Deck (`~/Desktop/WindowDeck.desktop`, ejecutable):

```ini
[Desktop Entry]
Type=Application
Name=WindowDeck
Exec=flatpak run io.github.ik3rurru.WindowDeck --h264-test --fullscreen
Icon=video-display
Terminal=false
```

El cliente nuevo descubre el PC mediante mDNS; no requiere una IP fija. El acceso directo ya esta instalado
en `/home/deck/Desktop/WindowDeck.desktop`, con permiso de ejecucion y validado
con `desktop-file-validate`. Su copia reutilizable esta en
`packaging/WindowDeck.desktop`. Abrir primero el host en Windows y despues este
acceso directo en el modo escritorio de la Deck.

Validacion: analisis sintactico PowerShell sin errores. Pendiente comprobar
interactivamente el ciclo completo Iniciar/conectar/Detener con la Deck.

Version 0.2.0: el panel usa la integracion multimedia si esta compilada. Mantiene
ambos brokers para aceptar tambien clientes antiguos. `-Legacy` fuerza CPU y
FFplay. No consulta `Get-NetTCPConnection`; los estados proceden del host.
Vease [implementacion, pruebas y limites](mejoras-implementadas.md).
