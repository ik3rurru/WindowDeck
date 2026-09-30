# WindowDeck para Steam Deck

En modo escritorio, extrae todo este paquete y abre una terminal en su carpeta:

```bash
bash install-steamdeck.sh
```

El instalador instala o actualiza el Flatpak para tu usuario, crea el acceso
**WindowDeck** en tu escritorio y deja el ejecutable `~/.local/bin/windowdeck`.
No necesita sudo. La primera instalación necesita Internet para el runtime de
Flathub. Si cancelas o falla Flatpak, no crea los accesos.

Para usarlo en modo juego, abre Steam → **Añadir un producto** → **Añadir un
producto que no es de Steam** y selecciona **WindowDeck**. Si no aparece, pulsa
**Buscar**, muestra todos los archivos y selecciona
`/home/deck/.local/bin/windowdeck`. Desactiva cualquier compatibilidad forzada
con Proton: este cliente es Linux nativo. En modo juego aparecerá en **Fuera de
Steam**. Abre primero WindowDeck en el PC, elige **Extender escritorio** o
**Duplicar pantalla principal** y pulsa **Iniciar**. El cliente funciona igual
en ambos modos; solo extensión requiere el driver virtual de Windows.

En **Duplicar**, el perfil automático usa la salida disponible al abrir el
cliente, hasta 2560 × 1440. Un escritorio 16:9 llega a 1280 × 720 en la pantalla
integrada; en una TV 4K puede llegar a 2560 × 1440 y ampliarse hasta la salida.
El host nunca amplía una fuente menor. Ambos extremos deben estar actualizados.
Conecta la TV antes de abrir WindowDeck; si la conectas durante la sesión,
cierra y abre el cliente para renegociar la calidad con la nueva salida.
Si al conectar la TV la imagen sigue rodeada de bandas, comprueba que Steam
ofrezca al juego la resolución de la pantalla externa (opción **Nativa**, si
está disponible). El registro `native_display output=...` indica el tamaño
que Gamescope entrega realmente a WindowDeck.

Para fijar un perfil de duplicación, añade `--quality deck`, `--quality 1080p`
o `--quality 1440p` a las opciones del cliente. Por ejemplo:

```bash
flatpak run io.github.ik3rurru.WindowDeck --fullscreen --quality 1080p
```

El perfil indica un máximo, conservando proporción y tamaño de la fuente.
`--quality auto` restaura la selección automática. Una resolución mayor exige
más red y trabajo de codificación; prueba 1080p o Deck si aparecen cortes.
Extender escritorio mantiene su modo de 1280 × 800.

Si añades el destino manualmente, también puedes utilizar `/usr/bin/flatpak`
con estas opciones de lanzamiento:

```text
run io.github.ik3rurru.WindowDeck --fullscreen
```

Al abrir solo `WindowDeck.flatpak` en Discover se instala la aplicación y su
entrada del menú. Para crear también el acceso del escritorio, ejecuta:

```bash
bash install-steamdeck.sh --shortcuts-only
```

El escritorio se localiza con `xdg-user-dir DESKTOP`, incluso si se llama
`Escritorio` o contiene espacios. Si está desactivado, la aplicación y el
lanzador siguen disponibles sin crear un escritorio nuevo.

Para actualizar, descarga la siguiente versión y vuelve a ejecutar su
instalador. El destino de Steam conserva su ruta. Para desinstalar:

```bash
flatpak uninstall --user io.github.ik3rurru.WindowDeck
```

Después puedes borrar el acceso WindowDeck del escritorio y
`~/.local/bin/windowdeck`, y quitar su entrada de Steam. El vídeo todavía no
está cifrado: utiliza una red local de confianza.
