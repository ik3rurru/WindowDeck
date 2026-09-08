# Lanzador WindowDeck

Abrir `WindowDeck.vbs` en la raiz del proyecto o el acceso directo WindowDeck
del escritorio de Windows. El panel ofrece Iniciar, Detener y Ver registros.
Iniciar solicita UAC solo para el broker; el host conserva permisos normales.
Se requieren el driver instalado, FFmpeg en PATH y los binarios compilados en
`target/debug` y `target/windows-idd`. El lanzador no instala el driver ni cambia
las reglas TCP existentes. El broker configura descubrimiento UDP 5353 para el host en redes privadas locales. Si el puerto 48150 esta ocupado, cerrar la prueba anterior.

Abrir despues el cliente en la Steam Deck. El panel indica conexion TCP, no
confirma que el reproductor haya presentado el primer frame. Detener o cerrar
el panel termina su host y solicita el cierre de su broker, con ocho segundos
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
